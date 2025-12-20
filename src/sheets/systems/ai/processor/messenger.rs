// src/sheets/systems/ai/processor/messenger.rs
//! Messenger - AI Communication
//!
//! This module handles communication with the AI (Python/Gemini bridge).
//!
//! ## Responsibilities
//!
//! - Build AI request payload from prepared data
//! - Execute Python AI query (via pyo3)
//! - Handle timeouts and errors
//! - Return raw response for parsing
//!
//! ## Payload Format
//!
//! The `AiPayload` structure matches what the Python `ai_processor.py` script expects.
//! Field names must match exactly for compatibility.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;
use std::ffi::CString;

use super::genealogist::Ancestry;
use super::genealogist::Lineage;
use super::pre_processor::PreparedBatch;

// ============================================================================
// Payload Structures - Must match Python ai_processor.py expected format
// ============================================================================

/// Payload structure for AI batch requests.
/// 
/// Field names match what Python `ai_processor.py` expects:
/// - `ai_model_id` - The Gemini model to use
/// - `general_sheet_rule` - AI context/rule for the sheet
/// - `column_contexts` - Per-column AI context (decorated with type info)
/// - `rows_data` - Array of row arrays (flat format)
/// - `requested_grounding_with_google_search` - Enable Google Search grounding
/// - `allow_row_additions` - Whether AI can add new rows
/// - `user_prompt` - Optional user prompt
#[derive(Clone, serde::Serialize, Debug)]
pub struct AiPayload {
    pub ai_model_id: String,
    pub general_sheet_rule: Option<String>,
    pub column_contexts: Vec<Option<String>>,
    /// For regular requests: flat array of rows
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rows_data: Vec<Vec<String>>,
    pub requested_grounding_with_google_search: bool,
    pub allow_row_additions: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_prefix_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_prefix_headers: Option<Vec<String>>,
    pub user_prompt: String,
}

/// Result of an AI request
#[derive(Debug, Clone)]
pub struct MessengerResult {
    /// Whether the request succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Raw JSON response from AI
    pub raw_response: Option<String>,
}

impl MessengerResult {
    /// Create a success result
    pub fn success(raw_response: String) -> Self {
        Self {
            success: true,
            error: None,
            raw_response: Some(raw_response),
        }
    }

    /// Create an error result
    pub fn error(message: String, raw_response: Option<String>) -> Self {
        Self {
            success: false,
            error: Some(message),
            raw_response,
        }
    }
}

/// Configuration for an AI request - mirrors AiPayload fields
#[derive(Debug, Clone)]
pub struct RequestConfig {
    /// Grid column indices to include (from collect_ai_included_columns)
    pub included_indices: Vec<usize>,
    /// Column header names (for response parsing)
    pub column_names: Vec<String>,
    /// Column contexts (ai_context per column, decorated with type info)
    pub column_contexts: Vec<Option<String>>,
    /// AI general rule for the sheet
    pub ai_context: Option<String>,
    /// AI model ID
    pub model_id: String,
    /// Whether to allow row generation
    pub allow_row_generation: bool,
    /// Grounding with Google Search
    pub grounding_with_google_search: bool,
    /// Lineage prefix values (ancestor display values) - prepended to each row
    pub lineage_prefix_values: Vec<String>,
    /// Lineage prefix contexts (AI context per ancestor) - prepended to column_contexts
    pub lineage_prefix_contexts: Vec<Option<String>>,
    /// Prefix column names (ancestor table names) - for response parsing of object format
    pub prefix_column_names: Vec<String>,
    /// Whether this is a child table (has parent_key column)
    /// Used to determine the key column index for matching rows
    pub is_child_table: bool,
    /// Root parent table name - when first step is a child table (from navigation)
    pub root_parent_table_name: Option<String>,
    /// Root parent stable index - when first step is a child table (from navigation)
    pub root_parent_stable_index: Option<usize>,
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            included_indices: Vec::new(),
            column_names: Vec::new(),
            column_contexts: Vec::new(),
            ai_context: None,
            model_id: "gemini-2.5-flash-preview-05-20".to_string(),
            allow_row_generation: false,
            grounding_with_google_search: false,
            lineage_prefix_values: Vec::new(),
            lineage_prefix_contexts: Vec::new(),
            prefix_column_names: Vec::new(),
            is_child_table: false,
            root_parent_table_name: None,
            root_parent_stable_index: None,
        }
    }
}

