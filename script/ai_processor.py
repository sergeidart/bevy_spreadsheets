"""execute_ai_query.py

Re‑implements the Rust‑callable helper for Google Gemini API
using the **Google Gen AI SDK** (>= v1) and the new
*Search as a Tool* interface for Grounding Search.

Call signature is unchanged so it remains a drop‑in replacement for
`PyO3` bindings in Rust.
"""
from __future__ import annotations

import json
import os
import re

"""AI Processor

Enhanced tolerant JSON parsing:
1. Standard extraction of first JSON array/object.
2. If strict parse fails, attempts repairs:
    - Trim junk before first bracket
    - Strip stray backticks / fences
    - Convert single to double quotes when no double quotes present
    - Remove trailing commas before closing brackets
    - Extract list from wrapper object keys: rows, data, output, result
    - Fallback to CSV line parsing if looks like comma separated rows
Repair notes appended to raw_response inside [Repairs: ...] tag for UI visibility.
"""
from typing import Any, Dict, List, Tuple, Optional

from google import genai
from google.genai.types import (
    GenerateContentConfig,
    GenerationConfig,  # for completeness; used for type hints only
    GoogleSearch,
    HttpOptions,
    Tool,
)

# ---------------------------------------------------------------------------
# Public API (exposed to Rust)
# ---------------------------------------------------------------------------

