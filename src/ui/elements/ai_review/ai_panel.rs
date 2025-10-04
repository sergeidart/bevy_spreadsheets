// New lightweight AI panel orchestrator replacing deprecated ai_control_panel.rs
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;

use crate::{
    sheets::{
        definitions::default_ai_model_id,
        events::{RequestToggleAiRowGeneration, RequestCreateAiSchemaGroup, RequestRenameAiSchemaGroup, RequestSelectAiSchemaGroup, RequestDeleteAiSchemaGroup},
        resources::SheetRegistry,
    },
    ui::elements::editor::state::{AiModeState, EditorWindowState},
    SessionApiKey,
};

use super::{ai_control_left_panel::draw_left_panel_impl, ai_group_panel::draw_group_panel, ai_context_utils::decorate_context_with_type};
use crate::sheets::systems::ai::control_handler::{spawn_batch_task, resolve_structure_override, BatchPayload};
use crate::ui::elements::popups::ai_prompt_popup::show_ai_prompt_popup;

/// Build batch payload (selected rows) and spawn task.
#[allow(clippy::too_many_arguments)]
pub fn send_selected_rows(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    runtime: &TokioTasksRuntime,
    commands: &mut Commands,
    session_api_key: &SessionApiKey,
    user_prompt: Option<String>,
) {
    let selection: Vec<usize> = state.ai_selected_rows.iter().copied().collect();
    // Allow empty selection if user_prompt is provided
    if selection.is_empty() && user_prompt.is_none() { return; }
    let category = state.selected_category.clone();
    let sheet_name = if let Some(vctx) = state.virtual_structure_stack.last() { vctx.virtual_sheet_name.clone() } else { state.selected_sheet_name.clone().unwrap_or_default() };
    let sheet_opt = registry.get_sheet(&category, &sheet_name);
    let meta_opt = sheet_opt.and_then(|s| s.metadata.as_ref());
    if meta_opt.is_none() { return; }
    let meta = meta_opt.unwrap();

    // Determine root sheet (for structure planning) before we potentially navigate virtual view
    let (root_category, root_sheet_opt) = state.resolve_root_sheet(registry);

    // Collect non-structure columns included (already influenced by active schema group via metadata flags)
    let mut included_indices: Vec<usize> = Vec::new();
    let mut column_contexts: Vec<Option<String>> = Vec::new();
    for (idx, col) in meta.columns.iter().enumerate() {
        if matches!(col.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { continue; }
        if matches!(col.ai_include_in_send, Some(false)) { continue; }
        included_indices.push(idx);
        column_contexts.push(decorate_context_with_type(col.ai_context.as_ref(), col.data_type));
    }
    // Gather row data (only included non-structure columns)
    let mut rows_data: Vec<Vec<String>> = Vec::new();
    if let Some(sheet) = sheet_opt { for row_index in selection.iter().copied() { if let Some(row) = sheet.grid.get(row_index) { let mut out_row = Vec::new(); for &ci in &included_indices { out_row.push(row.get(ci).cloned().unwrap_or_default()); } rows_data.push(out_row); } } }

    let model_id = if meta.ai_model_id.is_empty() { default_ai_model_id() } else { meta.ai_model_id.clone() };
    
    // Use root sheet's general rule when inside a virtual structure
    let rule = if !state.virtual_structure_stack.is_empty() {
        if let Some(root_sheet) = root_sheet_opt.as_ref() {
            if let Some(root_meta) = registry.get_sheet(&root_category, root_sheet).and_then(|s| s.metadata.as_ref()) {
                root_meta.ai_general_rule.clone()
            } else {
                meta.ai_general_rule.clone()
            }
        } else {
            meta.ai_general_rule.clone()
        }
    } else {
        meta.ai_general_rule.clone()
    };
    
    // Grounding flag: use root sheet metadata if in structure, otherwise current metadata
    let grounding = if !state.virtual_structure_stack.is_empty() {
        // Inside structure - get grounding from root sheet
        if let Some(root_sheet) = root_sheet_opt.as_ref() {
            if let Some(root_meta) = registry.get_sheet(&root_category, root_sheet).and_then(|s| s.metadata.as_ref()) {
                root_meta.requested_grounding_with_google_search.unwrap_or(false)
            } else {
                meta.requested_grounding_with_google_search.unwrap_or(false)
            }
        } else {
            meta.requested_grounding_with_google_search.unwrap_or(false)
        }
    } else {
        meta.requested_grounding_with_google_search.unwrap_or(false)
    };

    // Row additions flag (structure aware). If inside structure stack, resolve root & overrides similar to old panel.
    let mut allow_additions_flag = meta.ai_enable_row_generation;
    if !state.virtual_structure_stack.is_empty() {
        // Find root meta & structure path like legacy logic.
        if let Some(root_sheet) = root_sheet_opt.as_ref() { 
            if let Some(root_meta) = registry.get_sheet(&root_category, root_sheet).and_then(|s| s.metadata.as_ref()) {
                // Start with root sheet's default
                let sheet_default = root_meta.ai_enable_row_generation;
                // Then try to get structure-specific override
                if let Some(override_val) = resolve_structure_override(root_meta, &state.ai_cached_included_columns_path) { 
                    allow_additions_flag = override_val; 
                } else {
                    // No explicit override, use root sheet's default
                    allow_additions_flag = sheet_default;
                }
            } 
        }
    }

    // Note: Key prefix logic is no longer used here. When inside a structure, we send
    // data as parent_groups with ParentKeyInfo. When not in structure, rows_data is sent as-is.
    let key_prefix_count = 0usize; // For spawn_batch_task compatibility

    // Build payload differently based on whether we're in a structure context
    let payload = if !state.virtual_structure_stack.is_empty() {
        // Inside structure - send as parent_groups with parent key
        use crate::sheets::systems::ai::control_handler::{ParentGroup, ParentKeyInfo};
        
        // Get parent key information from the immediate parent (last in virtual_structure_stack)
        let parent_key = if let Some(parent_ctx) = state.virtual_structure_stack.last() {
            let parent_cat = parent_ctx.parent.parent_category.clone();
            let parent_sheet = parent_ctx.parent.parent_sheet.clone();
            let parent_row_idx = parent_ctx.parent.parent_row;
            
            // Get parent key column and value
            let (key_context, key_value) = if let Some(parent_sheet_obj) = registry.get_sheet(&parent_cat, &parent_sheet) {
                if let Some(parent_meta) = &parent_sheet_obj.metadata {
                    // Find the structure key column
                    let key_col_idx = parent_meta.columns.iter().enumerate()
                        .find_map(|(_i, c)| {
                            if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) {
                                c.structure_key_parent_column_index
                            } else {
                                None
                            }
                        });
                    
                    if let Some(key_idx) = key_col_idx {
                        let context = parent_meta.columns.get(key_idx).and_then(|col| col.ai_context.clone());
                        let value = parent_sheet_obj.grid.get(parent_row_idx)
                            .and_then(|r| r.get(key_idx))
                            .cloned()
                            .unwrap_or_default();
                        (context, value)
                    } else {
                        (None, String::new())
                    }
                } else {
                    (None, String::new())
                }
            } else {
                (None, String::new())
            };
            
            ParentKeyInfo {
                context: key_context,
                key: key_value,
            }
        } else {
            ParentKeyInfo {
                context: None,
                key: String::new(),
            }
        };
        
        // Create parent group with rows (no key prefix in rows)
        let parent_group = ParentGroup {
            parent_key,
            rows: rows_data.clone(),
        };
        
        BatchPayload {
            ai_model_id: model_id,
            general_sheet_rule: rule,
            column_contexts: column_contexts.clone(),
            rows_data: Vec::new(), // Empty for structure requests
            requested_grounding_with_google_search: grounding,
            allow_row_additions: allow_additions_flag,
            key_prefix_count: None,
            key_prefix_headers: None,
            parent_groups: Some(vec![parent_group]),
            user_prompt: user_prompt.clone().unwrap_or_default(),
        }
    } else {
        // Not in structure - send as regular rows_data
        BatchPayload {
            ai_model_id: model_id,
            general_sheet_rule: rule,
            column_contexts: column_contexts.clone(),
            rows_data: rows_data.clone(),
            requested_grounding_with_google_search: grounding,
            allow_row_additions: allow_additions_flag,
            key_prefix_count: None,
            key_prefix_headers: None,
            // parent_groups is only used for structure requests
            parent_groups: None,
            user_prompt: user_prompt.clone().unwrap_or_default(),
        }
    };
    let payload_json = match serde_json::to_string(&payload) { Ok(s)=>s, Err(e)=>{ state.ai_raw_output_display = format!("Serialize error: {}", e); return; } };

    // Update state to submitting
    state.ai_mode = AiModeState::Submitting; state.ai_raw_output_display.clear(); state.ai_row_reviews.clear(); state.ai_new_row_reviews.clear(); state.ai_context_prefix_by_row.clear();
    // Set last_ai_prompt_only flag if prompt was provided without rows
    state.last_ai_prompt_only = selection.is_empty() && user_prompt.is_some();
    state.ai_last_send_root_rows = selection.clone();
    state.ai_last_send_root_category = root_category.clone();
    state.ai_last_send_root_sheet = root_sheet_opt.clone();
    
    // Plan structure paths from root metadata ONLY if we're NOT already inside a structure
    // When inside a structure, we're sending a single-row request and don't want to trigger
    // additional structure processing jobs (which would cause duplicate requests)
    if state.virtual_structure_stack.is_empty() {
        if let Some(root_sheet_name) = &root_sheet_opt { 
            if let Some(root_meta_ref) = registry.get_sheet(&root_category, root_sheet_name).and_then(|s| s.metadata.as_ref()) { 
                state.ai_planned_structure_paths = root_meta_ref.ai_included_structure_paths(); 
            } 
        }
    } else {
        // Clear planned structure paths when inside a structure to prevent duplicate jobs
        state.ai_planned_structure_paths.clear();
        // Also clear any existing structure reviews since we're doing a direct review
        state.ai_structure_reviews.clear();
    }

    // Debug text (pretty) - keep for backward compatibility
    if let Ok(pretty) = serde_json::to_string_pretty(&payload) { 
        state.ai_raw_output_display = format!("--- AI Batch Payload ---\n{}", pretty); 
        // Add to new call log
        state.add_ai_call_log(
            "Sending AI request...".to_string(),
            None,
            Some(pretty),
            false,
        );
    }

    spawn_batch_task(runtime, commands, session_api_key, payload_json, selection, included_indices, key_prefix_count);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_ai_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category: &Option<String>,
    selected_sheet: &Option<String>,
    runtime: &TokioTasksRuntime,
    registry: &SheetRegistry,
    commands: &mut Commands,
    session_api_key: &SessionApiKey,
    toggle_writer: &mut EventWriter<RequestToggleAiRowGeneration>,
    create_group_writer: &mut EventWriter<RequestCreateAiSchemaGroup>,
    rename_group_writer: &mut EventWriter<RequestRenameAiSchemaGroup>,
    select_group_writer: &mut EventWriter<RequestSelectAiSchemaGroup>,
    delete_group_writer: &mut EventWriter<RequestDeleteAiSchemaGroup>,
) {
    ui.horizontal_wrapped(|ui| {
        // Indent under the AI Mode toggle position for unified second-row layout
        if state.last_ai_button_min_x > 0.0 {
            let panel_left = ui.max_rect().min.x;
            let indent = (state.last_ai_button_min_x - panel_left).max(0.0);
            ui.add_space(indent);
        }

        // Left panel (Send button + status + settings gear)
        draw_left_panel_impl(ui, state, registry, selected_category, selected_sheet, session_api_key, Some(runtime), Some(commands));

        // Add Rows toggle - properly resolve from structure context if inside virtual sheet
        if let Some(sheet_name) = selected_sheet.clone() {
            if let Some(meta) = registry.get_sheet(selected_category, &sheet_name).and_then(|s| s.metadata.as_ref()) {
                // Determine root context and structure path
                let (root_category, root_sheet_opt) = state.resolve_root_sheet(registry);
                let structure_path = if !state.virtual_structure_stack.is_empty() {
                    state.ai_cached_included_columns_path.clone()
                } else {
                    Vec::new()
                };
                
                // Resolve the actual value from structure override if applicable
                let mut allow_flag = meta.ai_enable_row_generation;
                let mut sheet_default = meta.ai_enable_row_generation;
                
                if !state.virtual_structure_stack.is_empty() {
                    // Inside virtual structure - resolve from parent structure column
                    if let Some(root_sheet) = root_sheet_opt.as_ref() {
                        if let Some(root_meta) = registry.get_sheet(&root_category, root_sheet).and_then(|s| s.metadata.as_ref()) {
                            sheet_default = root_meta.ai_enable_row_generation;
                            if let Some(override_val) = resolve_structure_override(root_meta, &structure_path) {
                                allow_flag = override_val;
                            } else {
                                allow_flag = sheet_default;
                            }
                        }
                    }
                }
                
                let mut toggle_val = allow_flag;
                let tooltip = if !state.virtual_structure_stack.is_empty() {
                    "Allow AI to append new rows to this structure"
                } else {
                    "Allow AI to append new rows to this sheet"
                };
                let resp = ui.add_enabled(true, egui::Checkbox::new(&mut toggle_val, "Add Rows")).on_hover_text(tooltip);
                
                if resp.changed() {
                    // For structures, calculate override value
                    let (new_structure_path, new_override) = if !state.virtual_structure_stack.is_empty() {
                        let new_override = if toggle_val == sheet_default {
                            None
                        } else {
                            Some(toggle_val)
                        };
                        (Some(structure_path), new_override)
                    } else {
                        (None, None)
                    };
                    
                    let (event_category, event_sheet) = if !state.virtual_structure_stack.is_empty() {
                        (root_category.clone(), root_sheet_opt.unwrap_or(sheet_name.clone()))
                    } else {
                        (selected_category.clone(), sheet_name.clone())
                    };
                    
                    toggle_writer.write(RequestToggleAiRowGeneration {
                        category: event_category,
                        sheet_name: event_sheet,
                        enabled: toggle_val,
                        structure_path: new_structure_path,
                        structure_override: new_override,
                    });
                }
            }
        }

        // Group panel (only when at root structure view)
        if state.virtual_structure_stack.is_empty() { if let Some(sheet_name) = selected_sheet.clone() { let meta_opt = registry.get_sheet(selected_category, &sheet_name).and_then(|s| s.metadata.as_ref()); draw_group_panel(ui, state, selected_category, &sheet_name, meta_opt, create_group_writer, rename_group_writer, select_group_writer, delete_group_writer); } }

        // Review Batch (if results ready)
        if state.ai_mode == AiModeState::ResultsReady { let total = state.ai_row_reviews.len() + state.ai_new_row_reviews.len(); if ui.add_enabled(total>0, egui::Button::new(format!("📋 Review Batch ({} rows)", total))).clicked() { state.ai_batch_review_active = true; state.ai_mode = AiModeState::Reviewing; } }

        if state.ai_mode == AiModeState::Submitting { ui.spinner(); }
    });

    // AI Prompt Popup (when no rows selected)
    show_ai_prompt_popup(ui.ctx(), state, registry, runtime, commands, session_api_key);
}
