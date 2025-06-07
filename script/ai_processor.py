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
from typing import Any, Dict, List

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
    """Execute a single prompt against a Gemini model.

    Parameters
    ----------
    api_key
        Gemini Developer API key (see https://ai.google.dev/gemini-api/docs/api-keys).
    payload_json
        JSON string produced by the Rust side.
        Expected schema (keys not listed are ignored):
            ai_model_id:   str  – default: ``"gemini-2.5-flash"``
            general_sheet_rule: str | None – optional system instruction
            row_data:      List[Any]
            column_contexts: List[Any]
            ai_temperature: float | None
            ai_top_p:      float | None
            ai_top_k:      int   | None
            requested_grounding_with_Google Search: bool – enable Search tool.

    Returns
    -------
    str
        JSON string encoding either a successful or failed result.  Shape:
        ``{"success": bool, "data"|"error": ..., "raw_response": str}``.
    """

    try:
        # If api_key is empty, try to fetch from Windows Credential Manager
        if not api_key:
            try:
                import keyring
                api_key = keyring.get_password("GoogleGeminiAPI", os.getlogin())
                if not api_key:
                    raise ValueError("API key not found in Windows Credential Manager under 'GoogleGeminiAPI'.")
            except ImportError:
                raise ImportError("keyring package not installed. Install with 'pip install keyring'.")

        client = genai.Client(
            api_key=api_key,
            http_options=HttpOptions(),  # explicit, future‑proof
        )

        payload: Dict[str, Any] = json.loads(payload_json)

        # -------- Prompt Construction --------------------------------------------------
        model_id: str = payload.get("ai_model_id", "gemini-2.5-flash")
        system_instruction: str | None = payload.get("general_sheet_rule")
        row_data: List[Any] = payload.get("row_data", [])
        column_contexts: List[Any] = payload.get("column_contexts", [])
        # ENFORCE GROUNDED SEARCH: always enable grounding
        enable_grounding: bool = True
        # If you want to make this a toggle, set enable_grounding from a config or payload

        # Build Gemini API contents list (system prompt as 'model', user as 'user')
        contents = []
        if system_instruction:
            contents.append(genai.types.Content(
                role="model",
                parts=[genai.types.Part.from_text(text=system_instruction)]
            ))
        user_prompt = (
            f"Considering the following column‑specific contexts and row data:\n"
            f"Column Contexts: {json.dumps(column_contexts, ensure_ascii=False)}\n"
            f"Row Data: {json.dumps(row_data, ensure_ascii=False)}\n\n"
            "Task: Apply these rules and contexts to the Row Data. "
            "Return the *modified* row data as a JSON array of strings first, context can go only afterwards "
            "with each array element holding the string value for its column."
        )
        contents.append(genai.types.Content(
            role="user",
            parts=[genai.types.Part.from_text(text=user_prompt)]
        ))

        # -------- Generation Config ----------------------------------------------------
        # Only include supported fields for GenerateContentConfig
        cfg_kwargs: Dict[str, Any] = {}
        if payload.get("ai_temperature") is not None:
            cfg_kwargs["temperature"] = payload["ai_temperature"]
        if enable_grounding:
            cfg_kwargs["tools"] = [Tool(google_search=GoogleSearch())]

        gen_config = GenerateContentConfig(**cfg_kwargs)

        # -------- API Call -------------------------------------------------------------
        # The model name should be just the model id, e.g. "gemini-2.0-flash", not "models/..."
        response = client.models.generate_content(
            model=model_id,  # model_id should be like "gemini-2.0-flash"
            contents=contents,
            config=gen_config,
        )

        response_text: str = response.text.strip()

        # Strip common markdown fences that LLMs sometimes add.
        if response_text.startswith("```json"):
            response_text = response_text[7:].strip()
        if response_text.startswith("```"):
            response_text = response_text[3:].strip()
        if response_text.endswith("```"):
            response_text = response_text[:-3].strip()

        # Extract only the first JSON array/object
        response_text = extract_first_json(response_text)

        try:
            parsed_data = json.loads(response_text)
            result: Dict[str, Any] = {
                "success": True,
                "data": parsed_data,
                "raw_response": response.text,
            }
        except json.JSONDecodeError as err:
            result = {
                "success": False,
                "error": f"Failed to decode JSON from AI response: {err}",
                "raw_response": response.text,
            }

    except Exception as exc:  # broad catch is deliberate – we proxy to Rust
        result = {
            "success": False,
            "error": str(exc),
            "raw_response": str(exc),
        }

    # Always return *string* back to Rust (PyO3 expects a Python str)
    return json.dumps(result, ensure_ascii=False)

def extract_first_json(text):
    # Try to find the first JSON array or object in the text
    array_match = re.search(r'\[.*?\]', text, re.DOTALL)
    object_match = re.search(r'\{.*?\}', text, re.DOTALL)
    if array_match:
        return array_match.group(0)
    if object_match:
        return object_match.group(0)
    return text  # fallback: return original