/// AI Messenger - handles communication with the AI
#[derive(Debug, Default)]
pub struct Messenger;

impl Messenger {
    /// Create a new messenger
    pub fn new() -> Self {
        Self
    }

    /// Ensure the Python processor script exists
    pub fn ensure_python_script() {
        const AI_PROCESSOR_PY: &str = include_str!("../../../../../script/ai_processor.py");
        let script_path = std::path::Path::new("script/ai_processor.py");
        if let Some(parent) = script_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(script_path, AI_PROCESSOR_PY);
    }

    /// Build request payload JSON using BatchPayload format
    ///
    /// Uses the same payload structure as the existing single-step AI system
    /// to ensure compatibility with the Python `ai_processor.py` script.
    ///
    /// # Arguments
    /// * `config` - Request configuration
    /// * `batch` - Prepared batch of rows (for root table or single-parent legacy)
    /// * `lineage` - Parent lineage (for root table or single-parent legacy)
    /// * `parent_batches` - Optional list of (Lineage, PreparedBatch) for multi-parent structure requests
    ///
    /// # Returns
    /// JSON string for the AI request
    /// 
    /// # Note
    /// This method is used by the structure_processor and unit tests.
    /// The new processor uses `build_payload_with_ancestry` instead.
    #[allow(dead_code)]
    pub fn build_payload(
        &self,
        config: &RequestConfig,
        batch: &PreparedBatch,
        lineage: Option<&Lineage>,
        parent_batches: Option<&Vec<(Lineage, PreparedBatch)>>,
    ) -> Result<String, String> {
        // Build rows_data as Vec<Vec<String>> - the format Python expects
        let mut rows_data: Vec<Vec<String>> = Vec::new();
        
        // Build column_contexts, prepending lineage prefixes if present
        let mut column_contexts = config.column_contexts.clone();
        
        // Determine prefix values and contexts to use
        let (prefix_values, prefix_contexts): (Vec<String>, Vec<Option<String>>) = 
            if !config.lineage_prefix_values.is_empty() {
                // Case 1: Root table with structure navigation context
                (config.lineage_prefix_values.clone(), config.lineage_prefix_contexts.clone())
            } else if let Some(lin) = lineage {
                if !lin.is_empty() {
                    // Case 2: Child table with lineage from Director (single batch)
                    let values: Vec<String> = lin.ancestors.iter()
                        .map(|a| a.display_value.clone())
                        .collect();
                    let contexts: Vec<Option<String>> = lin.ancestors.iter()
                        .map(|a| Some(format!("Parent from {}", a.table_name)))
                        .collect();
                    (values, contexts)
                } else {
                    (Vec::new(), Vec::new())
                }
            } else {
                (Vec::new(), Vec::new())
            };

        if let Some(batches) = parent_batches {
            // Multi-parent batch mode
            // Flatten all batches into rows_data, prepending parent display value to each row
            
            // Add parent context column
            column_contexts.insert(0, Some("Parent Context".to_string()));
            
            for (lin, p_batch) in batches {
                // Get parent display value (last ancestor)
                let parent_val = lin.ancestors.last()
                    .map(|a| a.display_value.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                
                if p_batch.rows.is_empty() {
                    // If no rows for this parent, create a single placeholder row
                    // containing parent value + empty strings for child columns.
                    // This allows AI to generate children for this parent.
                    let mut new_row = Vec::with_capacity(config.included_indices.len() + 1);
                    new_row.push(parent_val.clone());
                    new_row.extend(std::iter::repeat(String::new()).take(config.included_indices.len()));
                    rows_data.push(new_row);
                } else {
                    for row in &p_batch.rows {
                        let mut new_row = Vec::with_capacity(row.column_values.len() + 1);
                        new_row.push(parent_val.clone());
                        new_row.extend(row.column_values.clone());
                        rows_data.push(new_row);
                    }
                }
            }
        } else {
            // Single batch mode
            rows_data = batch.rows.iter()
                .map(|row| row.column_values.clone())
                .collect();
            
            // Apply prefix values and contexts if we have any
            if !prefix_values.is_empty() {
                // Prepend prefix contexts to column_contexts
                let mut new_contexts = prefix_contexts.clone();
                new_contexts.extend(column_contexts.clone());
                column_contexts = new_contexts;
                
                if rows_data.is_empty() {
                    // If no rows but we have prefix values (parent context), create a single row
                    // containing prefix values + empty strings for child columns.
                    let mut new_row = prefix_values.clone();
                    new_row.extend(std::iter::repeat(String::new()).take(config.included_indices.len()));
                    rows_data.push(new_row);
                } else {
                    // Prepend prefix values to each row
                    for row in rows_data.iter_mut() {
                        let mut new_row = prefix_values.clone();
                        new_row.extend(row.drain(..));
                        *row = new_row;
                    }
                }
            }
        }
        
        // Build AiPayload with the correct field names for Python
        let payload = AiPayload {
            ai_model_id: config.model_id.clone(),
            general_sheet_rule: config.ai_context.clone(),
            column_contexts,
            rows_data,
            requested_grounding_with_google_search: config.grounding_with_google_search,
            allow_row_additions: config.allow_row_generation,
            key_prefix_count: None,  // Never sent to AI - used internally for response parsing
            key_prefix_headers: None, // Never sent to AI - used internally for response parsing
            user_prompt: String::new(),
        };

        serde_json::to_string(&payload)
            .map_err(|e| format!("Failed to serialize payload: {}", e))
    }

    /// Build request payload JSON with per-batch ancestry
    ///
    /// This is the new ancestry-aware payload builder. Each batch has its own
    /// ancestry chain, which is prepended to every row in that batch.
    ///
    /// # Arguments
    /// * `config` - Request configuration
    /// * `batches` - Prepared batches of rows
    /// * `batch_ancestries` - Ancestry for each batch (parallel to batches)
    /// * `ancestry_contexts` - AI contexts for ancestry columns (gathered once per table)
    ///
    /// # Returns
    /// JSON string for the AI request
    pub fn build_payload_with_ancestry(
        &self,
        config: &RequestConfig,
        batches: &[PreparedBatch],
        batch_ancestries: &[Ancestry],
        ancestry_contexts: &[Option<String>],
    ) -> Result<String, String> {
        // Build rows_data as Vec<Vec<String>> - the format Python expects
        let mut rows_data: Vec<Vec<String>> = Vec::new();
        
        // Determine ancestry depth from the first non-empty ancestry
        let ancestry_depth = batch_ancestries
            .iter()
            .map(|a| a.depth())
            .max()
            .unwrap_or(0);
        
        // Build column_contexts, prepending ancestry contexts then lineage_prefix_contexts
        let mut column_contexts = Vec::new();
        
        // First: ancestry contexts (from table chain, e.g., ["Aircraft context", "Aircraft_Pylons context"])
        if ancestry_depth > 0 && !ancestry_contexts.is_empty() {
            column_contexts.extend(ancestry_contexts.iter().cloned());
        }
        
        // Second: any lineage_prefix_contexts from navigation stack (for root table started from child)
        if !config.lineage_prefix_contexts.is_empty() {
            column_contexts.extend(config.lineage_prefix_contexts.clone());
        }
        
        // Third: the actual data column contexts
        column_contexts.extend(config.column_contexts.clone());
        
        // Calculate total prefix columns: ancestry + lineage_prefix
        let lineage_prefix_count = config.lineage_prefix_values.len();
        let total_prefix_count = ancestry_depth + lineage_prefix_count;
        
        // Process each batch with its ancestry
        for (batch_idx, batch) in batches.iter().enumerate() {
            let ancestry = batch_ancestries.get(batch_idx)
                .cloned()
                .unwrap_or_else(Ancestry::empty);
            
            // Get ancestry display values (padded to ancestry_depth if needed)
            let mut ancestry_values: Vec<String> = ancestry.display_values();
            while ancestry_values.len() < ancestry_depth {
                ancestry_values.push(String::new()); // Pad with empty strings if ancestry is shorter
            }
            
            // Combine ancestry + lineage_prefix for row prefix
            let mut row_prefix = ancestry_values;
            row_prefix.extend(config.lineage_prefix_values.clone());
            
            if batch.rows.is_empty() && total_prefix_count > 0 {
                // If no rows for this batch but we have ancestry/prefix, create a placeholder row
                // This allows AI to generate children for this parent
                let mut new_row = row_prefix;
                new_row.extend(std::iter::repeat(String::new()).take(config.included_indices.len()));
                rows_data.push(new_row);
            } else {
                // Normal case: prepend ancestry + prefix to each row
                for row in &batch.rows {
                    let mut new_row = row_prefix.clone();
                    new_row.extend(row.column_values.clone());
                    rows_data.push(new_row);
                }
            }
        }
        
        // Handle edge case: root table with no ancestry but has lineage_prefix
        if ancestry_depth == 0 && !config.lineage_prefix_values.is_empty() && batches.len() == 1 {
            // Root table started from structure navigation
            // lineage_prefix_contexts were already added, no ancestry to add
            // rows_data already has lineage_prefix prepended from the loop above
        }
        
        // Handle edge case: root table with no prefix at all
        if ancestry_depth == 0 && config.lineage_prefix_values.is_empty() && batches.len() == 1 {
            // Pure root table, no ancestry or navigation prefix
            // rows_data should just be the raw row values
            rows_data = batches[0].rows.iter()
                .map(|row| row.column_values.clone())
                .collect();
            // column_contexts should just be the data contexts
            column_contexts = config.column_contexts.clone();
        }
        
        // Build AiPayload with the correct field names for Python
        let payload = AiPayload {
            ai_model_id: config.model_id.clone(),
            general_sheet_rule: config.ai_context.clone(),
            column_contexts,
            rows_data,
            requested_grounding_with_google_search: config.grounding_with_google_search,
            allow_row_additions: config.allow_row_generation,
            key_prefix_count: None,  // Not sent to AI
            key_prefix_headers: None, // Not sent to AI
            user_prompt: String::new(),
        };

        serde_json::to_string(&payload)
            .map_err(|e| format!("Failed to serialize payload: {}", e))
    }

    /// Execute AI request
    ///
    /// # Arguments
    /// * `api_key` - API key for the AI service
    /// * `payload_json` - JSON payload string
    ///
    /// # Returns
    /// MessengerResult with response data or error
    pub async fn execute(
        &self,
        api_key: String,
        payload_json: String,
    ) -> MessengerResult {
        let result = tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| -> PyResult<MessengerResult> {
                let python_file_path = "script/ai_processor.py";
                let processor_code_string = std::fs::read_to_string(python_file_path)?;
                let code_c_str = CString::new(processor_code_string)
                    .map_err(|e| PyValueError::new_err(format!("CString error: {}", e)))?;
                let file_name_c_str = CString::new(python_file_path)
                    .map_err(|e| PyValueError::new_err(format!("File name CString error: {}", e)))?;
                let module_name_c_str = CString::new("ai_processor")
                    .map_err(|e| PyValueError::new_err(format!("Module name CString error: {}", e)))?;

                let module = PyModule::from_code(
                    py,
                    code_c_str.as_c_str(),
                    file_name_c_str.as_c_str(),
                    module_name_c_str.as_c_str(),
                )?;

                let binding = module.call_method1("execute_ai_query", (api_key, payload_json))?;
                let result_str: &str = binding.downcast::<PyString>()?.to_str()?;

                Self::parse_python_response(result_str)
            })
        })
        .await;

        match result {
            Ok(Ok(messenger_result)) => messenger_result,
            Ok(Err(e)) => MessengerResult::error(format!("PyO3 error: {}", e), None),
            Err(e) => MessengerResult::error(format!("Tokio panic: {}", e), None),
        }
    }

    /// Parse Python response JSON
    fn parse_python_response(response_text: &str) -> PyResult<MessengerResult> {
        let parsed: serde_json::Value = serde_json::from_str(response_text)
            .map_err(|e| PyValueError::new_err(format!("JSON parse error: {}", e)))?;

        let success = parsed.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        let raw_response = parsed
            .get("raw_response")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if !success {
            let error = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            return Ok(MessengerResult::error(error, raw_response));
        }

        // Just validate that data exists, actual parsing done by Director's ResponseParser
        if parsed.get("data").and_then(|v| v.as_array()).is_none() {
            return Ok(MessengerResult::error("Expected data array".to_string(), raw_response));
        }

        Ok(MessengerResult::success(raw_response.unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_payload() {
        use super::super::pre_processor::{PreparedRow, PreparedBatch};
        use super::super::navigator::{StableRowId, RowOrigin};
        use std::collections::HashSet;

        let messenger = Messenger::new();

        let config = RequestConfig {
            included_indices: vec![0, 1],
            column_names: vec!["Name".to_string(), "Speed".to_string()],
            column_contexts: vec![Some("Aircraft name".to_string()), Some("Speed in km/h".to_string())],
            ai_context: Some("Fill in aircraft data".to_string()),
            model_id: "gemini-2.5-flash-preview-05-20".to_string(),
            allow_row_generation: true,
            grounding_with_google_search: false,
            lineage_prefix_values: Vec::new(),
            lineage_prefix_contexts: Vec::new(),
            prefix_column_names: Vec::new(),
            is_child_table: false,
            root_parent_table_name: None,
            root_parent_stable_index: None,
        };

        let stable_id = StableRowId {
            table_name: "Aircraft".to_string(),
            category: None,
            stable_index: 0,
            display_value: "MiG-25PD".to_string(),
            origin: RowOrigin::Original,
            parent_stable_index: None,
            parent_table_name: None,
        };

        let mut sent_display_values = HashSet::new();
        sent_display_values.insert("MiG-25PD".to_string());

        let batch = PreparedBatch {
            rows: vec![PreparedRow {
                stable_id,
                column_values: vec!["MiG-25PD".to_string(), "3000".to_string()],
            }],
            sent_display_values,
        };

        let payload = messenger.build_payload(&config, &batch, None, None);
        assert!(payload.is_ok());

        let json: serde_json::Value = serde_json::from_str(&payload.unwrap()).unwrap();
        // Check BatchPayload field names
        assert_eq!(json["ai_model_id"], "gemini-2.5-flash-preview-05-20");
        assert_eq!(json["general_sheet_rule"], "Fill in aircraft data");
        assert!(json["rows_data"].as_array().unwrap().len() == 1);
        assert_eq!(json["rows_data"][0][0], "MiG-25PD");
        assert_eq!(json["rows_data"][0][1], "3000");
    }

    #[test]
    fn test_messenger_result() {
        let success = MessengerResult::success(r#"{"data": []}"#.to_string());
        assert!(success.success);
        assert!(success.raw_response.is_some());

        let error = MessengerResult::error("Test error".to_string(), None);
        assert!(!error.success);
        assert_eq!(error.error, Some("Test error".to_string()));
    }
}
