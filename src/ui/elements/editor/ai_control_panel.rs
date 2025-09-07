// src/ui/elements/editor/ai_control_panel.rs
use bevy::log::error;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;
use crate::sheets::definitions::default_ai_model_id;
use crate::sheets::events::AiBatchTaskResult;
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
pub(crate) fn show_ai_control_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    selected_sheet_name_clone: &Option<String>,
    runtime: &TokioTasksRuntime,
    registry: &SheetRegistry,
    commands: &mut Commands,
    session_api_key: &SessionApiKey,
) {
    ui.horizontal_wrapped(|ui| {
        // Indent this sub-row directly under the AI toggle button position
        if state.last_ai_button_min_x > 0.0 {
            let panel_left = ui.max_rect().min.x;
            let indent = (state.last_ai_button_min_x - panel_left).max(0.0);
            ui.add_space(indent);
        }

    // Send button first, then status label to its right

    // Removed inline 'AI Can Add Rows' toggle (managed in Settings popup only)

        let can_send = (state.ai_mode == AiModeState::Preparing || state.ai_mode == AiModeState::ResultsReady)
            && !state.ai_selected_rows.is_empty()
            && session_api_key.0.is_some();

        let mut hover_text_send = "Send selected row(s) for AI processing".to_string();
        if session_api_key.0.is_none() {
            hover_text_send = "API Key not set. Please set it in Settings.".to_string();
        } else if state.ai_selected_rows.is_empty() {
            hover_text_send = "Select at least one row first.".to_string();
        }

    let send_button_text = format!("🚀 Send to AI");

    if ui.add_enabled(can_send, egui::Button::new(send_button_text)).on_hover_text(hover_text_send).clicked() {
            // BATCH SEND IMPLEMENTATION
            state.ai_mode = AiModeState::Submitting;
            state.ai_raw_output_display.clear();
            state.ai_output_panel_visible = true; // show bottom panel at start
            // Clear snapshot review data (legacy maps removed)
            state.ai_row_reviews.clear();
            state.ai_new_row_reviews.clear();
            state.ai_context_prefix_by_row.clear();
            ui.ctx().request_repaint();

            let task_category = selected_category_clone.clone();
            let effective_sheet_name = if let Some(vctx) = state.virtual_structure_stack.last() { vctx.virtual_sheet_name.clone() } else { selected_sheet_name_clone.clone().unwrap_or_default() };
            let task_sheet_name = effective_sheet_name.clone();
            let api_key_for_task = session_api_key.0.clone();

            // Collect and sort selected row indices
            let mut original_rows: Vec<usize> = state.ai_selected_rows.iter().cloned().collect();
            original_rows.sort_unstable();

            let sheet_data_opt = registry.get_sheet(&task_category, &task_sheet_name);
            let metadata_opt_ref = sheet_data_opt.and_then(|d| d.metadata.as_ref());
            // Resolve root sheet metadata (for model & general rule & allow additions)
            let (root_category, root_sheet, root_meta) = {
                let mut root_category = selected_category_clone.clone();
                let mut root_sheet = task_sheet_name.clone();
                let mut safety = 0;
                loop {
                    safety += 1; if safety > 16 { break; }
                    let meta_opt = registry.get_sheet(&root_category, &root_sheet).and_then(|s| s.metadata.as_ref());
                    if let Some(m) = meta_opt {
                        if let Some(parent) = &m.structure_parent { root_category = parent.parent_category.clone(); root_sheet = parent.parent_sheet.clone(); continue; } else { break; }
                    } else { break; }
                }
                let root_meta = registry.get_sheet(&root_category, &root_sheet).and_then(|s| s.metadata.as_ref()).cloned();
                (root_category, root_sheet, root_meta)
            };
            let model_id = root_meta.as_ref().map_or_else(default_ai_model_id, |m| m.ai_model_id.clone());
            let rule = root_meta.as_ref().and_then(|m| m.ai_general_rule.clone());
            // Use per-sheet grounding flag (default false if missing)
            let grounding = root_meta.as_ref()
                .and_then(|m| m.requested_grounding_with_google_search)
                .unwrap_or(false);

            // Determine included columns (non-structure) & contexts
            let mut included_actual_indices: Vec<usize> = Vec::new();
            let mut column_contexts: Vec<Option<String>> = Vec::new();
            if let Some(meta) = metadata_opt_ref {
                for (c_idx, col_def) in meta.columns.iter().enumerate() {
                    if matches!(col_def.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { continue; }
                    included_actual_indices.push(c_idx);
                    column_contexts.push(col_def.ai_context.clone());
                }
            }

            // Build rows_data (only non-structure columns)
            let mut rows_data: Vec<Vec<String>> = Vec::new();
            if let Some(sheet_data) = sheet_data_opt {
                for &row_idx in &original_rows {
                    let full_row = sheet_data.grid.get(row_idx).cloned().unwrap_or_default();
                    let mut reduced: Vec<String> = Vec::with_capacity(included_actual_indices.len());
                    for &col_index in &included_actual_indices { reduced.push(full_row.get(col_index).cloned().unwrap_or_default()); }
                    rows_data.push(reduced);
                }
            }

            #[derive(serde::Serialize)]
            struct BatchPythonPayload {
                ai_model_id: String,
                general_sheet_rule: Option<String>,
                // Contexts for ALL columns in rows_data after prefixing keys (keys first, then original non-structure columns)
                column_contexts: Vec<Option<String>>,
                // Row data with key prefix columns included
                rows_data: Vec<Vec<String>>,
                // Number of leading columns that are key/context only (must be preserved untouched by AI)
                key_prefix_count: usize,
                requested_grounding_with_google_search: bool,
                allow_row_additions: bool,
            }

            // Resolve allow_row_additions flag with optimistic UI toggle support
            let mut allow_additions_flag = root_meta.as_ref().map(|m| m.ai_enable_row_generation).unwrap_or(false);
            // Prefer an in-flight pending toggle for this specific root sheet, if any
            if let Some((p_cat, p_sheet, p_val)) = &state.pending_ai_row_generation_toggle {
                if *p_cat == root_category && *p_sheet == root_sheet { allow_additions_flag = *p_val; }
            } else if let Some(eff) = state.effective_ai_can_add_rows {
                // Fallback to UI-cached effective flag (kept in sync in the panel)
                allow_additions_flag = eff;
            }
            // ---- Build recursive key chain (ancestor keys) ----
            let mut key_chain_headers: Vec<String> = Vec::new();
            let mut key_chain_contexts: Vec<Option<String>> = Vec::new();
            let mut key_chain_values_per_row: Vec<Vec<String>> = Vec::new();
            // Reconstruct ancestry by walking up virtual_structure_stack plus root metadata relationships.
            // The virtual_structure_stack keeps only path down to current sheet (each with parent context). We walk it to collect parent row indices.
            // For each ancestor level we read the parent sheet's key column value for that parent row.
            // Determine path contexts (exclude current leaf sheet; gather ancestors in order top->bottom)
            let mut ancestry: Vec<(Option<String>, String, usize)> = Vec::new();
            for vctx in &state.virtual_structure_stack {
                ancestry.push((vctx.parent.parent_category.clone(), vctx.parent.parent_sheet.clone(), vctx.parent.parent_row));
            }
            // Remove leaf if present (last vctx corresponds to current sheet view, but its parent row belongs to ancestor sheet which we need)
            // ancestry already only has parents; order is from first entered to last; that's already top->bottom.
            // Now gather key column header & context per ancestor sheet
            for (anc_cat, anc_sheet, _row_idx) in &ancestry {
                if let Some(sheet) = registry.get_sheet(anc_cat, anc_sheet) {
                    if let Some(meta) = &sheet.metadata {
                        // Prefer an explicitly selected key column from any structure child on this sheet; else fallback to first non-structure
                        let key_col_index = meta.columns.iter().enumerate().find_map(|(_idx, c)| {
                            // If any structure column on this sheet points to a parent key, that is the key for this level
                            if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) {
                                c.structure_key_parent_column_index
                            } else { None }
                        }).unwrap_or_else(|| meta.columns.iter().position(|c| !matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure))).unwrap_or(0));
                        if let Some(col_def) = meta.columns.get(key_col_index) {
                            key_chain_headers.push(col_def.header.clone());
                            key_chain_contexts.push(col_def.ai_context.clone());
                        }
                    }
                }
            }
            // For current sheet, also include its first non-structure column as local key if in a nested view
            if !state.virtual_structure_stack.is_empty() {
                if let Some(sheet) = sheet_data_opt { if let Some(meta) = &sheet.metadata { if let Some(idx) = meta.columns.iter().position(|c| !matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure))) { if let Some(col_def)= meta.columns.get(idx){ key_chain_headers.push(col_def.header.clone()); key_chain_contexts.push(col_def.ai_context.clone()); } } } }
            }
            // Build key_chain_rows by extracting values for each selected row from combined ancestry + current sheet
            for &row_idx in &original_rows {
                let mut row_vals: Vec<String> = Vec::new();
                // Ancestor values: use stored ancestor row indices to fetch value from each ancestor sheet's chosen key column
                for (anc_cat, anc_sheet, anc_row_idx) in &ancestry {
                    if let Some(sheet) = registry.get_sheet(anc_cat, anc_sheet) {
                        if let Some(meta) = &sheet.metadata {
                            let key_col_index = meta.columns.iter().enumerate().find_map(|(_idx, c)| {
                                if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) {
                                    c.structure_key_parent_column_index
                                } else { None }
                            }).unwrap_or_else(|| meta.columns.iter().position(|c| !matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure))).unwrap_or(0));
                            let val = sheet.grid.get(*anc_row_idx).and_then(|r| r.get(key_col_index)).cloned().unwrap_or_default();
                            row_vals.push(val);
                        }
                    }
                }
                // Current sheet key value for this row
                if let Some(sheet) = sheet_data_opt { if let Some(meta) = &sheet.metadata {
                    // Use local explicit key if any child structure selected it; else first non-structure
                    let idx = meta.columns.iter().enumerate().find_map(|(_i,c)| if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { c.structure_key_parent_column_index } else { None })
                        .or_else(|| meta.columns.iter().position(|c| !matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure))))
                        .unwrap_or(0);
                    let val = sheet.grid.get(row_idx).and_then(|r| r.get(idx)).cloned().unwrap_or_default(); row_vals.push(val);
                } }
                key_chain_values_per_row.push(row_vals);
            }

            let key_prefix_count = if key_chain_headers.is_empty() { 0 } else { key_chain_headers.len() };
            // Prepend key values to each existing row in rows_data and extend contexts accordingly
            if key_prefix_count > 0 {
                // Extend column contexts: keys first
                let mut new_contexts: Vec<Option<String>> = Vec::with_capacity(key_prefix_count + column_contexts.len());
                for ctx in &key_chain_contexts { new_contexts.push(ctx.clone()); }
                new_contexts.extend(column_contexts.into_iter());
                column_contexts = new_contexts;
                // Prepend per-row
                for (i, row) in rows_data.iter_mut().enumerate() {
                    if let Some(keys) = key_chain_values_per_row.get(i) {
                        let mut new_row = keys.clone();
                        new_row.extend(row.drain(..));
                        *row = new_row;
                    }
                }
            }

            let payload = BatchPythonPayload {
                ai_model_id: model_id,
                general_sheet_rule: rule,
                column_contexts: column_contexts.clone(),
                rows_data: rows_data.clone(),
                key_prefix_count,
                requested_grounding_with_google_search: grounding,
                allow_row_additions: allow_additions_flag,
            };
            let payload_json = match serde_json::to_string(&payload) { Ok(j) => j, Err(e) => { error!("Failed to serialize batch payload: {}", e); return; } };
            // Show what is being sent in the bottom AI output panel for debugging
            if let Ok(pretty) = serde_json::to_string_pretty(&payload) {
                let mut dbg = String::new();
                use std::fmt::Write as _;
                let _ = writeln!(dbg, "--- AI Request Payload (Debug) ---");
                let _ = writeln!(dbg, "{}", pretty);
                let _ = writeln!(dbg, "--- Selected Original Row Indices (sheet) ---");
                let _ = writeln!(dbg, "{:?}", original_rows);
                let _ = writeln!(dbg, "--- Included Non-Structure Columns (payload order -> sheet col index) ---");
                let _ = writeln!(dbg, "{:?}", included_actual_indices);
                // Attempt to list column names for clarity
                if let Some(sheet_meta) = metadata_opt_ref {
                    let mut names: Vec<(usize, String)> = Vec::new();
                    for (payload_pos, actual_idx) in included_actual_indices.iter().enumerate() {
                        if let Some(col) = sheet_meta.columns.get(*actual_idx) { names.push((payload_pos, col.header.clone())); }
                    }
                    let _ = writeln!(dbg, "--- Included Column Names (payload position, name) ---");
                    let _ = writeln!(dbg, "{:?}", names);
                }
                // Show allow additions flag + model id
                let _ = writeln!(dbg, "Model: {}  AllowRowAdditions:{}  Grounding:{}", payload.ai_model_id, payload.allow_row_additions, grounding);
                if key_prefix_count > 0 {
                    let _ = writeln!(dbg, "--- Key Prefix Count --- {}", key_prefix_count);
                    for (i, keys) in key_chain_values_per_row.iter().enumerate() { let _ = writeln!(dbg, "Row {} Keys: {:?}", original_rows[i], keys); }
                }
                if payload.allow_row_additions {
                    let _ = writeln!(dbg, "Row Additions Enabled: The model may add new rows AFTER the first {} original rows to provide similar item if any applicable here. Each new row must match column count.", original_rows.len());
                }
                state.ai_raw_output_display = dbg;
                state.ai_output_panel_visible = true;
            }

            let included_cols_clone = included_actual_indices.clone();
            let original_rows_clone = original_rows.clone();
            let commands_entity = commands.spawn_empty().id();

            runtime.spawn_background_task(move |mut ctx| async move {
                let api_key_value = match api_key_for_task { Some(k) if !k.is_empty() => k, _ => {
                    let err_msg = "API Key not set".to_string();
                    ctx.run_on_main_thread(move |world_ctx| { world_ctx.world.commands().entity(commands_entity).insert(SendEvent::<AiBatchTaskResult>{ event: AiBatchTaskResult { original_row_indices: original_rows_clone, result: Err(err_msg), raw_response: None, included_non_structure_columns: Vec::new(), key_prefix_count: 0 } }); }).await; return; } };

                let (result, raw_response) = tokio::task::spawn_blocking(move || {
                    Python::with_gil(|py| -> PyResult<(Result<Vec<Vec<String>>, String>, Option<String>)> {
                        let python_file_path = "script/ai_processor.py";
                        let processor_code_string = std::fs::read_to_string(python_file_path)?;
                        let code_c_str = CString::new(processor_code_string).map_err(|e| PyValueError::new_err(format!("CString error: {}", e)))?;
                        let file_name_c_str = CString::new(python_file_path).map_err(|e| PyValueError::new_err(format!("File name CString error: {}", e)))?;
                        let module_name_c_str = CString::new("ai_processor").map_err(|e| PyValueError::new_err(format!("Module name CString error: {}", e)))?;
                        let module = PyModule::from_code(py, code_c_str.as_c_str(), file_name_c_str.as_c_str(), module_name_c_str.as_c_str())?;
                        let binding = module.call_method1("execute_ai_query", (api_key_value, payload_json))?;
                        let result_json_str: &str = binding.downcast::<PyString>()?.to_str()?;
                        #[derive(serde::Deserialize)] struct PyResp { success: bool, data: Option<serde_json::Value>, error: Option<String>, raw_response: Option<String> }
                        let resp: PyResp = serde_json::from_str(result_json_str).map_err(|e| PyValueError::new_err(format!("Parse JSON error: {}", e)))?;
                        if resp.success {
                            if let Some(serde_json::Value::Array(arr)) = resp.data {
                                let mut out: Vec<Vec<String>> = Vec::new();
                                for row_v in arr {
                                    match row_v {
                                        serde_json::Value::Array(cells) => {
                                            out.push(cells.into_iter().map(|v| match v { serde_json::Value::String(s)=>s, other=>other.to_string() }).collect());
                                        }
                                        other => { return Ok((Err(format!("Row not an array: {}", other)), resp.raw_response)); }
                                    }
                                }
                                Ok((Ok(out), resp.raw_response))
                            } else {
                                Ok((Err("Expected array of rows".to_string()), resp.raw_response))
                            }
                        } else {
                            Ok((Err(resp.error.unwrap_or_else(|| "Unknown batch error".to_string())), resp.raw_response))
                        }
                    })
                }).await.unwrap_or_else(|e| Ok((Err(format!("Tokio panic: {}", e)), None)))
                .unwrap_or_else(|e| (Err(format!("PyO3 error: {}", e)), Some(e.to_string())));

                ctx.run_on_main_thread(move |world_ctx| { world_ctx.world.commands().entity(commands_entity).insert(SendEvent::<AiBatchTaskResult>{ event: AiBatchTaskResult { original_row_indices: original_rows_clone, result, raw_response, included_non_structure_columns: included_cols_clone, key_prefix_count } }); }).await;
            });
        }

        // Status label right of the send button. Include row count and remove the words "AI Mode" from the text.
        let status_text = match state.ai_mode {
            AiModeState::Preparing => format!("Preparing ({} Rows)", state.ai_selected_rows.len()),
            AiModeState::Submitting => "Submitting".to_string(),
            AiModeState::ResultsReady => "Results Ready".to_string(),
            AiModeState::Reviewing => "Reviewing".to_string(),
            AiModeState::Idle => "".to_string(),
        };
        if !status_text.is_empty() { ui.label(status_text); }
        // Place AI Context just to the right of the status instead of far right
        if ui.add_enabled(selected_sheet_name_clone.is_some(), egui::Button::new("⚙ AI Context")).on_hover_text("Edit per-sheet AI model and context").clicked() {
            // Reset tracking so popup initializes for the currently selected sheet, regardless of prior context
            state.ai_rule_popup_last_category = None;
            state.ai_rule_popup_last_sheet = None;
            state.ai_rule_popup_needs_init = true;
            state.show_ai_rule_popup = true;
        }

        if state.ai_mode == AiModeState::ResultsReady {
            let num_existing = state.ai_row_reviews.len();
            let num_new = state.ai_new_row_reviews.len();
            let total = num_existing + num_new;
            if ui.add_enabled(total > 0, egui::Button::new(format!("🧐 Review Batch ({} rows)", total))).clicked() {
                state.ai_batch_review_active = true;
                state.ai_mode = AiModeState::Reviewing;
            }
        }
    // No far-right push; the controls remain inline to resemble the Toybox row

        if state.ai_mode == AiModeState::Submitting {
            ui.spinner();
        }
    });
}