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
    """Execute (single or batch) prompt enforcing flat row protocol.

    Returns JSON: {success: bool, data|error, raw_response}.
    For legacy single-row payloads (row_data only) returns data = list[str].
    For batch (rows_data) returns data = list[list[str]].
    """
    def make_err(msg: str, raw: str | None = None) -> str:
        return json.dumps({"success": False, "error": msg, "raw_response": raw or msg}, ensure_ascii=False)

    try:
        if not api_key:
            try:
                import keyring  # type: ignore
                api_key = keyring.get_password("GoogleGeminiAPI", os.getlogin()) or ""
            except ImportError:
                return make_err("API key missing and keyring not installed")
            if not api_key:
                return make_err("API key missing")

        try:
            payload: Dict[str, Any] = json.loads(payload_json)
        except Exception as e:  # pragma: no cover
            return make_err(f"Invalid payload JSON: {e}")

        model_id = payload.get("ai_model_id", "gemini-2.5-pro-preview-06-05")
        system_instruction = payload.get("general_sheet_rule")
        rows_data: List[List[Any]] = payload.get("rows_data") or []
        legacy_single = False
        if not rows_data:
            single_row = payload.get("row_data", [])
            if single_row:
                rows_data = [single_row]
                legacy_single = True
        column_contexts: List[Any] = payload.get("column_contexts", [])
        allow_row_additions: bool = payload.get("allow_row_additions", False)
        orig_n = len(rows_data)
        if orig_n == 0:
            return json.dumps({"success": True, "data": [] if legacy_single else [], "raw_response": "No rows provided"}, ensure_ascii=False)

        ordering = (
            f"There are {orig_n} original rows. Output a JSON array of row arrays. "
            f"First {orig_n} arrays must correspond 1:1 & in order to originals. "
            + ("You may append new rows after originals." if allow_row_additions and not legacy_single else "Do not add extra rows.")
            + " Row length must equal number of column contexts. No markdown fences."
        )
        system_full = system_instruction + "\n" + ordering if system_instruction else ordering

        contents = [
            genai.types.Content(role="model", parts=[genai.types.Part.from_text(text=system_full)]),
            genai.types.Content(role="user", parts=[genai.types.Part.from_text(text=(
                "Column Contexts:" + json.dumps(column_contexts, ensure_ascii=False) + "\n" +
                "Rows Data:" + json.dumps(rows_data, ensure_ascii=False) + "\n" +
                "Return ONLY JSON."))])
        ]

        cfg: Dict[str, Any] = {}
        if payload.get("ai_temperature") is not None:
            cfg["temperature"] = payload["ai_temperature"]
        cfg["tools"] = [Tool(google_search=GoogleSearch())]  # always enable search tool for now

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

        # ---------------- Parsing & Repair Pipeline -----------------
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
            # Remove leading junk before first '[' or '{'
            m = re.search(r'[\[{].*', candidate, re.DOTALL)
            if m:
                candidate = m.group(0)
            # Strip trailing code fences / stray backticks
            candidate = candidate.strip('` \n\t')
            # Insert missing comma between touching arrays/objects: '][', '}{', ']{', '}[', etc.
            candidate_comma_fix = re.sub(r'(\]|\})(\[|\{)', r'\1,\2', candidate)
            if candidate_comma_fix != candidate:
                repair_notes.append('Inserted missing commas between adjacent top-level elements')
                candidate = candidate_comma_fix
            # Replace single quotes with double if it looks like JSON-ish but not valid
            if candidate.count('"') == 0 and candidate.count("'") > 0:
                repair_notes.append('Replaced single quotes with double quotes')
                candidate = re.sub(r"'", '"', candidate)
            # Remove trailing commas before ] or }
            def remove_trailing_commas(s: str) -> str:
                return re.sub(r',\s*([\]}])', r'\1', s)
            new_candidate = remove_trailing_commas(candidate)
            if new_candidate != candidate:
                repair_notes.append('Removed trailing commas')
                candidate = new_candidate
            # If wrapped in an object with key rows|data|output|result extract list
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
                    # CSV fallback: detect if looks like rows of comma separated items without brackets
                    lines = [l.strip() for l in original_raw.splitlines() if l.strip()]
                    if lines and all(("," in l) for l in lines[: min(5, len(lines))]):
                        parsed = [[c.strip() for c in re.split(r',(?=(?:[^"]*"[^"]*")*[^"]*$)', l)] for l in lines]
                        repair_notes.append('Parsed as CSV fallback')
                    else:
                        return make_err(f"JSON decode error: {err2}", original_raw)
                else:
                    if err:
                        repair_notes.append(f'Primary parse error: {err}')
        # ---------------- End Repair Pipeline --------------------

        if not isinstance(parsed, list):
            return make_err("Top-level JSON must be an array", original_raw)

        if parsed and all(not isinstance(el, list) for el in parsed):
            # Model returned a single row directly
            parsed_rows = [parsed]
        else:
            parsed_rows = []
            for r in parsed:
                if isinstance(r, list):
                    parsed_rows.append(r)
                else:
                    return make_err(f"Non-array row element: {r}", response.text)

        if len(parsed_rows) < orig_n:
            return make_err(f"Returned {len(parsed_rows)} rows but {orig_n} required", response.text)
        if (legacy_single or not allow_row_additions) and len(parsed_rows) > orig_n:
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

        payload_out = {
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
