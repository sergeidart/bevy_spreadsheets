// Logic handlers extracted from the monolithic UI ai_control_panel implementation.
// This file should remain UI-framework agnostic (no egui usage) so it can be reused
// from multiple UI layouts.
use bevy::prelude::*;
use bevy_tokio_tasks::TokioTasksRuntime;
use pyo3::{exceptions::PyValueError, prelude::*, types::PyString};
use std::ffi::CString;

use crate::{
    sheets::{
        events::{AiBatchResultKind, AiBatchTaskResult},
    },
    ui::systems::SendEvent,
    SessionApiKey,
};

/// Rewrite the Python processor file to ensure it's up to date before execution
fn rewrite_python_processor() {
    const AI_PROCESSOR_PY: &str = include_str!("../../../../script/ai_processor.py");
    let script_path = std::path::Path::new("script/ai_processor.py");
    if let Some(parent) = script_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(script_path, AI_PROCESSOR_PY);
}

/// Parent key information for structured data (e.g., structure columns)
#[derive(Clone, serde::Serialize, Debug)]
pub struct ParentKeyInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    pub key: String,
}

/// A group of rows belonging to a single parent (for structure requests)
#[derive(Clone, serde::Serialize, Debug)]
pub struct ParentGroup {
    pub parent_key: ParentKeyInfo,
    pub rows: Vec<Vec<String>>,
}

/// Shared payload structure for all AI batch requests (regular and structure)
#[derive(Clone, serde::Serialize)]
pub struct BatchPayload {
    pub ai_model_id: String,
    pub general_sheet_rule: Option<String>,
    pub column_contexts: Vec<Option<String>>,
    /// For regular requests: flat array of rows
    /// For structure requests: this will be empty, use parent_groups instead
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rows_data: Vec<Vec<String>>,
    pub requested_grounding_with_google_search: bool,
    pub allow_row_additions: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_prefix_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_prefix_headers: Option<Vec<String>>,
    /// For structure requests: grouped parent-child data with clear boundaries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_groups: Option<Vec<ParentGroup>>,
    pub user_prompt: String,
}

/// Helper to parse Python response data - handles both flat rows and grouped parent_groups responses
fn parse_python_response_data(data: &serde_json::Value) -> Result<Vec<Vec<String>>, String> {
    if let serde_json::Value::Array(arr) = data {
        let mut out = Vec::new();
        // Check if this is grouped data (parent_groups response) - array of arrays of rows
        let is_grouped = !arr.is_empty()
            && arr[0].is_array()
            && arr[0]
                .as_array()
                .map_or(false, |inner| !inner.is_empty() && inner[0].is_array());

        if is_grouped {
            // Flatten grouped response: [[row, row], [row, row]] -> [row, row, row, row]
            for group_v in arr {
                if let serde_json::Value::Array(group_rows) = group_v {
                    for row_v in group_rows {
                        if let serde_json::Value::Array(cells) = row_v {
                            out.push(
                                cells
                                    .iter()
                                    .map(|v| match v {
                                        serde_json::Value::String(s) => s.clone(),
                                        other => other.to_string(),
                                    })
                                    .collect(),
                            );
                        } else {
                            return Err(format!("Row in group not an array: {}", row_v));
                        }
                    }
                } else {
                    return Err(format!("Group not an array: {}", group_v));
                }
            }
        } else {
            // Regular flat array of rows
            for row_v in arr {
                if let serde_json::Value::Array(cells) = row_v {
                    out.push(
                        cells
                            .iter()
                            .map(|v| match v {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            })
                            .collect(),
                    );
                } else {
                    return Err(format!("Row not an array: {}", row_v));
                }
            }
        }
        Ok(out)
    } else {
        Err("Expected array of rows or groups".to_string())
    }
}

