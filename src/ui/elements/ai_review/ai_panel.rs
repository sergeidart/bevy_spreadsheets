// New lightweight AI panel orchestrator replacing deprecated ai_control_panel.rs
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;

use crate::{
    sheets::{
        definitions::default_ai_model_id,
        events::{
            RequestCreateAiSchemaGroup, RequestDeleteAiSchemaGroup, RequestRenameAiSchemaGroup,
            RequestSelectAiSchemaGroup, RequestToggleAiRowGeneration,
        },
        resources::SheetRegistry,
    },
    ui::elements::editor::state::{AiModeState, EditorWindowState},
    SessionApiKey,
};

use super::{
    ai_context_utils::{
        build_lineage_prefixes, collect_ai_included_columns,
    },
    ai_control_left_panel::draw_left_panel_impl,
    ai_group_panel::draw_group_panel,
};
use crate::sheets::systems::ai::control_handler::{
    spawn_batch_task, BatchPayload,
};
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
    if selection.is_empty() && user_prompt.is_none() {
        return;
    }
    let category = state.selected_category.clone();
    let sheet_name = state.selected_sheet_name.clone().unwrap_or_default();
    let sheet_opt = registry.get_sheet(&category, &sheet_name);
    let meta_opt = sheet_opt.and_then(|s| s.metadata.as_ref());
    if meta_opt.is_none() {
        return;
    }
    let meta = meta_opt.unwrap();

    // Determine root sheet (for structure planning) before we potentially navigate virtual view
    let (root_category, root_sheet_opt) = state.resolve_root_sheet(registry);

    // Collect non-structure columns to include (influenced by schema groups) and
    // when in a structure sheet: exclude the technical columns (row_index, id), keep 'parent_key'.
    // Treat as structure context when the selected sheet's metadata marks it as a structure table
    let in_structure_sheet = meta.is_structure_table();
    
    let inclusion = collect_ai_included_columns(meta, in_structure_sheet);
    let included_indices = inclusion.included_indices;
    let mut column_contexts = inclusion.column_contexts;
    
    // Gather row data (only included non-structure columns)
    let mut rows_data: Vec<Vec<String>> = Vec::new();
    if let Some(sheet) = sheet_opt {
        info!(
            "AI Request Building: Sheet='{}', included_indices={:?}, column_contexts.len()={}",
            sheet_name,
            included_indices,
            column_contexts.len()
        );
        for row_index in selection.iter().copied() {
            if let Some(row) = sheet.grid.get(row_index) {
                let mut out_row = Vec::new();
                for &ci in &included_indices {
                    let value = row.get(ci).cloned().unwrap_or_default();
                    info!("  Row {} col_idx={}: '{}'", row_index, ci, value);
                    out_row.push(value);
                }
                rows_data.push(out_row);
            }
        }
    }

    let model_id = if meta.ai_model_id.is_empty() {
        default_ai_model_id()
    } else {
        meta.ai_model_id.clone()
    };

    // Use current sheet's general rule directly (virtual structures deprecated)
    let rule = meta.ai_general_rule.clone();

    // Grounding flag: use current metadata directly (virtual structures deprecated)
    let grounding = meta.requested_grounding_with_google_search.unwrap_or(false);

    // Row additions flag (use current metadata directly - virtual structures deprecated)
    let allow_additions_flag = meta.ai_enable_row_generation;

    // Build human-readable ancestor prefixes using programmatic lineage walking.
    // Virtual structure context deprecated; use legacy structure navigation if present.
    let lineage_prefixes = build_lineage_prefixes(state, registry, &selection);
    
    let key_prefix_count = lineage_prefixes.key_prefix_count;
    
    // Prepend lineage prefixes to column contexts and row data if present
    if !lineage_prefixes.prefix_values.is_empty() {
        // Prepend prefix contexts to column_contexts
        let old_contexts_len = column_contexts.len();
        let mut new_contexts: Vec<Option<String>> =
            Vec::with_capacity(lineage_prefixes.key_prefix_count + old_contexts_len);
        new_contexts.extend(lineage_prefixes.prefix_contexts.into_iter());
        new_contexts.extend(column_contexts.into_iter());
        info!(
            "After prepending: new_contexts.len()={} (was {})",
            new_contexts.len(), old_contexts_len
        );
        column_contexts = new_contexts;
        
        // Prepend values to each selected row
        for row in rows_data.iter_mut() {
            info!("Before prepending row: {:?}", row);
            for (i, v) in lineage_prefixes.prefix_values.iter().enumerate() {
                row.insert(i, v.clone());
            }
            info!("After prepending row: {:?}", row);
        }
        
        // Store pairs for review UI fallback
        state.ai_context_prefix_by_row = lineage_prefixes.prefix_pairs_by_row;
    }

    // Build payload differently based on whether we're in a structure context
    let payload = if in_structure_sheet {
        // Inside structure - send FLAT rows (no parent_groups). Rows already include prefixed human-readable context
        BatchPayload {
            ai_model_id: model_id,
            general_sheet_rule: rule,
            column_contexts: column_contexts.clone(),
            rows_data: rows_data.clone(),
            requested_grounding_with_google_search: grounding,
            allow_row_additions: allow_additions_flag,
            // Do not include key_prefix_* metadata in payload
            key_prefix_count: None,
            key_prefix_headers: None,
            parent_groups: None,
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
            // Do not include key_prefix_* metadata in payload
            key_prefix_count: None,
            key_prefix_headers: None,
            // parent_groups is only used for structure requests
            parent_groups: None,
            user_prompt: user_prompt.clone().unwrap_or_default(),
        }
    };
    let payload_json = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            state.ai_raw_output_display = format!("Serialize error: {}", e);
            return;
        }
    };

    // Update state to submitting
    state.ai_mode = AiModeState::Submitting;
    state.ai_raw_output_display.clear();
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    // Preserve ai_context_prefix_by_row so green ancestor columns can render during review
    // Set last_ai_prompt_only flag if prompt was provided without rows
    state.last_ai_prompt_only = selection.is_empty() && user_prompt.is_some();
    state.ai_last_send_root_rows = selection.clone();
    state.ai_last_send_root_category = root_category.clone();
    state.ai_last_send_root_sheet = root_sheet_opt.clone();

    // Plan structure paths from root metadata ONLY if we're NOT already inside a structure sheet
    // When inside a structure sheet, we're sending a single-row request and don't want to trigger
    // additional structure processing jobs (which would cause duplicate requests)
    if !in_structure_sheet {
        if let Some(root_sheet_name) = &root_sheet_opt {
            if let Some(root_meta_ref) = registry
                .get_sheet(&root_category, root_sheet_name)
                .and_then(|s| s.metadata.as_ref())
            {
                state.ai_planned_structure_paths = root_meta_ref.ai_included_structure_paths();
            }
        }
    } else {
        // Clear planned structure paths when inside a structure to prevent duplicate jobs
        state.ai_planned_structure_paths.clear();
        // Also clear any existing structure reviews since we're doing a direct review
        state.ai_structure_reviews.clear();
    }

    // Initialize progress tracking: 1 for Phase 1, potentially 1 for Phase 2, N for structures
    // We'll know the actual count after Phase 1, but estimate conservatively
    state.ai_total_tasks = 1 + state.ai_planned_structure_paths.len();
    state.ai_completed_tasks = 0;

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

    spawn_batch_task(
        runtime,
        commands,
        session_api_key,
        payload_json,
        selection,
        included_indices,
        key_prefix_count,
    );
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
    // NEW: Show navigation breadcrumb with back button when in child table drill-down
    if !state.ai_navigation_stack.is_empty() {
        ui.horizontal(|ui| {
            use super::navigation::navigate_back;
            
            if ui.button("⬅ Back").clicked() {
                navigate_back(state, registry);
            }
            
            ui.separator();
            
            // Build breadcrumb trail
            let mut breadcrumb = String::new();
            for (idx, nav_ctx) in state.ai_navigation_stack.iter().enumerate() {
                if idx > 0 {
                    breadcrumb.push_str(" › ");
                }
                breadcrumb.push_str(&nav_ctx.sheet_name);
                if let Some(ref display_name) = nav_ctx.parent_display_name {
                    breadcrumb.push_str(&format!(" ({})", display_name));
                }
            }
            breadcrumb.push_str(" › ");
            breadcrumb.push_str(&state.ai_current_sheet);
            
            ui.label(egui::RichText::new(breadcrumb).color(egui::Color32::from_rgb(100, 150, 255)));
            
            if let Some(ref parent_filter) = state.ai_parent_filter {
                ui.label(
                    egui::RichText::new(format!("(filtered by parent_key={})", parent_filter.parent_row_index))
                        .italics()
                        .color(egui::Color32::GRAY)
                );
            }
        });
        ui.separator();
    }
    
    ui.horizontal_wrapped(|ui| {
        // Indent under the AI Mode toggle position for unified second-row layout
        if state.last_ai_button_min_x > 0.0 {
            let panel_left = ui.max_rect().min.x;
            let indent = (state.last_ai_button_min_x - panel_left).max(0.0);
            ui.add_space(indent);
        }

        // Left panel (Send button + status + settings gear)
        draw_left_panel_impl(
            ui,
            state,
            registry,
            selected_category,
            selected_sheet,
            session_api_key,
            Some(runtime),
            Some(commands),
        );

        // Add Rows toggle - properly resolve from structure context if inside virtual sheet
        if let Some(sheet_name) = selected_sheet.clone() {
            if let Some(meta) = registry
                .get_sheet(selected_category, &sheet_name)
                .and_then(|s| s.metadata.as_ref())
            {
                // Use current sheet's enable_row_generation directly
                let allow_flag = meta.ai_enable_row_generation;
                let mut toggle_val = allow_flag;
                let tooltip = "Allow AI to append new rows to this sheet";
                let resp = ui
                    .add_enabled(true, egui::Checkbox::new(&mut toggle_val, "Add Rows"))
                    .on_hover_text(tooltip);

                if resp.changed() {
                    // Update directly for current sheet (virtual structures deprecated)
                    toggle_writer.write(RequestToggleAiRowGeneration {
                        category: selected_category.clone(),
                        sheet_name: sheet_name.clone(),
                        enabled: toggle_val,
                        structure_path: None,
                        structure_override: None,
                    });
                }
            }
        }

        // Group panel (always shown - virtual structures deprecated)
        if let Some(sheet_name) = selected_sheet.clone() {
            let meta_opt = registry
                .get_sheet(selected_category, &sheet_name)
                .and_then(|s| s.metadata.as_ref());
            draw_group_panel(
                ui,
                state,
                selected_category,
                &sheet_name,
                meta_opt,
                create_group_writer,
                rename_group_writer,
                select_group_writer,
                delete_group_writer,
            );
        }

        // Review Batch (if results ready)
        if state.ai_mode == AiModeState::ResultsReady {
            let total = state.ai_row_reviews.len() + state.ai_new_row_reviews.len();
            if ui
                .add_enabled(
                    total > 0,
                    egui::Button::new(format!("📋 Review Batch ({} rows)", total)),
                )
                .clicked()
            {
                state.ai_batch_review_active = true;
                state.ai_mode = AiModeState::Reviewing;
                // Initialize navigation context for drill_into_structure support
                state.ai_current_category = state.ai_last_send_root_category.clone();
                state.ai_current_sheet = state.ai_last_send_root_sheet.clone().unwrap_or_default();
            }
        }

        // Show progress when submitting
        if state.ai_mode == AiModeState::Submitting {
            ui.spinner();
            if state.ai_total_tasks > 0 {
                let percentage = (state.ai_completed_tasks as f32 / state.ai_total_tasks as f32
                    * 100.0)
                    .min(100.0);
                ui.label(format!(
                    "{}/{} tasks ({:.0}%)",
                    state.ai_completed_tasks, state.ai_total_tasks, percentage
                ));
            }
        }
    });

    // When in structure-detail context, show a compact back bar above the table
    if state.ai_batch_review_active && state.ai_structure_detail_context.is_some() {
        egui::TopBottomPanel::top("ai_structure_back_bar")
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("⬅ Back to root AI review").clicked() {
                        state.ai_structure_detail_context = None;
                    }
                    if let Some(detail) = &state.ai_structure_detail_context {
                        let path = detail
                            .structure_path
                            .iter()
                            .map(|idx| idx.to_string())
                            .collect::<Vec<_>>()
                            .join(" › ");
                        ui.label(format!("Structure context: {}", path));
                    }
                });
            });
    }

    // AI Prompt Popup (when no rows selected)
    show_ai_prompt_popup(
        ui.ctx(),
        state,
        registry,
        runtime,
        commands,
        session_api_key,
    );
}
