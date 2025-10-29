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
    resolve_structure_override, spawn_batch_task, BatchPayload,
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
    let sheet_name = if let Some(vctx) = state.virtual_structure_stack.last() {
        vctx.virtual_sheet_name.clone()
    } else {
        state.selected_sheet_name.clone().unwrap_or_default()
    };
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
    // Treat as structure context when either:
    // - Navigated into a virtual structure view (virtual_structure_stack), or
    // - The selected sheet's metadata marks it as a structure table
    let in_structure_sheet = !state.virtual_structure_stack.is_empty()
        || meta.is_structure_table();
    
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

    // Use root sheet's general rule when inside a virtual structure
    let rule = if !state.virtual_structure_stack.is_empty() {
        if let Some(root_sheet) = root_sheet_opt.as_ref() {
            if let Some(root_meta) = registry
                .get_sheet(&root_category, root_sheet)
                .and_then(|s| s.metadata.as_ref())
            {
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
            if let Some(root_meta) = registry
                .get_sheet(&root_category, root_sheet)
                .and_then(|s| s.metadata.as_ref())
            {
                root_meta
                    .requested_grounding_with_google_search
                    .unwrap_or(false)
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
            if let Some(root_meta) = registry
                .get_sheet(&root_category, root_sheet)
                .and_then(|s| s.metadata.as_ref())
            {
                // Start with root sheet's default
                let sheet_default = root_meta.ai_enable_row_generation;
                // Then try to get structure-specific override
                if let Some(override_val) =
                    resolve_structure_override(root_meta, &state.ai_cached_included_columns_path)
                {
                    allow_additions_flag = override_val;
                } else {
                    // No explicit override, use root sheet's default
                    allow_additions_flag = sheet_default;
                }
            }
        }
    }

    // Build human-readable ancestor prefixes using programmatic lineage walking.
    // Prefer virtual structure context; fall back to legacy structure navigation if present.
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
                        if let Some(root_meta) = registry
                            .get_sheet(&root_category, root_sheet)
                            .and_then(|s| s.metadata.as_ref())
                        {
                            sheet_default = root_meta.ai_enable_row_generation;
                            if let Some(override_val) =
                                resolve_structure_override(root_meta, &structure_path)
                            {
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
                let resp = ui
                    .add_enabled(true, egui::Checkbox::new(&mut toggle_val, "Add Rows"))
                    .on_hover_text(tooltip);

                if resp.changed() {
                    // For structures, calculate override value
                    let (new_structure_path, new_override) =
                        if !state.virtual_structure_stack.is_empty() {
                            let new_override = if toggle_val == sheet_default {
                                None
                            } else {
                                Some(toggle_val)
                            };
                            (Some(structure_path), new_override)
                        } else {
                            (None, None)
                        };

                    let (event_category, event_sheet) = if !state.virtual_structure_stack.is_empty()
                    {
                        (
                            root_category.clone(),
                            root_sheet_opt.unwrap_or(sheet_name.clone()),
                        )
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
        if state.virtual_structure_stack.is_empty() {
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
