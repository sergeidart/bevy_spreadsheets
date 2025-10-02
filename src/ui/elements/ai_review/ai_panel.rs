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
    let rule = meta.ai_general_rule.clone();
    let grounding = meta.requested_grounding_with_google_search.unwrap_or(false);

    // Row additions flag (structure aware). If inside structure stack, resolve root & overrides similar to old panel.
    let mut allow_additions_flag = meta.ai_enable_row_generation;
    if !state.virtual_structure_stack.is_empty() {
        // Find root meta & structure path like legacy logic.
        let (root_category, root_sheet_opt) = state.resolve_root_sheet(registry);
        if let Some(root_sheet) = root_sheet_opt { if let Some(root_meta) = registry.get_sheet(&root_category, &root_sheet).and_then(|s| s.metadata.as_ref()) { if let Some(override_val) = resolve_structure_override(root_meta, &state.ai_cached_included_columns_path) { allow_additions_flag = override_val; } } }
    }

    // Key prefix (ancestor key columns) for nested structure context. For root sends this is empty.
    let mut key_prefix_headers: Vec<String> = Vec::new();
    let mut key_prefix_values_per_row: Vec<Vec<String>> = Vec::new();
    let mut key_prefix_count = 0usize;
    if !state.virtual_structure_stack.is_empty() {
        // Collect ancestor key columns (headers & values) similar to results reconstruction logic
        for vctx in &state.virtual_structure_stack {
            let anc_cat = vctx.parent.parent_category.clone();
            let anc_sheet = vctx.parent.parent_sheet.clone();
            let anc_row_idx = vctx.parent.parent_row;
            if let Some(sheet) = registry.get_sheet(&anc_cat, &anc_sheet) {
                if let Some(meta) = &sheet.metadata {
                    if let Some(key_col_index) = meta.columns.iter().enumerate().find_map(|(_i,c)| {
                        if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { c.structure_key_parent_column_index } else { None }
                    }) {
                        if let Some(col_def) = meta.columns.get(key_col_index) {
                            key_prefix_headers.push(col_def.header.clone());
                            // Get value for that ancestor row
                            let val = sheet.grid.get(anc_row_idx).and_then(|r| r.get(key_col_index)).cloned().unwrap_or_default();
                            // We'll append later per selected row (same chain for all rows)
                            // For now store chain in a temp row vector
                        }
                    }
                }
            }
        }
        key_prefix_count = key_prefix_headers.len();
        if key_prefix_count > 0 {
            // Build prefix values per selected row: replicate ancestor values chain for each selected root row.
            // (Each ancestor chain describes hierarchical context; values identical for all selected rows given current virtual stack.)
            let mut chain_values: Vec<String> = Vec::new();
            for (idx, header) in key_prefix_headers.iter().enumerate() {
                // Value already looked up above; need to re-fetch to keep code simple.
                if let Some(vctx) = state.virtual_structure_stack.get(idx) {
                    let anc_cat = vctx.parent.parent_category.clone();
                    let anc_sheet = vctx.parent.parent_sheet.clone();
                    let anc_row_idx = vctx.parent.parent_row;
                    let value = registry.get_sheet(&anc_cat, &anc_sheet)
                        .and_then(|s| {
                            let meta = &s.metadata; meta.as_ref()?;
                            let key_col_index = meta.as_ref().and_then(|m| m.columns.iter().enumerate().find_map(|(_i,c)| if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { c.structure_key_parent_column_index } else { None }))?;
                            s.grid.get(anc_row_idx).and_then(|r| r.get(key_col_index)).cloned()
                        }).unwrap_or_default();
                    chain_values.push(value);
                } else {
                    chain_values.push(String::new());
                }
            }
            key_prefix_values_per_row = vec![chain_values; rows_data.len()];
        }
    }

    // Apply key prefix to rows_data copy for payload (do not mutate original mapping arrays yet)
    let mut rows_with_prefix: Vec<Vec<String>> = Vec::with_capacity(rows_data.len());
    if key_prefix_count > 0 {
        for (i, row) in rows_data.iter().enumerate() {
            let mut combined = Vec::with_capacity(key_prefix_count + row.len());
            if let Some(prefix_vals) = key_prefix_values_per_row.get(i) { combined.extend(prefix_vals.clone()); }
            combined.extend(row.clone());
            rows_with_prefix.push(combined);
        }
    } else {
        rows_with_prefix = rows_data.clone();
    }

    let payload = BatchPayload { 
        ai_model_id: model_id, 
        general_sheet_rule: rule, 
        column_contexts: column_contexts.clone(), 
        rows_data: rows_with_prefix.clone(), 
        requested_grounding_with_google_search: grounding, 
        allow_row_additions: allow_additions_flag, 
        // Key is already embedded in rows_data, so don't send these metadata fields
        key_prefix_count: None, 
        key_prefix_headers: None,
        // parent_groups is only used for structure requests
        parent_groups: None,
        user_prompt: user_prompt.clone().unwrap_or_default() 
    };
    let payload_json = match serde_json::to_string(&payload) { Ok(s)=>s, Err(e)=>{ state.ai_raw_output_display = format!("Serialize error: {}", e); return; } };

    // Update state to submitting
    state.ai_mode = AiModeState::Submitting; state.ai_raw_output_display.clear(); state.ai_output_panel_visible = true; state.ai_row_reviews.clear(); state.ai_new_row_reviews.clear(); state.ai_context_prefix_by_row.clear();
    // Set last_ai_prompt_only flag if prompt was provided without rows
    state.last_ai_prompt_only = selection.is_empty() && user_prompt.is_some();
    state.ai_last_send_root_rows = selection.clone();
    state.ai_last_send_root_category = root_category.clone();
    state.ai_last_send_root_sheet = root_sheet_opt.clone();
    // Plan structure paths from root metadata if available
    if let (Some(root_sheet_name), Some(root_meta_sheet)) = (root_sheet_opt.clone(), root_sheet_opt.as_ref().and_then(|sname| registry.get_sheet(&root_category, sname)).and_then(|s| s.metadata.as_ref()).map(|_| root_sheet_opt.clone())) { let _ = root_meta_sheet; }
    if let Some(root_sheet_name) = &root_sheet_opt { if let Some(root_meta_ref) = registry.get_sheet(&root_category, root_sheet_name).and_then(|s| s.metadata.as_ref()) { state.ai_planned_structure_paths = root_meta_ref.ai_included_structure_paths(); } }

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

        // Add Rows toggle (replicating logic simplified: only root sheet toggle for now)
        if let Some(sheet_name) = selected_sheet.clone() { if let Some(meta) = registry.get_sheet(selected_category, &sheet_name).and_then(|s| s.metadata.as_ref()) { let mut allow_flag = meta.ai_enable_row_generation; let mut toggle_val = allow_flag; let resp = ui.add_enabled(true, egui::Checkbox::new(&mut toggle_val, "Add Rows")).on_hover_text("Allow AI to append new rows to this sheet"); if resp.changed() { allow_flag = toggle_val; toggle_writer.write(RequestToggleAiRowGeneration { category: selected_category.clone(), sheet_name: sheet_name.clone(), enabled: allow_flag, structure_path: None, structure_override: None }); } } }

        // Group panel (only when at root structure view)
        if state.virtual_structure_stack.is_empty() { if let Some(sheet_name) = selected_sheet.clone() { let meta_opt = registry.get_sheet(selected_category, &sheet_name).and_then(|s| s.metadata.as_ref()); draw_group_panel(ui, state, selected_category, &sheet_name, meta_opt, create_group_writer, rename_group_writer, select_group_writer, delete_group_writer); } }

        // Review Batch (if results ready)
        if state.ai_mode == AiModeState::ResultsReady { let total = state.ai_row_reviews.len() + state.ai_new_row_reviews.len(); if ui.add_enabled(total>0, egui::Button::new(format!("📋 Review Batch ({} rows)", total))).clicked() { state.ai_batch_review_active = true; state.ai_mode = AiModeState::Reviewing; } }

        if state.ai_mode == AiModeState::Submitting { ui.spinner(); }
    });

    // AI Prompt Popup (when no rows selected)
    show_ai_prompt_popup(ui.ctx(), state, registry, runtime, commands, session_api_key);
}
