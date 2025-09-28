// src/ui/elements/popups/ai_prompt_popup.rs
use crate::sheets::definitions::{default_ai_model_id, ColumnDataType, SheetMetadata};
use crate::sheets::events::AiBatchTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::ai_context_utils::decorate_context_with_type;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState};
use crate::ui::systems::SendEvent;
use crate::SessionApiKey;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;
use std::ffi::CString;

#[allow(dead_code)]
pub fn show_ai_prompt_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    runtime: &TokioTasksRuntime,
    commands: &mut Commands,
    session_api_key: &SessionApiKey,
) {
    if !state.show_ai_prompt_popup {
        return;
    }

    let mut is_open = state.show_ai_prompt_popup;
    let mut do_send = false;
    egui::Window::new("AI Prompt")
        .id(egui::Id::new("ai_prompt_popup_window"))
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut is_open)
        .show(ctx, |ui| {
            ui.label("Enter a prompt. Result rows will be treated as new AI rows.");
            ui.add_sized(
                [ui.available_width(), 120.0],
                egui::TextEdit::multiline(&mut state.ai_prompt_input)
                    .hint_text("e.g. Give me list of games released this month"),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.ai_prompt_input.trim().is_empty() && session_api_key.0.is_some(),
                        egui::Button::new("Send"),
                    )
                    .on_hover_text(if session_api_key.0.is_none() {
                        "API key missing"
                    } else {
                        "Send prompt to AI"
                    })
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
        state.show_ai_prompt_popup = false;
        state.last_ai_prompt_only = true;
        state.ai_mode = AiModeState::Submitting;
        state.ai_raw_output_display.clear();
        state.ai_output_panel_visible = true;
        state.ai_row_reviews.clear();
        state.ai_new_row_reviews.clear();
        state.ai_context_prefix_by_row.clear();
        let task_category = state.selected_category.clone();
        let sheet_name = if let Some(vctx) = state.virtual_structure_stack.last() {
            vctx.virtual_sheet_name.clone()
        } else {
            state.selected_sheet_name.clone().unwrap_or_default()
        };
        // Resolve root meta for model & rule
        let (_root_category, _root_sheet, root_meta, structure_path) = {
            let mut root_category = state.selected_category.clone();
            let mut root_sheet = sheet_name.clone();
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
        let model_id = root_meta
            .as_ref()
            .map_or_else(default_ai_model_id, |m| m.ai_model_id.clone());
        let rule = root_meta.as_ref().and_then(|m| m.ai_general_rule.clone());
        let grounding = root_meta
            .as_ref()
            .and_then(|m| m.requested_grounding_with_google_search)
            .unwrap_or(false);
        let mut allow_additions_flag = root_meta
            .as_ref()
            .map_or(false, |m| m.ai_enable_row_generation);
        if !structure_path.is_empty() {
            if let Some(override_val) = root_meta
                .as_ref()
                .and_then(|meta| resolve_structure_override(meta, &structure_path))
            {
                allow_additions_flag = override_val;
            }
        }
        // Non-structure columns
        let mut included_actual_indices: Vec<usize> = Vec::new();
        let mut column_contexts: Vec<Option<String>> = Vec::new();
        let mut column_data_types: Vec<ColumnDataType> = Vec::new();
        if let Some(sheet_meta) = registry
            .get_sheet(&task_category, &sheet_name)
            .and_then(|d| d.metadata.as_ref())
        {
            for (c_idx, col_def) in sheet_meta.columns.iter().enumerate() {
                if matches!(
                    col_def.validator,
                    Some(crate::sheets::definitions::ColumnValidator::Structure)
                ) {
                    continue;
                }
                if matches!(col_def.ai_include_in_send, Some(false)) {
                    continue;
                }
                included_actual_indices.push(c_idx);
                column_contexts.push(decorate_context_with_type(
                    col_def.ai_context.as_ref(),
                    col_def.data_type,
                ));
                column_data_types.push(col_def.data_type);
            }
        }
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
                            key_chain_contexts.push(decorate_context_with_type(
                                col_def.ai_context.as_ref(),
                                col_def.data_type,
                            ));
                        }
                        ancestors_with_keys.push((anc_cat, anc_sheet, anc_row_idx, key_col_index));
                    }
                }
            }
        }
        #[derive(Clone, serde::Serialize)]
        struct KeyPayload {
            #[serde(rename = "Context")]
            context: String,
            #[serde(rename = "Key")]
            key: String,
        }
        #[derive(serde::Serialize)]
        struct PromptPayload {
            ai_model_id: String,
            general_sheet_rule: Option<String>,
            column_contexts: Vec<Option<String>>,
            column_data_types: Vec<String>,
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
            column_data_types: column_data_types.iter().map(|dt| dt.to_string()).collect(),
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
        // Debug display
        if let Ok(pretty) = serde_json::to_string_pretty(&payload) {
            let mut dbg = format!("--- AI Prompt Payload ---\n{}", pretty);
            use std::fmt::Write as _;
            let _ = writeln!(dbg, "AllowRowAdditions:{}", allow_additions_flag);
            let _ = writeln!(
                dbg,
                "Column Data Types: {:?}",
                column_data_types
                    .iter()
                    .map(|dt| dt.to_string())
                    .collect::<Vec<_>>()
            );
            if let Some(ref key_payload) = prompt_key_payload {
                let _ = writeln!(dbg, "Key Context:{}", key_payload.context);
                let _ = writeln!(dbg, "Key Value:{}", key_payload.key);
            }
            state.ai_raw_output_display = dbg;
        }
        let api_key_for_task = session_api_key.0.clone();
        let included_cols_clone = included_actual_indices.clone();
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
                                    original_row_indices: Vec::new(),
                                    result: Err(err_msg),
                                    raw_response: None,
                                    included_non_structure_columns: Vec::new(),
                                    key_prefix_count: 0,
                                    prompt_only: true,
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
                        let binding = module
                            .call_method1("execute_ai_query", (api_key_value, payload_json))?;
                        let result_json_str: &str = binding.downcast::<PyString>()?.to_str()?;
                        #[derive(serde::Deserialize)]
                        #[allow(dead_code)]
                        struct PyResp {
                            success: bool,
                            data: Option<serde_json::Value>,
                            error: Option<String>,
                            raw_response: Option<String>,
                        }
                        let resp: PyResp = serde_json::from_str(result_json_str).map_err(|e| {
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
                                Ok((Err("Expected array of rows".to_string()), resp.raw_response))
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
