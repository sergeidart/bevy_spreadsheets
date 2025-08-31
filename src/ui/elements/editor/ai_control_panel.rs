// src/ui/elements/editor/ai_control_panel.rs
use bevy::log::{error, info, warn};
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;
use crate::sheets::definitions::{default_ai_model_id, default_grounding_with_google_search};
use crate::sheets::events::AiTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::ui::systems::SendEvent;
use crate::SessionApiKey;
use super::state::{AiModeState, EditorWindowState};

use std::ffi::CString;
// --- PyO3 Imports ---
use pyo3::prelude::*;
use pyo3::types::PyString;
// Import the exception type we will create for JSON errors
use pyo3::exceptions::PyValueError;


#[allow(clippy::too_many_arguments)]
pub(super) fn show_ai_control_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    selected_sheet_name_clone: &Option<String>,
    runtime: &TokioTasksRuntime,
    registry: &SheetRegistry,
    commands: &mut Commands,
    session_api_key: &SessionApiKey,
) {
    ui.horizontal(|ui| {
        if ui.button("⚙ Settings").on_hover_text("Configure API Key (Session)").clicked() {
            state.show_settings_popup = true;
        }

        if ui.button("Edit AI Config").on_hover_text("Edit sheet-specific AI model, rules, and parameters").clicked() {
            state.show_ai_rule_popup = true;
            state.ai_rule_popup_needs_init = true;
        }

        ui.separator();
        ui.label(format!("✨ AI Mode: {:?}", state.ai_mode));
        ui.separator();

        let can_send = (state.ai_mode == AiModeState::Preparing || state.ai_mode == AiModeState::ResultsReady)
            && !state.ai_selected_rows.is_empty()
            && session_api_key.0.is_some();

        let mut hover_text_send = "Send selected row(s) for AI processing".to_string();
        if session_api_key.0.is_none() {
            hover_text_send = "API Key not set. Please set it in Settings.".to_string();
        } else if state.ai_selected_rows.is_empty() {
            hover_text_send = "Select at least one row first.".to_string();
        }

        let send_button_text = format!("🚀 Send to AI ({} Rows)", state.ai_selected_rows.len());

        if ui.add_enabled(can_send, egui::Button::new(send_button_text)).on_hover_text(hover_text_send).clicked() {
            state.ai_mode = AiModeState::Submitting;
            state.ai_suggestions.clear();
            state.ai_review_queue.clear();
            state.current_ai_suggestion_edit_buffer = None;
            ui.ctx().request_repaint();

            for row_index in state.ai_selected_rows.iter().cloned() {
                let task_category = selected_category_clone.clone();
                // If inside a virtual structure view, use that virtual sheet for row data & metadata
                let effective_sheet_name = if let Some(vctx) = state.virtual_structure_stack.last() { vctx.virtual_sheet_name.clone() } else { selected_sheet_name_clone.clone().unwrap_or_default() };
                let task_sheet_name = effective_sheet_name.clone();
                let api_key_for_task = session_api_key.0.clone();

                let (
                    active_model_id,
                    general_rule_opt,
                    column_contexts,
                    row_data,
                    temperature,
                    top_k,
                    top_p,
                    enable_grounding,
                ) = {
                    let sheet_data_opt = registry.get_sheet(&task_category, &task_sheet_name);
                    let metadata_opt_ref = sheet_data_opt.and_then(|d| d.metadata.as_ref());
                    let model_id = metadata_opt_ref.map_or_else(default_ai_model_id, |m| m.ai_model_id.clone());
                    let rule = metadata_opt_ref.and_then(|m| m.ai_general_rule.clone());
                    // Row index refers to the active (possibly virtual) sheet
                    let full_row = sheet_data_opt.and_then(|d| d.grid.get(row_index)).cloned().unwrap_or_default();
                    let (temp, k, p) = metadata_opt_ref.map_or((None, None, None), |m| (m.ai_temperature, m.ai_top_k, m.ai_top_p));
                    let grounding = metadata_opt_ref.and_then(|m| m.requested_grounding_with_google_search).unwrap_or_else(|| default_grounding_with_google_search().unwrap_or(false));

                    // Determine which columns to include:
                    // Rule: Always exclude Structure validator columns from AI payload (both top-level and inside virtual views)
                    let mut included_contexts: Vec<Option<String>> = Vec::new();
                    let mut included_row: Vec<String> = Vec::new();
                    let mut included_actual_indices: Vec<usize> = Vec::new();
            if let Some(meta) = metadata_opt_ref {
                        if !full_row.is_empty() {
                            // Prepend ancestor + self key columns (context only, not part of mapping for merge)
                            // To obtain ancestor key indices, we'll need to know which structure column we are inside (if virtual). Use last virtual context if any.
                            let mut context_prefix_count = 0usize;
                let mut context_prefix_display: Vec<(String, String)> = Vec::new();
                            if let Some(vctx) = state.virtual_structure_stack.last() {
                                // Determine parent sheet metadata and structure column index from parent context
                                let parent_cat = vctx.parent.parent_category.clone();
                                let parent_sheet_name = vctx.parent.parent_sheet.clone();
                                let parent_col_index = vctx.parent.parent_col; // index inside parent sheet
                                let parent_row_index = vctx.parent.parent_row; // row in parent sheet
                                if let Some(parent_sheet) = registry.get_sheet(&parent_cat, &parent_sheet_name) {
                                    if let Some(parent_meta) = &parent_sheet.metadata {
                                        if let Some(parent_col) = parent_meta.columns.get(parent_col_index) {
                                            if let Some(ancestors) = parent_col.structure_ancestor_key_parent_column_indices.as_ref() {
                                                for anc_idx in ancestors.iter() {
                                                    if let Some(val) = parent_sheet.grid.get(parent_row_index).and_then(|r| r.get(*anc_idx)) {
                                                        let header = parent_meta.columns.get(*anc_idx).map(|c| c.header.clone()).unwrap_or_else(|| format!("AncestorKey{}", anc_idx));
                                                        included_contexts.push(parent_meta.columns.get(*anc_idx).and_then(|c| c.ai_context.clone()));
                                                        included_row.push(val.clone());
                                                        context_prefix_display.push((header, val.clone()));
                                                        context_prefix_count += 1;
                                                    }
                                                }
                                            }
                                            if let Some(kidx) = parent_col.structure_key_parent_column_index {
                                                if let Some(val) = parent_sheet.grid.get(parent_row_index).and_then(|r| r.get(kidx)) {
                                                    let header = parent_meta.columns.get(kidx).map(|c| c.header.clone()).unwrap_or_else(|| format!("Key{}", kidx));
                                                    included_contexts.push(parent_meta.columns.get(kidx).and_then(|c| c.ai_context.clone()));
                                                    included_row.push(val.clone());
                                                    context_prefix_display.push((header, val.clone()));
                                                    context_prefix_count += 1;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Now add this sheet's non-structure columns (updatable part)
                            for (c_idx, col_def) in meta.columns.iter().enumerate() {
                                let is_structure_col = matches!(col_def.validator, Some(crate::sheets::definitions::ColumnValidator::Structure));
                                if is_structure_col { continue; }
                                included_contexts.push(col_def.ai_context.clone());
                                let val = full_row.get(c_idx).cloned().unwrap_or_default();
                                included_row.push(val);
                                included_actual_indices.push(c_idx);
                            }
                            state.ai_context_only_prefix_count = context_prefix_count;
                            state.ai_context_prefix_by_row.insert(row_index, context_prefix_display);
                        }
                    }
                    // Store mapping (overwrite on each send)
                    state.ai_included_non_structure_columns = included_actual_indices;
                    (model_id, rule, included_contexts, included_row, temp, k, p, grounding)
                };

                #[derive(serde::Serialize)]
                struct PythonPayload {
                    ai_model_id: String,
                    general_sheet_rule: Option<String>,
                    column_contexts: Vec<Option<String>>,
                    row_data: Vec<String>,
                    ai_temperature: Option<f32>,
                    ai_top_k: Option<i32>,
                    ai_top_p: Option<f32>,
                    requested_grounding_with_google_search: bool,
                    context_only_prefix_count: usize,
                }

                let payload = PythonPayload {
                    ai_model_id: active_model_id,
                    general_sheet_rule: general_rule_opt,
                    column_contexts,
                    row_data,
                    ai_temperature: temperature,
                    ai_top_k: top_k,
                    ai_top_p: top_p,
                    requested_grounding_with_google_search: enable_grounding,
                    context_only_prefix_count: state.ai_context_only_prefix_count,
                };
                
                let payload_json = match serde_json::to_string(&payload) {
                    Ok(json) => json,
                    Err(e) => {
                        error!("Failed to serialize payload for Python script: {}", e);
                        continue;
                    }
                };
                
                let commands_entity = commands.spawn_empty().id();

                runtime.spawn_background_task(move |mut ctx| async move {
                    let api_key_value = match api_key_for_task {
                        Some(key) if !key.is_empty() => key,
                        _ => {
                            let err_msg = "API Key not found or empty in session.".to_string();
                            ctx.run_on_main_thread(move |world_ctx| {
                                world_ctx.world.commands().entity(commands_entity).insert(
                                    SendEvent::<AiTaskResult> { event: AiTaskResult { original_row_index: row_index, result: Err(err_msg), raw_response: None } }
                                );
                            }).await;
                            return;
                        }
                    };

                    let (result, raw_response) = tokio::task::spawn_blocking(move || {
                        // Log the Current Working Directory
                        match std::env::current_dir() {
                            Ok(cwd) => info!("Current Working Directory: {}", cwd.display()),
                            Err(e) => warn!("Failed to get CWD: {}", e),
                        }

                        Python::with_gil(|py| -> PyResult<(Result<Vec<String>, String>, Option<String>)> {
                            // This `?` is okay because `io::Error` can be converted to `PyErr` automatically.
                            let python_file_path = "script/ai_processor.py"; // Corrected path
                            info!("Attempting to load Python script from: {}", python_file_path);
                            let processor_code_string = std::fs::read_to_string(python_file_path)?;

                            let code_c_str = CString::new(processor_code_string)
                                .map_err(|e| PyValueError::new_err(format!("Failed to create CString for Python code: {}", e)))?;
                            // Use the same path for file_name_c_str for consistency
                            let file_name_c_str = CString::new(python_file_path) // Corrected path
                                .map_err(|e| PyValueError::new_err(format!("Failed to create CString for file_name ({}): {}", python_file_path, e)))?;
                            let module_name_c_str = CString::new("ai_processor")
                                .map_err(|e| PyValueError::new_err(format!("Failed to create CString for module_name: {}", e)))?;

                            let processor_module = PyModule::from_code(
                                py,
                                code_c_str.as_c_str(),
                                file_name_c_str.as_c_str(),
                                module_name_c_str.as_c_str()
                            )?;
                            let binding = processor_module
                                .call_method1("execute_ai_query", (api_key_value, payload_json))?;
                            let result_json_str: &str = binding
                                .downcast::<PyString>()?
                                .to_str()?;
                            
                            #[derive(serde::Deserialize)]
                            struct PythonResponse {
                                success: bool,
                                data: Option<Vec<String>>,
                                error: Option<String>,
                                raw_response: Option<String>,
                            }
                            
                            // FIX #2: Manually handle the `serde_json::Error` instead of using `?`.
                            let py_res: PythonResponse = match serde_json::from_str(result_json_str) {
                                Ok(res) => res,
                                Err(e) => {
                                    // Create a Python `ValueError` and return it as a `PyErr`.
                                    return Err(PyValueError::new_err(format!("Failed to parse JSON from Python: {}", e)));
                                }
                            };
                            
                            let final_result = if py_res.success {
                                Ok(py_res.data.unwrap_or_default())
                            } else {
                                Err(py_res.error.unwrap_or_else(|| "Unknown error from Python".into()))
                            };

                            Ok((final_result, py_res.raw_response.or_else(|| Some(result_json_str.to_string()))))
                        })
                    })
                    .await
                    .unwrap_or_else(|e| Ok((Err(format!("Tokio task panicked: {}", e)), None))) // Handle task panic
                    .unwrap_or_else(|e| (Err(format!("PyO3 Error: {}", e)), Some(e.to_string()))); // Handle PyO3 error

                    ctx.run_on_main_thread(move |world_ctx| {
                        world_ctx.world.commands().entity(commands_entity).insert(
                            SendEvent::<AiTaskResult> { event: AiTaskResult { original_row_index: row_index, result, raw_response } }
                        );
                    }).await;
                });
            }
        }

        if state.ai_mode == AiModeState::ResultsReady {
            let num_results = state.ai_suggestions.len();
            if ui.add_enabled(num_results > 0, egui::Button::new(format!("🧐 Review Suggestions ({})", num_results))).clicked() {
                state.ai_mode = AiModeState::Reviewing;
                state.ai_review_queue = state.ai_suggestions.keys().cloned().collect();
                state.ai_review_queue.sort_unstable();
                if !state.ai_review_queue.is_empty() {
                    super::ai_helpers::setup_review_for_index(state, 0);
                } else {
                    super::ai_helpers::exit_review_mode(state);
                }
            }
        }
        
        if state.ai_mode == AiModeState::Submitting {
            ui.spinner();
        }
    });
}