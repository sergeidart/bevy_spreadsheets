// Moved from editor/ai_control_panel.rs
use super::ai_context_utils::decorate_context_with_type;
use super::ai_control_left_panel::draw_left_panel;
use super::ai_group_panel::draw_group_panel;
use crate::sheets::definitions::{default_ai_model_id, ColumnDataType, SheetMetadata};
use crate::sheets::events::{
    AiBatchResultKind, AiBatchTaskResult, RequestCreateAiSchemaGroup, RequestDeleteAiSchemaGroup,
    RequestRenameAiSchemaGroup, RequestSelectAiSchemaGroup, RequestToggleAiRowGeneration,
    RequestUpdateAiSendSchema, RequestUpdateAiStructureSend,
};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState};
use crate::ui::systems::SendEvent;
use crate::SessionApiKey;
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

#[derive(Clone)]
struct NonStructureColumnInfo {
    index: usize,
    context: Option<String>,
    data_type: ColumnDataType,
    included: bool,
}

fn rebuild_included_vectors(
    columns: &[NonStructureColumnInfo],
    indices: &mut Vec<usize>,
    contexts: &mut Vec<Option<String>>,
    data_types: &mut Vec<ColumnDataType>,
) {
    indices.clear();
    contexts.clear();
    data_types.clear();
    for info in columns.iter() {
        if info.included {
            indices.push(info.index);
            contexts.push(decorate_context_with_type(
                info.context.as_ref(),
                info.data_type,
            ));
            data_types.push(info.data_type);
        }
    }
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
    _send_schema_writer: &mut EventWriter<RequestUpdateAiSendSchema>,
    _structure_send_writer: &mut EventWriter<RequestUpdateAiStructureSend>,
    create_group_writer: &mut EventWriter<RequestCreateAiSchemaGroup>,
    rename_group_writer: &mut EventWriter<RequestRenameAiSchemaGroup>,
    select_group_writer: &mut EventWriter<RequestSelectAiSchemaGroup>,
    delete_group_writer: &mut EventWriter<RequestDeleteAiSchemaGroup>,
) {
    let selection_allowed =
        state.ai_mode == AiModeState::Preparing || state.ai_mode == AiModeState::ResultsReady;
    let has_api = session_api_key.0.is_some();
    let _can_send = selection_allowed && has_api; // retained for potential future logic (left panel handles send availability)

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

    let mut non_structure_columns: Vec<NonStructureColumnInfo> = Vec::new();
    if let Some(meta) = metadata_opt_ref {
        for (c_idx, col_def) in meta.columns.iter().enumerate() {
            if matches!(
                col_def.validator,
                Some(crate::sheets::definitions::ColumnValidator::Structure)
            ) {
                continue;
            }
            let included = !matches!(col_def.ai_include_in_send, Some(false));
            non_structure_columns.push(NonStructureColumnInfo {
                index: c_idx,
                context: col_def.ai_context.clone(),
                data_type: col_def.data_type,
                included,
            });
        }
    }

    let mut included_actual_indices: Vec<usize> = Vec::new();
    let mut column_contexts: Vec<Option<String>> = Vec::new();
    let mut column_data_types: Vec<ColumnDataType> = Vec::new();
    rebuild_included_vectors(
        &non_structure_columns,
        &mut included_actual_indices,
        &mut column_contexts,
        &mut column_data_types,
    );

    let root_model_id = root_meta
        .as_ref()
        .map_or_else(default_ai_model_id, |m| m.ai_model_id.clone());
    let root_rule = root_meta.as_ref().and_then(|m| m.ai_general_rule.clone());
    let root_grounding = root_meta
        .as_ref()
        .and_then(|m| m.requested_grounding_with_google_search)
        .unwrap_or(false);

    ui.horizontal_wrapped(|ui| {
        if state.last_ai_button_min_x > 0.0 { let panel_left = ui.max_rect().min.x; let indent = (state.last_ai_button_min_x - panel_left).max(0.0); ui.add_space(indent); }
        // Left panel draw (send button + status + context)
        draw_left_panel(ui, state, registry, selected_category_clone, selected_sheet_name_clone, session_api_key);

    let toggle_label = "Add Rows";
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

    if structure_path.is_empty() { draw_group_panel(ui, state, &root_category, &root_sheet, root_meta.as_ref(), create_group_writer, rename_group_writer, select_group_writer, delete_group_writer); }

        if state.ai_mode == AiModeState::ResultsReady {
            let num_existing = state.ai_row_reviews.len();
            let num_new = state.ai_new_row_reviews.len();
            let total = num_existing + num_new;
            if ui.add_enabled(total > 0, egui::Button::new(format!("📋 Review Batch ({} rows)", total))).clicked() {
                state.ai_batch_review_active = true;
                state.ai_mode = AiModeState::Reviewing;
            }
        }
    // No far-right push; the controls remain inline to resemble the Toybox row

        if state.ai_mode == AiModeState::Submitting {
            ui.spinner();
        }
    });

    if state.ai_group_add_popup_open {
        let mut is_open = true;
        egui::Window::new("New Schema Group")
            .id(egui::Id::new("ai_group_add_popup_window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
            .open(&mut is_open)
            .show(ui.ctx(), |popup_ui| {
                popup_ui.label("Group name");
                let text_edit = popup_ui.add(
                    egui::TextEdit::singleline(&mut state.ai_group_add_name_input)
                        .hint_text("e.g. Draft")
                        .desired_width(220.0),
                );
                let trimmed = state.ai_group_add_name_input.trim();
                let create_enabled = !trimmed.is_empty() && !root_sheet.is_empty();
                let create_clicked = popup_ui
                    .add_enabled(create_enabled, egui::Button::new("Create"))
                    .clicked();
                let pressed_enter =
                    text_edit.lost_focus() && popup_ui.input(|i| i.key_pressed(egui::Key::Enter));
                let cancel_clicked = popup_ui.button("Cancel").clicked();

                if (create_clicked || (pressed_enter && create_enabled)) && create_enabled {
                    create_group_writer.write(RequestCreateAiSchemaGroup {
                        category: root_category.clone(),
                        sheet_name: root_sheet.clone(),
                        desired_name: Some(trimmed.to_string()),
                    });
                    state.mark_ai_included_columns_dirty();
                    state.ai_group_add_popup_open = false;
                    state.ai_group_add_name_input.clear();
                    popup_ui.ctx().request_repaint();
                }

                if cancel_clicked {
                    state.ai_group_add_popup_open = false;
                    state.ai_group_add_name_input.clear();
                }
            });
        if !is_open {
            state.ai_group_add_popup_open = false;
            state.ai_group_add_name_input.clear();
        }
    }

    if state.ai_group_rename_popup_open {
        if let Some(target_name) = state.ai_group_rename_target.clone() {
            let mut is_open = true;
            egui::Window::new("Rename Schema Group")
                .id(egui::Id::new("ai_group_rename_popup_window"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_TOP, [0.0, 120.0])
                .open(&mut is_open)
                .show(ui.ctx(), |popup_ui| {
                    popup_ui.label(format!("Rename '{}' to:", target_name));
                    let text_edit = popup_ui.add(
                        egui::TextEdit::singleline(&mut state.ai_group_rename_input)
                            .hint_text("Group name")
                            .desired_width(220.0),
                    );
                    let trimmed = state.ai_group_rename_input.trim();
                    let rename_enabled = !trimmed.is_empty() && !root_sheet.is_empty();
                    let rename_clicked = popup_ui
                        .add_enabled(rename_enabled, egui::Button::new("Rename"))
                        .clicked();
                    let pressed_enter = text_edit.lost_focus()
                        && popup_ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let cancel_clicked = popup_ui.button("Cancel").clicked();

                    if (rename_clicked || (pressed_enter && rename_enabled)) && rename_enabled {
                        rename_group_writer.write(RequestRenameAiSchemaGroup {
                            category: root_category.clone(),
                            sheet_name: root_sheet.clone(),
                            old_name: target_name.clone(),
                            new_name: trimmed.to_string(),
                        });
                        state.ai_group_rename_popup_open = false;
                        state.ai_group_rename_target = None;
                        state.ai_group_rename_input.clear();
                        popup_ui.ctx().request_repaint();
                    }

                    if cancel_clicked {
                        state.ai_group_rename_popup_open = false;
                        state.ai_group_rename_target = None;
                        state.ai_group_rename_input.clear();
                    }
                });
            if !is_open {
                state.ai_group_rename_popup_open = false;
                state.ai_group_rename_target = None;
                state.ai_group_rename_input.clear();
            }
        } else {
            state.ai_group_rename_popup_open = false;
            state.ai_group_rename_input.clear();
        }
    }

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
            rebuild_included_vectors(
                &non_structure_columns,
                &mut included_actual_indices,
                &mut column_contexts,
                &mut column_data_types,
            );
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
            let column_type_names: Vec<String> =
                column_data_types.iter().map(|dt| dt.to_string()).collect();
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
                column_data_types: column_type_names.clone(),
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
                    "AllowAddRows:{} ({})",
                    allow_additions_flag, allow_additions_source
                );
                let _ = writeln!(dbg, "Column Data Types: {:?}", column_type_names);
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
                                            kind: AiBatchResultKind::Root {
                                                structure_context: None,
                                            },
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
                                kind: AiBatchResultKind::Root {
                                    structure_context: None,
                                },
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

/// Parse structure rows from a serialized cell string produced previously by serialize_structure_rows_for_review.
/// Accepts either a JSON array of objects or legacy newline/pipe separated formats.
// parse_structure_rows_from_cell now provided by sheets::systems::ai::utils

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