def execute_ai_query(api_key: str, payload_json: str) -> str:  # noqa: D401
    """Execute batch / single / prompt-only AI query.

    Returns JSON with shape:
      { success: bool, data: list[list[str]]|list[str], raw_response: str, error?: str }
    """

    def make_err(msg: str, raw: str | None = None) -> str:
        return json.dumps({"success": False, "error": msg, "raw_response": raw or msg}, ensure_ascii=False)

    try:
        # --- API Key fallback (keyring) ---
        if not api_key:
            try:  # best-effort keyring lookup
                import keyring  # type: ignore
                api_key = keyring.get_password("GoogleGeminiAPI", os.getlogin()) or ""
            except ImportError:
                return make_err("API key missing and keyring not installed")
            if not api_key:
                return make_err("API key missing")

        # --- Parse payload ---
        try:
            payload: Dict[str, Any] = json.loads(payload_json)
        except Exception as e:  # pragma: no cover
            return make_err(f"Invalid payload JSON: {e}")

        model_id = payload.get("ai_model_id", "gemini-flash-latest")
        system_instruction = payload.get("general_sheet_rule")
        rows_data: List[List[Any]] = payload.get("rows_data") or []
        legacy_single = False
        if not rows_data:
            single_row = payload.get("row_data", [])
            if single_row:
                rows_data = [single_row]
                legacy_single = True
        user_prompt: Optional[str] = payload.get("user_prompt")
        column_contexts: List[Any] = payload.get("column_contexts", [])
        column_data_types: List[Any] = payload.get("column_data_types", [])
        keys_block = payload.get("keys")
        # Build a single key payload. Prefer an explicit `payload["key"]` if callers
        # already use it. Otherwise attempt to normalize legacy `keys` which may
        # contain headers/contexts/rows. Do NOT send the raw legacy block (with
        # headers/rows) — that caused the model to merge key rows into the table.
        key_payload: Optional[Dict[str, Any]] = None

        # 1) If caller provided explicit 'key', and it's already in normalized form,
        #    use it directly.
        if "key" in payload:
            kp = payload.get("key")
            if isinstance(kp, dict) and ("Context" in kp or "Key" in kp):
                # Keep only Context and Key fields to be safe
                key_payload = {k: kp.get(k) for k in ("Context", "Key") if kp.get(k) is not None}

        # 2) Otherwise try to normalize legacy 'keys' block to single key dict.
        if key_payload is None and keys_block and isinstance(keys_block, dict):
            # If keys_block already looks normalized, convert to minimal form.
            if "Context" in keys_block or "Key" in keys_block:
                key_payload = {k: keys_block.get(k) for k in ("Context", "Key") if keys_block.get(k) is not None}
            else:
                try:
                    headers = keys_block.get("headers") or []
                    contexts = keys_block.get("contexts") or []
                    rows = keys_block.get("rows") or []
                    # Prefer first context and first header and first row value
                    ctx = contexts[0] if contexts and isinstance(contexts, list) else None
                    header = headers[0] if headers and isinstance(headers, list) else None
                    first_row_val = None
                    if rows and isinstance(rows, list) and rows[0]:
                        # rows may be list of lists; flatten first cell
                        r0 = rows[0]
                        if isinstance(r0, list) and r0:
                            first_row_val = r0[0]
                        elif not isinstance(r0, list):
                            first_row_val = r0
                    # If we have either a context and a key value, build normalized key
                    if ctx is not None and (first_row_val is not None or header is not None):
                        # Choose value preferring first_row_val then header
                        key_val = first_row_val if first_row_val is not None else header
                        key_payload = {"Context": ctx, "Key": key_val}
                except Exception:
                    # On any error, prefer to omit key_payload rather than send
                    # the entire legacy block which may contain multiple rows.
                    key_payload = None
        allow_row_additions: bool = payload.get("allow_row_additions", False)
        orig_n = len(rows_data)
        prompt_only_mode = bool(orig_n == 0 and not legacy_single and isinstance(user_prompt, str) and user_prompt.strip())
        if orig_n == 0 and not prompt_only_mode:
            return json.dumps({"success": True, "data": [] if legacy_single else [], "raw_response": "No rows provided"}, ensure_ascii=False)

        # --- Instruction assembly ---
        if prompt_only_mode:
            ordering = (
                "Prompt-only generation request. There are 0 original rows. "
                "Output a JSON array of row arrays derived from the prompt + column contexts. "
                "Generate distinct, high-quality rows (avoid duplicates). "
                "You may output between 1 and 25 rows. Row length must equal number of column contexts. No markdown fences."
            )
        else:
            ordering = (
                f"There are {orig_n} original rows. Output a JSON array of row arrays. "
                f"First {orig_n} arrays must correspond 1:1 & in order to originals. "
                + ("You may append new rows after originals." if allow_row_additions and not legacy_single else "Do not add extra rows.")
                + " Row length must equal number of column contexts. No markdown fences."
            )
        row_additions_hint = (
            f"Add Rows Enabled: The model may add new rows AFTER the first {orig_n} original rows to provide similar item if any applicable here. Each new row must match column count."
            if allow_row_additions and not legacy_single and not prompt_only_mode else ""
        )
        grounded_search_instruction = (
            "For each row, use Google Search to verify every provided value and update it with the latest reliable data you find. "
            "If a value is missing or clearly outdated, search for it and fill it in. Prefer authoritative, recent sources. "
            "Never invent facts: leave a cell blank if after searching you cannot find a credible value."
        )
        base_instr = (system_instruction + "\n") if system_instruction else ""
        system_full = base_instr + ordering + "\n" + grounded_search_instruction + ("\n" + row_additions_hint if row_additions_hint else "")

        # key_payload was constructed above (either from payload['key'] or normalized)
        # If still None, do not send the legacy keys block to avoid confusion.

        user_text = (
            ("ALLOW_ROW_ADDITIONS: true\n" if ((allow_row_additions and not legacy_single) or prompt_only_mode) else "")
            + (f"USER_PROMPT: {user_prompt.strip()}\n" if prompt_only_mode else "")
            + "Column Contexts:" + json.dumps(column_contexts, ensure_ascii=False) + "\n"
            + "Column Types:" + json.dumps(column_data_types, ensure_ascii=False) + "\n"
            + ("Key:" + json.dumps(key_payload, ensure_ascii=False) + "\n" if key_payload else "")
            + ("Rows Data:" + json.dumps(rows_data, ensure_ascii=False) + "\n" if not prompt_only_mode else "")
            + (row_additions_hint + "\n" if row_additions_hint and not prompt_only_mode else "")
            + "Return ONLY JSON."
        )

        contents = [
            genai.types.Content(role="model", parts=[genai.types.Part.from_text(text=system_full)]),
            genai.types.Content(role="user", parts=[genai.types.Part.from_text(text=user_text)]),
        ]
        cfg: Dict[str, Any] = {}
        if payload.get("ai_temperature") is not None:
            cfg["temperature"] = payload["ai_temperature"]
        cfg["tools"] = [Tool(google_search=GoogleSearch())]
        cfg["system_instruction"] = genai.types.Content(
            role="model", parts=[genai.types.Part.from_text(text=system_full)]
        )

        client = genai.Client(api_key=api_key, http_options=HttpOptions())
        response = client.models.generate_content(
            model=model_id,
            contents=contents,
            config=GenerateContentConfig(**cfg),
        )
        raw_text = (response.text or "").strip()
        if raw_text.startswith("```json"):
            raw_text = raw_text[7:].strip()
        if raw_text.startswith("```"):
            raw_text = raw_text[3:].strip()
        if raw_text.endswith("```"):
            raw_text = raw_text[:-3].strip()

        # --- Parse & repair ---
        original_raw = raw_text
        extracted = extract_first_json(raw_text)
        repair_notes: List[str] = []

        def attempt_parse(txt: str) -> Tuple[Optional[Any], Optional[str]]:
            try:
                return json.loads(txt), None
            except json.JSONDecodeError as je:
                return None, str(je)

        parsed, err = attempt_parse(extracted)
        if parsed is None:
            candidate = extracted
            m = re.search(r'[\[{].*', candidate, re.DOTALL)
            if m:
                candidate = m.group(0)
            candidate = candidate.strip('` \n\t')
            candidate_comma_fix = re.sub(r'(\]|\})(\[|\{)', r'\1,\2', candidate)
            if candidate_comma_fix != candidate:
                repair_notes.append('Inserted missing commas between adjacent top-level elements')
                candidate = candidate_comma_fix
            if candidate.count('"') == 0 and candidate.count("'") > 0:
                repair_notes.append('Replaced single quotes with double quotes')
                candidate = re.sub(r"'", '"', candidate)
            def remove_trailing_commas(s: str) -> str:
                return re.sub(r',\s*([\]}])', r'\1', s)
            new_candidate = remove_trailing_commas(candidate)
            if new_candidate != candidate:
                repair_notes.append('Removed trailing commas')
                candidate = new_candidate
            obj_match = re.match(r'^\{.*\}$', candidate, re.DOTALL)
            if obj_match:
                try:
                    tmp = json.loads(candidate)
                    for k in ('rows', 'data', 'output', 'result'):
                        if k in tmp and isinstance(tmp[k], list):
                            parsed = tmp[k]
                            repair_notes.append(f'Extracted list from key "{k}"')
                            break
                except Exception:
                    pass
            if parsed is None:
                parsed, err2 = attempt_parse(candidate)
                if parsed is None and err2:
                    lines = [l.strip() for l in original_raw.splitlines() if l.strip()]
                    if lines and all(("," in l) for l in lines[:min(5, len(lines))]):
                        parsed = [[c.strip() for c in re.split(r',(?=(?:[^"]*"[^"]*")*[^"]*$)', l)] for l in lines]
                        repair_notes.append('Parsed as CSV fallback')
                    else:
                        return make_err(f"JSON decode error: {err2}", original_raw)
                else:
                    if err:
                        repair_notes.append(f'Primary parse error: {err}')

        if not isinstance(parsed, list):
            return make_err("Top-level JSON must be an array", original_raw)

        if parsed and all(not isinstance(el, list) for el in parsed):
            parsed_rows = [parsed]
        else:
            parsed_rows = []
            for r in parsed:
                if isinstance(r, list):
                    parsed_rows.append(r)
                else:
                    return make_err(f"Non-array row element: {r}", response.text)

        if not prompt_only_mode and len(parsed_rows) < orig_n:
            return make_err(f"Returned {len(parsed_rows)} rows but {orig_n} required", response.text)
        if not prompt_only_mode and (legacy_single or not allow_row_additions) and len(parsed_rows) > orig_n:
            parsed_rows = parsed_rows[:orig_n]

        expected_len = len(column_contexts)
        norm_rows: List[List[str]] = []
        for r in parsed_rows:
            cells = ["" if c is None else str(c) for c in r]
            if expected_len > 0:
                if len(cells) < expected_len:
                    cells.extend(["" for _ in range(expected_len - len(cells))])
                elif len(cells) > expected_len:
                    cells = cells[:expected_len]
            norm_rows.append(cells)

        payload_out: Dict[str, Any] = {
            "success": True,
            "raw_response": original_raw + (f"\n[Repairs: {'; '.join(repair_notes)}]" if repair_notes else "")
        }
        if legacy_single:
            payload_out["data"] = norm_rows[0] if norm_rows else []
        else:
            payload_out["data"] = norm_rows
        return json.dumps(payload_out, ensure_ascii=False)
    except Exception as e:  # pragma: no cover
        return make_err(f"Unhandled exception: {e}")

def extract_first_json(text: str) -> str:
    """Extract the first balanced top-level JSON array or object.

    Previous regex approach failed on nested arrays (stopped at first ]). This
    implementation walks the string, tracking bracket depth while respecting
    string literals and escapes. Returns the balanced slice or original text
    if no complete structure found.
    """
    start_idx = None
    stack_char = None
    depth = 0
    in_string = False
    escape = False
    for i, ch in enumerate(text):
        if start_idx is None:
            if ch in '[{':
                start_idx = i
                stack_char = ch
                depth = 1
            continue
        # Inside candidate
        if in_string:
            if escape:
                escape = False
            elif ch == '\\':
                escape = True
            elif ch == '"':
                in_string = False
            continue
        else:
            if ch == '"':
                in_string = True
                continue
            if ch in '[{':
                depth += 1
            elif ch in ']}':
                depth -= 1
                if depth == 0:
                    # Return slice
                    return text[start_idx:i+1]
    # Fallback
    return text