/// Spawn a background python task for batch (selected rows) processing.
#[allow(clippy::too_many_arguments)]
pub fn spawn_batch_task(
    runtime: &TokioTasksRuntime,
    commands: &mut Commands,
    api_key: &SessionApiKey,
    payload_json: String,
    original_rows: Vec<usize>,
    included_cols: Vec<usize>,
    key_prefix_count: usize,
) {
    // Rewrite Python processor file to ensure it's up to date
    rewrite_python_processor();

    let api_key_for_task = api_key.0.clone();
    let included_cols_clone = included_cols.clone();
    let original_rows_clone = original_rows.clone();
    let key_prefix_count_clone = key_prefix_count;
    let commands_entity = commands.spawn_empty().id();
    runtime.spawn_background_task(move |mut ctx| async move {
        let api_key_value = match api_key_for_task {
            Some(k) if !k.is_empty() => k,
            _ => {
                let err_msg = "API Key not set".to_string();
                ctx.run_on_main_thread(move |world_ctx| {
                    world_ctx
                        .world
                        .commands()
                        .entity(commands_entity)
                        .insert(SendEvent::<AiBatchTaskResult> {
                            event: AiBatchTaskResult {
                                original_row_indices: original_rows_clone,
                                result: Err(err_msg),
                                raw_response: None,
                                included_non_structure_columns: Vec::new(),
                                key_prefix_count: key_prefix_count_clone,
                                kind: AiBatchResultKind::Root {
                                    structure_context: None,
                                },
                            },
                        });
                })
                .await;
                return;
            }
        };
        let (result, raw_response) = tokio::task::spawn_blocking(move || {
            Python::with_gil(
                |py| -> PyResult<(Result<Vec<Vec<String>>, String>, Option<String>)> {
                    let python_file_path = "script/ai_processor.py";
                    let processor_code_string = std::fs::read_to_string(python_file_path)?;
                    let code_c_str = CString::new(processor_code_string)
                        .map_err(|e| PyValueError::new_err(format!("CString error: {}", e)))?;
                    let file_name_c_str = CString::new(python_file_path).map_err(|e| {
                        PyValueError::new_err(format!("File name CString error: {}", e))
                    })?;
                    let module_name_c_str = CString::new("ai_processor").map_err(|e| {
                        PyValueError::new_err(format!("Module name CString error: {}", e))
                    })?;
                    let module = PyModule::from_code(
                        py,
                        code_c_str.as_c_str(),
                        file_name_c_str.as_c_str(),
                        module_name_c_str.as_c_str(),
                    )?;
                    let binding =
                        module.call_method1("execute_ai_query", (api_key_value, payload_json))?;
                    let result_json_str: &str = binding.downcast::<PyString>()?.to_str()?;
                    #[derive(serde::Deserialize)]
                    struct PyResp {
                        success: bool,
                        data: Option<serde_json::Value>,
                        error: Option<String>,
                        raw_response: Option<String>,
                    }
                    let resp: PyResp = serde_json::from_str(result_json_str)
                        .map_err(|e| PyValueError::new_err(format!("Parse JSON error: {}", e)))?;
                    if resp.success {
                        if let Some(data) = resp.data {
                            let parsed_result = parse_python_response_data(&data);
                            Ok((parsed_result, resp.raw_response))
                        } else {
                            Ok((Err("No data returned".to_string()), resp.raw_response))
                        }
                    } else {
                        Ok((
                            Err(resp
                                .error
                                .unwrap_or_else(|| "Unknown batch error".to_string())),
                            resp.raw_response,
                        ))
                    }
                },
            )
        })
        .await
        .unwrap_or_else(|e| Ok((Err(format!("Tokio panic: {}", e)), None)))
        .unwrap_or_else(|e| (Err(format!("PyO3 error: {}", e)), Some(e.to_string())));
        ctx.run_on_main_thread(move |world_ctx| {
            world_ctx
                .world
                .commands()
                .entity(commands_entity)
                .insert(SendEvent::<AiBatchTaskResult> {
                    event: AiBatchTaskResult {
                        original_row_indices: original_rows_clone,
                        result,
                        raw_response,
                        included_non_structure_columns: included_cols_clone,
                        key_prefix_count: key_prefix_count_clone,
                        kind: AiBatchResultKind::Root {
                            structure_context: None,
                        },
                    },
                });
        })
        .await;
    });
}

