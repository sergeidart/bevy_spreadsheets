// src/ui/elements/editor/ai_control_panel.rs
use super::state::{AiModeState, EditorWindowState};
use crate::sheets::definitions::{default_ai_model_id, SheetMetadata};
use crate::sheets::events::{AiBatchTaskResult, RequestToggleAiRowGeneration};
use crate::sheets::resources::SheetRegistry;
use crate::ui::systems::SendEvent;
use crate::SessionApiKey;
use bevy::log::error;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;

use std::ffi::CString;
// --- PyO3 Imports ---
use pyo3::prelude::*;
use pyo3::types::PyString;
// Import the exception type we will create for JSON errors
use pyo3::exceptions::PyValueError;

#[derive(Clone, serde::Serialize)]
struct KeyPayload {
    #[serde(rename = "Context")]
    context: String,
    #[serde(rename = "Key")]
    key: String,
}

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
    toggle_writer: &mut EventWriter<RequestToggleAiRowGeneration>,
) {
    let selection_allowed =
        state.ai_mode == AiModeState::Preparing || state.ai_mode == AiModeState::ResultsReady;
    let has_api = session_api_key.0.is_some();
    let can_send = selection_allowed && has_api;

    let task_category = selected_category_clone.clone();
    let effective_sheet_name = if let Some(vctx) = state.virtual_structure_stack.last() {
        vctx.virtual_sheet_name.clone()
    } else {
        selected_sheet_name_clone.clone().unwrap_or_default()
    };
    let task_sheet_name = effective_sheet_name.clone();
    let sheet_data_opt = registry.get_sheet(&task_category, &task_sheet_name);
    let metadata_opt_ref = sheet_data_opt.and_then(|d| d.metadata.as_ref());

    let (root_category, root_sheet, root_meta, structure_path_vec) = {
        let mut root_category = selected_category_clone.clone();
        let mut root_sheet = task_sheet_name.clone();
        let mut path_rev: Vec<usize> = Vec::new();
        let mut safety = 0;
        loop {
            safety += 1;
            if safety > 16 {
                break;
            }
            let meta_opt = registry
                .get_sheet(&root_category, &root_sheet)
                .and_then(|s| s.metadata.as_ref());
            if let Some(m) = meta_opt {
                if let Some(parent) = &m.structure_parent {
                    path_rev.push(parent.parent_column_index);
                    root_category = parent.parent_category.clone();
                    root_sheet = parent.parent_sheet.clone();
                    continue;
                }
            }
            break;
        }
        path_rev.reverse();
        let root_meta = registry
            .get_sheet(&root_category, &root_sheet)
            .and_then(|s| s.metadata.as_ref())
            .cloned();
        (root_category, root_sheet, root_meta, path_rev)
    };
    let structure_path = structure_path_vec;

    let mut general_allow = root_meta
        .as_ref()
        .map_or(false, |m| m.ai_enable_row_generation);
    let mut allow_additions_flag = general_allow;
    let mut allow_additions_source = if allow_additions_flag {
        "metadata:true".to_string()
    } else {
        "metadata:false".to_string()
    };

    let structure_override_initial = if !structure_path.is_empty() {
        root_meta
            .as_ref()
            .and_then(|meta| resolve_structure_override(meta, &structure_path))
    } else {
        None
    };
    if let Some(override_val) = structure_override_initial {
        allow_additions_flag = override_val;
        allow_additions_source = "metadata:structure_override".to_string();
    }
    let structure_label = if !structure_path.is_empty() {
        root_meta
            .as_ref()
            .and_then(|meta| describe_structure_path(meta, &structure_path))
    } else {
        None
    };

    let mut key_chain_headers: Vec<String> = Vec::new();
    let mut key_chain_contexts: Vec<Option<String>> = Vec::new();
    let mut ancestors_with_keys: Vec<(Option<String>, String, usize, usize)> = Vec::new();
    for vctx in &state.virtual_structure_stack {
        let anc_cat = vctx.parent.parent_category.clone();
        let anc_sheet = vctx.parent.parent_sheet.clone();
        let anc_row_idx = vctx.parent.parent_row;
        if let Some(sheet) = registry.get_sheet(&anc_cat, &anc_sheet) {
            if let Some(meta) = &sheet.metadata {
                if let Some(key_col_index) =
                    meta.columns.iter().enumerate().find_map(|(_idx, c)| {
                        if matches!(
                            c.validator,
                            Some(crate::sheets::definitions::ColumnValidator::Structure)
                        ) {
                            c.structure_key_parent_column_index
                        } else {
                            None
                        }
                    })
                {
                    if let Some(col_def) = meta.columns.get(key_col_index) {
                        key_chain_headers.push(col_def.header.clone());
                        key_chain_contexts.push(col_def.ai_context.clone());
                    }
                    ancestors_with_keys.push((anc_cat, anc_sheet, anc_row_idx, key_col_index));
                }
            }
        }
    }

    let mut included_actual_indices: Vec<usize> = Vec::new();
    let mut column_contexts: Vec<Option<String>> = Vec::new();
    if let Some(meta) = metadata_opt_ref {
        for (c_idx, col_def) in meta.columns.iter().enumerate() {
            if matches!(
                col_def.validator,
                Some(crate::sheets::definitions::ColumnValidator::Structure)
            ) {
                continue;
            }
            included_actual_indices.push(c_idx);
            column_contexts.push(col_def.ai_context.clone());
        }
    }

    let root_model_id = root_meta
        .as_ref()
        .map_or_else(default_ai_model_id, |m| m.ai_model_id.clone());
    let root_rule = root_meta.as_ref().and_then(|m| m.ai_general_rule.clone());
    let root_grounding = root_meta
        .as_ref()
        .and_then(|m| m.requested_grounding_with_google_search)
        .unwrap_or(false);

    ui.horizontal_wrapped(|ui| {
        if state.last_ai_button_min_x > 0.0 {
            let panel_left = ui.max_rect().min.x;
            let indent = (state.last_ai_button_min_x - panel_left).max(0.0);
            ui.add_space(indent);
        }

        let mut hover_text_send = "Send selected row(s) for AI processing".to_string();
        if session_api_key.0.is_none() {
            hover_text_send = "API Key not set. Please set it in Settings.".to_string();
        } else if state.ai_selected_rows.is_empty() {
            hover_text_send =
                "No rows selected: click to send a Prompt-only AI request (will create rows)".to_string();
        }

        let send_button_text = format!("🚀 Send to AI");

        if ui
            .add_enabled(can_send, egui::Button::new(send_button_text))
            .on_hover_text(hover_text_send)
            .clicked()
        {
            // If no rows selected, open the prompt popup instead of immediate send
            if state.ai_selected_rows.is_empty() {
                state.show_ai_prompt_popup = true;
                // Prefill prompt box with last input retained
                ui.ctx().request_repaint();
                return;
            }
            // BATCH SEND IMPLEMENTATION
            state.ai_mode = AiModeState::Submitting;
            state.ai_raw_output_display.clear();
            state.ai_output_panel_visible = true; // show bottom panel at start
            // Clear snapshot review data (legacy maps removed)
            state.ai_row_reviews.clear();
            state.ai_new_row_reviews.clear();
            state.ai_context_prefix_by_row.clear();
            ui.ctx().request_repaint();

            let api_key_for_task = session_api_key.0.clone();

            // Collect and sort selected row indices
            let mut original_rows: Vec<usize> = state.ai_selected_rows.iter().cloned().collect();
            original_rows.sort_unstable();
            let model_id = root_model_id.clone();
            let rule = root_rule.clone();
            let grounding = root_grounding;

            let included_actual_indices = included_actual_indices.clone();
            let column_contexts = column_contexts.clone();

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
                // Contexts for ONLY non-structure columns; order matches rows_data columns
                column_contexts: Vec<Option<String>>,
                // Row data for ONLY non-structure columns
                rows_data: Vec<Vec<String>>,
                // Normalized single key object (Context + Key). We prefer a single
                // key instead of the legacy headers/rows block which confused the AI.
                key: Option<KeyPayload>,
                requested_grounding_with_google_search: bool,
                allow_row_additions: bool,
                // For visibility in debug payload JSON (model also receives the hint via prompt)
                row_additions_hint: Option<String>,
            }

            // ---- Build keys context (ancestor keys only; separate from rows_data) ----
            let mut key_chain_values_per_row: Vec<Vec<String>> = Vec::new();
            // Build key values aligned with headers (only for ancestors that have explicit keys)
            for &_row_idx in &original_rows {
                let mut row_vals: Vec<String> = Vec::new();
                for (anc_cat, anc_sheet, anc_row_idx, key_col_index) in &ancestors_with_keys {
                    if let Some(sheet) = registry.get_sheet(anc_cat, anc_sheet) {
                        let val = sheet
                            .grid
                            .get(*anc_row_idx)
                            .and_then(|r| r.get(*key_col_index))
                            .cloned()
                            .unwrap_or_default();
                        row_vals.push(val);
                    }
                }
                key_chain_values_per_row.push(row_vals);
            }
            // Normalize keys to a single object: prefer first context and first value.
            let key_payload_opt = if key_chain_headers.is_empty() || key_chain_contexts.is_empty() {
                None
            } else if !key_chain_values_per_row.is_empty() {
                let ctx = key_chain_contexts.get(0).and_then(|c| c.clone()).unwrap_or_default();
                let key_val = key_chain_values_per_row
                    .get(0)
                    .and_then(|r| r.get(0).cloned())
                    .unwrap_or_else(|| key_chain_headers.get(0).cloned().unwrap_or_default());
                Some(KeyPayload { context: ctx, key: key_val })
            } else {
                resolve_prompt_only_key(&key_chain_contexts, &key_chain_headers, &ancestors_with_keys, registry)
                    .map(|(ctx, key)| KeyPayload { context: ctx, key })
            };

            let payload = BatchPythonPayload {
                ai_model_id: model_id,
                general_sheet_rule: rule,
                column_contexts: column_contexts.clone(),
                rows_data: rows_data.clone(),
                key: key_payload_opt,
                requested_grounding_with_google_search: grounding,
                allow_row_additions: allow_additions_flag,
                row_additions_hint: if allow_additions_flag { Some(format!(
                    "Row Additions Enabled: The model may add new rows AFTER the first {} original rows to provide similar item if any applicable here. Each new row must match column count.",
                    original_rows.len()
                )) } else { None },
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
                // Show allow additions flag + its source + model id
                let _ = writeln!(dbg, "Model: {}  AllowRowAdditions:{} ({})  Grounding:{}", payload.ai_model_id, payload.allow_row_additions, allow_additions_source, grounding);
                if !key_chain_headers.is_empty() {
                    let _ = writeln!(dbg, "--- Keys (separate context) ---");
                    let _ = writeln!(dbg, "Headers: {:?}", key_chain_headers);
                    let _ = writeln!(dbg, "Contexts: {:?}", key_chain_contexts);
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
            ctx.run_on_main_thread(move |world_ctx| { world_ctx.world.commands().entity(commands_entity).insert(SendEvent::<AiBatchTaskResult>{ event: AiBatchTaskResult { original_row_indices: original_rows_clone, result: Err(err_msg), raw_response: None, included_non_structure_columns: Vec::new(), key_prefix_count: 0, prompt_only: false } }); }).await; return; } };

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

                // No key prefixes sent to AI; keep key_prefix_count=0 for downstream consumers
                ctx.run_on_main_thread(move |world_ctx| { world_ctx.world.commands().entity(commands_entity).insert(SendEvent::<AiBatchTaskResult>{ event: AiBatchTaskResult { original_row_indices: original_rows_clone, result, raw_response, included_non_structure_columns: included_cols_clone, key_prefix_count: 0, prompt_only: false } }); }).await;
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

        let toggle_label = if structure_path.is_empty() { "Row Additions" } else { "Structure Row Additions" };
        let toggle_tooltip = if structure_path.is_empty() {
            "Allow the AI to append new rows when generating results for this sheet.".to_string()
        } else if let Some(label) = structure_label.as_deref() {
            format!("Override row addition behavior for structure '{}'. Defaults to the general sheet setting until overridden.", label)
        } else {
            "Override row addition behavior for this structure. Defaults to the general sheet setting until overridden.".to_string()
        };
        let mut toggle_value = allow_additions_flag;
        let toggle_response = ui
            .add_enabled(root_meta.is_some(), egui::Checkbox::new(&mut toggle_value, toggle_label))
            .on_hover_text(toggle_tooltip);
        if toggle_response.changed() {
            allow_additions_flag = toggle_value;
            if structure_path.is_empty() {
                general_allow = toggle_value;
                allow_additions_source = "ui:general".to_string();
                toggle_writer.write(RequestToggleAiRowGeneration {
                    category: root_category.clone(),
                    sheet_name: root_sheet.clone(),
                    enabled: toggle_value,
                    structure_path: None,
                    structure_override: None,
                });
            } else {
                let override_opt = if toggle_value == general_allow { None } else { Some(toggle_value) };
                allow_additions_source = if override_opt.is_some() {
                    "ui:structure_override".to_string()
                } else {
                    "ui:structure_general".to_string()
                };
                toggle_writer.write(RequestToggleAiRowGeneration {
                    category: root_category.clone(),
                    sheet_name: root_sheet.clone(),
                    enabled: toggle_value,
                    structure_path: Some(structure_path.clone()),
                    structure_override: override_opt,
                });
            }
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

    // Inline Prompt Popup (simple implementation)
    if state.show_ai_prompt_popup {
        let mut is_open = true;
        let mut do_send = false;
        egui::Window::new("AI Prompt")
            .id(egui::Id::new("ai_prompt_inline_popup"))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut is_open)
            .show(ui.ctx(), |ui| {
                ui.label("Enter a prompt to generate rows (no rows selected).");
                ui.add_sized(
                    [ui.available_width(), 120.0],
                    egui::TextEdit::multiline(&mut state.ai_prompt_input)
                        .hint_text("Give me list of games released this month"),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !state.ai_prompt_input.trim().is_empty() && session_api_key.0.is_some(),
                            egui::Button::new("Send"),
                        )
                        .clicked()
                    {
                        do_send = true;
                    }
                    if ui.button("Cancel").clicked() {
                        state.show_ai_prompt_popup = false;
                    }
                });
            });
        if !is_open {
            state.show_ai_prompt_popup = false;
        }
        if do_send {
            // Build and send payload similar to batch but with zero originals + prompt
            state.show_ai_prompt_popup = false;
            state.last_ai_prompt_only = true;
            state.ai_mode = AiModeState::Submitting;
            state.ai_raw_output_display.clear();
            state.ai_output_panel_visible = true;
            state.ai_row_reviews.clear();
            state.ai_new_row_reviews.clear();
            state.ai_context_prefix_by_row.clear();
            let model_id = root_model_id.clone();
            let rule = root_rule.clone();
            let grounding = root_grounding;
            let column_contexts = column_contexts.clone();
            #[derive(serde::Serialize)]
            struct PromptPayload {
                ai_model_id: String,
                general_sheet_rule: Option<String>,
                column_contexts: Vec<Option<String>>,
                rows_data: Vec<Vec<String>>,
                key: Option<KeyPayload>,
                user_prompt: String,
                requested_grounding_with_google_search: bool,
                allow_row_additions: bool,
            }
            let prompt_key_payload = resolve_prompt_only_key(
                &key_chain_contexts,
                &key_chain_headers,
                &ancestors_with_keys,
                registry,
            )
            .map(|(ctx, key)| KeyPayload { context: ctx, key });
            let payload = PromptPayload {
                ai_model_id: model_id,
                general_sheet_rule: rule,
                column_contexts: column_contexts.clone(),
                rows_data: Vec::new(),
                key: prompt_key_payload.clone(),
                user_prompt: state.ai_prompt_input.clone(),
                requested_grounding_with_google_search: grounding,
                allow_row_additions: allow_additions_flag,
            };
            let payload_json = match serde_json::to_string(&payload) {
                Ok(j) => j,
                Err(e) => {
                    state.ai_raw_output_display = format!("Serialize error: {}", e);
                    state.ai_mode = AiModeState::Preparing;
                    return;
                }
            };
            if let Ok(pretty) = serde_json::to_string_pretty(&payload) {
                let mut dbg = format!("--- AI Prompt Payload ---\n{}", pretty);
                use std::fmt::Write as _;
                let _ = writeln!(
                    dbg,
                    "AllowRowAdditions:{} ({})",
                    allow_additions_flag, allow_additions_source
                );
                if let Some(ref key_payload) = prompt_key_payload {
                    let _ = writeln!(dbg, "Key Context:{}", key_payload.context);
                    let _ = writeln!(dbg, "Key Value:{}", key_payload.key);
                }
                state.ai_raw_output_display = dbg;
                state.ai_output_panel_visible = true;
            }
            let api_key_for_task = session_api_key.0.clone();
            let included_cols_clone = included_actual_indices.clone();
            let commands_entity = commands.spawn_empty().id();
            runtime.spawn_background_task(move |mut ctx| async move {
                let api_key_value =
                    match api_key_for_task {
                        Some(k) if !k.is_empty() => k,
                        _ => {
                            let err_msg = "API Key not set".to_string();
                            ctx.run_on_main_thread(move |world_ctx| {
                                world_ctx.world.commands().entity(commands_entity).insert(
                                    SendEvent::<AiBatchTaskResult> {
                                        event: AiBatchTaskResult {
                                            original_row_indices: Vec::new(),
                                            result: Err(err_msg),
                                            raw_response: None,
                                            included_non_structure_columns: Vec::new(),
                                            key_prefix_count: 0,
                                            prompt_only: true,
                                        },
                                    },
                                );
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
                            let code_c_str = CString::new(processor_code_string).map_err(|e| {
                                PyValueError::new_err(format!("CString error: {}", e))
                            })?;
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
                            let binding = module
                                .call_method1("execute_ai_query", (api_key_value, payload_json))?;
                            let result_json_str: &str = binding.downcast::<PyString>()?.to_str()?;
                            #[derive(serde::Deserialize)]
                            struct PyResp {
                                success: bool,
                                data: Option<serde_json::Value>,
                                error: Option<String>,
                                raw_response: Option<String>,
                            }
                            let resp: PyResp =
                                serde_json::from_str(result_json_str).map_err(|e| {
                                    PyValueError::new_err(format!("Parse JSON error: {}", e))
                                })?;
                            if resp.success {
                                if let Some(serde_json::Value::Array(arr)) = resp.data {
                                    let mut out: Vec<Vec<String>> = Vec::new();
                                    for row_v in arr {
                                        match row_v {
                                            serde_json::Value::Array(cells) => {
                                                out.push(
                                                    cells
                                                        .into_iter()
                                                        .map(|v| match v {
                                                            serde_json::Value::String(s) => s,
                                                            other => other.to_string(),
                                                        })
                                                        .collect(),
                                                );
                                            }
                                            other => {
                                                return Ok((
                                                    Err(format!("Row not an array: {}", other)),
                                                    resp.raw_response,
                                                ));
                                            }
                                        }
                                    }
                                    Ok((Ok(out), resp.raw_response))
                                } else {
                                    Ok((
                                        Err("Expected array of rows".to_string()),
                                        resp.raw_response,
                                    ))
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
                                original_row_indices: Vec::new(),
                                result,
                                raw_response,
                                included_non_structure_columns: included_cols_clone,
                                key_prefix_count: 0,
                                prompt_only: true,
                            },
                        });
                })
                .await;
            });
        }
    }
}

fn resolve_prompt_only_key(
    key_chain_contexts: &[Option<String>],
    key_chain_headers: &[String],
    ancestors_with_keys: &[(Option<String>, String, usize, usize)],
    registry: &SheetRegistry,
) -> Option<(String, String)> {
    for (idx, (anc_cat, anc_sheet, anc_row_idx, key_col_index)) in
        ancestors_with_keys.iter().enumerate()
    {
        let context = key_chain_contexts
            .get(idx)
            .and_then(|c| c.clone())
            .or_else(|| key_chain_headers.get(idx).cloned());
        let sheet = registry.get_sheet(anc_cat, anc_sheet)?;
        let row = sheet.grid.get(*anc_row_idx)?;
        let mut value = row.get(*key_col_index).cloned().unwrap_or_default();
        if value.trim().is_empty() {
            value = key_chain_headers.get(idx).cloned().unwrap_or_default();
        }
        if let Some(ctx) = context {
            return Some((ctx, value));
        }
    }
    None
}

fn resolve_structure_override(meta: &SheetMetadata, path: &[usize]) -> Option<bool> {
    if path.is_empty() {
        return None;
    }
    let column = meta.columns.get(path[0])?;
    if path.len() == 1 {
        return column.ai_enable_row_generation;
    }
    let mut field = column.structure_schema.as_ref()?.get(path[1])?;
    if path.len() == 2 {
        return field.ai_enable_row_generation;
    }
    for idx in path.iter().skip(2) {
        field = field.structure_schema.as_ref()?.get(*idx)?;
    }
    field.ai_enable_row_generation
}

fn describe_structure_path(meta: &SheetMetadata, path: &[usize]) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let mut names: Vec<String> = Vec::new();
    let column = meta.columns.get(path[0])?;
    names.push(column.header.clone());
    if path.len() == 1 {
        return Some(names.join(" -> "));
    }
    let mut field = column.structure_schema.as_ref()?.get(path[1])?;
    names.push(field.header.clone());
    for idx in path.iter().skip(2) {
        field = field.structure_schema.as_ref()?.get(*idx)?;
        names.push(field.header.clone());
    }
    Some(names.join(" -> "))
}
