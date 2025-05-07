// src/ui/elements/editor/ai_panel_structs.rs
use serde::{Deserialize, Serialize};

// ++ Make structs pub(crate) or pub if not already ++
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct AiColumnContext { // Changed to pub(crate)
    pub header: String,
    pub original_value: String,
    pub data_type: String,
    pub validator: Option<String>,
    pub ai_column_context: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct AiPromptPayload { // Changed to pub(crate)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub general_sheet_rule: Option<String>,
    pub columns_to_process: Vec<AiColumnContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_max_output_tokens: Option<i32>,
    // --- Field name corrected ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_grounding_with_google_search: Option<bool>,
    // --- End Field name corrected ---
    pub output_format_instruction: String,
}