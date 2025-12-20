use crate::sheets::events::{AddSheetRowRequest, UpdateCellEvent};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai_review::cache_handlers::cancel_batch;
use crate::sheets::systems::ai_review::display_context::prepare_display_context;
use crate::sheets::systems::ai_review::review_logic::{
    hydrate_structure_detail_if_needed, process_accept_all_normal_mode,
    process_accept_all_structure_mode, process_decline_all_structure_mode, should_auto_exit,
    update_review_state_flags,
};
use crate::ui::elements::ai_review::handlers::{
    finalize_if_empty, process_existing_accept, process_existing_decline, process_new_accept,
    process_new_decline,
};
use crate::ui::elements::ai_review::header_actions::draw_header_actions;
use crate::ui::elements::ai_review::render::row_render::{build_blocks, render_rows, RowContext};
use crate::ui::elements::ai_review::review_processing::{
    populate_linked_column_options, precompute_plans, process_structure_review_changes,
};
use crate::ui::elements::ai_review::table_headers::render_table_headers;
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use bevy_egui::egui;
pub(crate) fn draw_ai_batch_review_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    selected_sheet_name_clone: &Option<String>,
    registry: &SheetRegistry,
    cell_update_writer: &mut EventWriter<UpdateCellEvent>,
    add_row_writer: &mut EventWriter<AddSheetRowRequest>,
) {
    if !state.ai_batch_review_active {
        return;
    }

    // Hydrate structure detail mode if needed
    if state.ai_structure_detail_context.is_some() {
        hydrate_structure_detail_if_needed(state);
    }

    // Auto-exit if nothing left
    if should_auto_exit(state, state.ai_structure_detail_context.is_some()) {
        cancel_batch(state, None);
        return;
    }

    // Prepare display context (all edge case logic extracted)
    let Some(display_ctx) = prepare_display_context(
        state,
        selected_category_clone,
        selected_sheet_name_clone,
        registry,
    ) else {
        return;
    };

    // Update review state flags
    update_review_state_flags(state);

    // Draw header actions (now includes navigation back button support)
    let actions = draw_header_actions(ui, state, display_ctx.show_pending_structures, registry);

    // Process accept all action
    if actions.accept_all {
        if display_ctx.in_structure_mode {
            if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
                process_accept_all_structure_mode(state, detail_ctx);
            } else if !state.ai_navigation_stack.is_empty() {
                // Navigation drilldown mode - accept all temp rows in current view
                info!("Accept All in navigation drilldown: accepting {} existing + {} new rows", 
                    state.ai_row_reviews.len(), state.ai_new_row_reviews.len());
                // This will be handled by the normal processing logic below
                // Just fall through without early return
            }
        } else {
            process_accept_all_normal_mode(
                state,
                selected_category_clone,
                &display_ctx.active_sheet_name,
                cell_update_writer,
                add_row_writer,
                registry,
            );
            return;
        }
    }

    let (blocks, group_starts) = build_blocks(state);

    // Get the active sheet grid for structure column access
    let active_sheet_grid = registry
        .get_sheet(&display_ctx.active_category, &display_ctx.active_sheet_name)
        .map(|sheet| &sheet.grid);

    // Wrap table in ScrollArea to ensure proper scrolling in all modes
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .id_salt("ai_batch_review_table_mod")
        .show(ui, |ui| {
            use bevy_egui::egui::Align;
            use egui_extras::{Column, TableBuilder};

            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(Align::Center))
                .min_scrolled_height(0.0);

            builder = builder.column(Column::exact(120.0));
            for _ in &display_ctx.ancestor_key_columns {
                builder = builder.column(
                    Column::initial(120.0)
                        .at_least(80.0)
                        .resizable(true)
                        .clip(true),
                );
            }
            for _ in &display_ctx.merged_columns {
                builder = builder.column(
                    Column::initial(120.0)
                        .at_least(80.0)
                        .resizable(true)
                        .clip(true),
                );
            }

            // Add header row
            let row_height = 25.0;
            builder
                .header(row_height, |mut header_row| {
                    render_table_headers(
                        &mut header_row,
                        &display_ctx,
                        registry,
                    );
                })
                .body(|mut body| {
                    let mut existing_accept = Vec::new();
                    let mut existing_cancel = Vec::new();
                    let mut new_accept = Vec::new();
                    let mut new_cancel = Vec::new();
                    let mut structure_nav_clicked: Option<(
                        Option<usize>,
                        Option<usize>,
                        Vec<usize>,
                    )> = None;
                    let mut structure_quick_accept: Vec<(
                        Option<usize>,
                        Option<usize>,
                        Vec<usize>,
                    )> = Vec::new();

                    // Clone structure reviews for reading (they're only needed for display, not mutation)
                    let _ai_structure_reviews = state.ai_structure_reviews.clone();

                    // Get sheet metadata for column validators
                    let sheet_metadata = registry
                        .get_sheet(&display_ctx.active_category, &display_ctx.active_sheet_name)
                        .and_then(|sheet| sheet.metadata.as_ref());

                    // Pre-fetch all linked column options
                    let linked_column_options = populate_linked_column_options(
                        state,
                        registry,
                        &display_ctx,
                        sheet_metadata,
                    );

                    // Pre-compute all plans (cache them for this frame)
                    let (ai_plans_cache, original_plans_cache) = precompute_plans(
                        state,
                        &_ai_structure_reviews,
                        &display_ctx,
                        &blocks,
                        &linked_column_options,
                    );

                    render_rows(
                        &mut body,
                        RowContext {
                            state,
                            ancestor_key_columns: &display_ctx.ancestor_key_columns,
                            merged_columns: &display_ctx.merged_columns,
                            blocks: &blocks,
                            group_start_indices: &group_starts,
                            existing_accept: &mut existing_accept,
                            existing_cancel: &mut existing_cancel,
                            new_accept: &mut new_accept,
                            new_cancel: &mut new_cancel,
                            active_sheet_grid,
                            sheet_metadata,
                            registry,
                            linked_column_options: &linked_column_options,
                            structure_nav_clicked: &mut structure_nav_clicked,
                            structure_quick_accept: &mut structure_quick_accept,
                            ai_plans_cache: &ai_plans_cache,
                            original_plans_cache: &original_plans_cache,
                        },
                    );
                    
                    if !new_accept.is_empty() || !new_cancel.is_empty() || !existing_accept.is_empty() || !existing_cancel.is_empty() {
                        info!("AFTER render_rows: new_accept={}, new_cancel={}, existing_accept={}, existing_cancel={}", 
                            new_accept.len(), new_cancel.len(), existing_accept.len(), existing_cancel.len());
                    }

                    if actions.accept_all {
                        if display_ctx.in_structure_mode && !state.ai_navigation_stack.is_empty() {
                            // Navigation drilldown mode - accept all temp rows
                            existing_accept.extend(0..state.ai_row_reviews.len());
                            new_accept.extend(0..state.ai_new_row_reviews.len());
                            info!("Accept All triggered: accepting {} existing + {} new rows", 
                                state.ai_row_reviews.len(), state.ai_new_row_reviews.len());
                        }
                    }

                    if actions.decline_all {
                        if display_ctx.in_structure_mode {
                            process_decline_all_structure_mode(state);
                        } else {
                            existing_cancel.extend(0..state.ai_row_reviews.len());
                            new_cancel.extend(0..state.ai_new_row_reviews.len());
                        }
                    }

                    // Process quick accepts from context menu
                    for (parent_row, parent_new_row, path) in structure_quick_accept {
                        if let Some(entry) = state.ai_structure_reviews.iter_mut().find(|sr| {
                            sr.parent_row_index == parent_row.unwrap_or(usize::MAX)
                                && sr.parent_new_row_index == parent_new_row
                                && sr.structure_path == path
                        }) {
                            // Mark as accepted and decided
                            entry.accepted = true;
                            entry.rejected = false;
                            entry.decided = true;
                            // Use merged_rows if populated (contains user edits), otherwise use ai_rows
                            if entry.merged_rows.is_empty() {
                                entry.merged_rows = entry.ai_rows.clone();
                            }
                        }
                    }

                    existing_accept.sort_unstable();
                    existing_accept.dedup();
                    existing_cancel.sort_unstable();
                    existing_cancel.dedup();
                    existing_cancel.retain(|i| !existing_accept.contains(i));

                    new_accept.sort_unstable();
                    new_accept.dedup();
                    new_cancel.sort_unstable();
                    new_cancel.dedup();
                    new_cancel.retain(|i| !new_accept.contains(i));

                    if !new_accept.is_empty() || !new_cancel.is_empty() || !existing_accept.is_empty() || !existing_cancel.is_empty() {
                        info!("Processing actions: existing_accept={}, existing_cancel={}, new_accept={}, new_cancel={}, in_structure_mode={}", 
                            existing_accept.len(), existing_cancel.len(), new_accept.len(), new_cancel.len(), display_ctx.in_structure_mode);
                    }

                    if display_ctx.in_structure_mode {
                        process_structure_review_changes(
                            state,
                            registry,
                            &existing_accept,
                            &existing_cancel,
                            &new_accept,
                            &new_cancel,
                        );
                    } else {
                        if !existing_accept.is_empty() {
                            process_existing_accept(
                                &existing_accept,
                                state,
                                selected_category_clone,
                                &display_ctx.active_sheet_name,
                                cell_update_writer,
                                registry,
                            );
                        }

                        if !existing_cancel.is_empty() {
                            process_existing_decline(&existing_cancel, state);
                        }

                        if !new_accept.is_empty() {
                            process_new_accept(
                                &new_accept,
                                state,
                                selected_category_clone,
                                &display_ctx.active_sheet_name,
                                cell_update_writer,
                                add_row_writer,
                                registry,
                            );
                        }

                        if !new_cancel.is_empty() {
                            process_new_decline(&new_cancel, state);
                        }
                    }

                    // Handle structure navigation click - drill into child table using new navigation system
                    if let Some((parent_row_idx, parent_new_row_idx, structure_path)) =
                        structure_nav_clicked
                    {
                        use crate::ui::elements::ai_review::navigation;
                        
                        // Call drill_into_structure with both indices to properly handle existing vs new rows
                        if parent_row_idx.is_some() || parent_new_row_idx.is_some() {
                            navigation::drill_into_structure(
                                state,
                                structure_path[0],  // column_index is first element of path
                                parent_row_idx,
                                parent_new_row_idx,
                                registry,
                            );
                        } else {
                            warn!("Structure navigation clicked but no parent index available");
                        }
                    }
                });
        });

    // Restore original reviews if we were in structure mode
    // Do NOT restore saved reviews; structure mode maintains its own working copy until exit

    finalize_if_empty(state);
    if !state.ai_batch_review_active {
        cancel_batch(state, None);
    }
}
