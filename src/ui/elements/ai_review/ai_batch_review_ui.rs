use crate::sheets::definitions::ColumnValidator;
use crate::sheets::events::{AddSheetRowRequest, OpenStructureViewEvent, UpdateCellEvent};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai_review::cache_handlers::cancel_batch;
use crate::sheets::systems::ai_review::display_context::prepare_display_context;
use crate::sheets::systems::ai_review::review_logic::{
    hydrate_structure_detail_if_needed, process_accept_all_normal_mode,
    process_accept_all_structure_mode, process_decline_all_structure_mode, should_auto_exit,
    update_review_state_flags, ColumnEntry,
};
use crate::sheets::systems::ai_review::structure_persistence::{
    structure_row_apply_existing, structure_row_apply_new,
};
use crate::ui::elements::ai_review::handlers::{
    finalize_if_empty, process_existing_accept, process_existing_decline, process_new_accept,
    process_new_decline,
};
use crate::ui::elements::ai_review::header_actions::draw_header_actions;
use crate::ui::elements::ai_review::render::row_render::{build_blocks, render_rows, RowContext};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use bevy_egui::egui::{self, RichText};
pub(crate) fn draw_ai_batch_review_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    selected_sheet_name_clone: &Option<String>,
    registry: &SheetRegistry,
    _open_structure_writer: &mut EventWriter<OpenStructureViewEvent>,
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
        cancel_batch(state);
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

    // Draw header actions
    let actions = draw_header_actions(ui, state, display_ctx.show_pending_structures);

    // Process accept all action
    if actions.accept_all {
        if display_ctx.in_structure_mode {
            if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
                process_accept_all_structure_mode(state, detail_ctx);
            }
        } else {
            process_accept_all_normal_mode(
                state,
                selected_category_clone,
                &display_ctx.active_sheet_name,
                cell_update_writer,
                add_row_writer,
            );
            return;
        }
    }

    let (blocks, group_starts) = build_blocks(state);

    // Get the active sheet grid for structure column access
    let active_sheet_grid = registry
        .get_sheet(selected_category_clone, &display_ctx.active_sheet_name)
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
                    // First column: Action/Status header
                    header_row.col(|ui| {
                        ui.label(RichText::new("Action").strong());
                        let rect = ui.max_rect();
                        let y = rect.bottom();
                        ui.painter().hline(
                            rect.x_range(),
                            y,
                            ui.visuals().widgets.noninteractive.bg_stroke,
                        );
                    });

                    // Ancestor key columns (green)
                    for (key_header, value) in &display_ctx.ancestor_key_columns {
                        header_row.col(|ui| {
                            let r = ui.colored_label(
                                egui::Color32::from_rgb(0, 170, 0),
                                RichText::new(key_header).strong(),
                            );
                            if !value.is_empty() {
                                r.on_hover_text(format!("Key value: {}", value));
                            } else {
                                r.on_hover_text(format!("Key column: {}", key_header));
                            }
                            let rect = ui.max_rect();
                            let y = rect.bottom();
                            ui.painter().hline(
                                rect.x_range(),
                                y,
                                ui.visuals().widgets.noninteractive.bg_stroke,
                            );
                        });
                    }

                    // Regular and structure columns
                    let sheet_metadata = registry
                        .get_sheet(selected_category_clone, &display_ctx.active_sheet_name)
                        .and_then(|sheet| sheet.metadata.as_ref());

                    for col_entry in &display_ctx.merged_columns {
                        header_row.col(|ui| {
                            let header_text = match col_entry {
                                ColumnEntry::Regular(col_idx) => {
                                    if display_ctx.in_structure_mode {
                                        // In structure mode, use structure schema
                                        display_ctx.structure_schema
                                            .get(*col_idx)
                                            .map(|field| field.header.as_str())
                                            .unwrap_or("?")
                                    } else {
                                        // In normal mode, use sheet metadata
                                        sheet_metadata
                                            .and_then(|meta| meta.columns.get(*col_idx))
                                            .map(|col| col.header.as_str())
                                            .unwrap_or("?")
                                    }
                                }
                                ColumnEntry::Structure(col_idx) => {
                                    if display_ctx.in_structure_mode {
                                        // In structure mode, use structure schema
                                        display_ctx.structure_schema
                                            .get(*col_idx)
                                            .map(|field| field.header.as_str())
                                            .unwrap_or("Structure")
                                    } else {
                                        // In normal mode, use sheet metadata
                                        sheet_metadata
                                            .and_then(|meta| meta.columns.get(*col_idx))
                                            .map(|col| col.header.as_str())
                                            .unwrap_or("Structure")
                                    }
                                }
                            };

                            // Color parent_key header green to indicate non-interactable key column
                            if header_text.eq_ignore_ascii_case("parent_key") {
                                ui.label(RichText::new(header_text).color(egui::Color32::from_rgb(0, 170, 0)).strong());
                            } else {
                                ui.label(RichText::new(header_text).strong());
                            }
                            let rect = ui.max_rect();
                            let y = rect.bottom();
                            ui.painter().hline(
                                rect.x_range(),
                                y,
                                ui.visuals().widgets.noninteractive.bg_stroke,
                            );
                        });
                    }
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
                    let ai_structure_reviews = state.ai_structure_reviews.clone();

                    // Get sheet metadata for column validators
                    let sheet_metadata = registry
                        .get_sheet(selected_category_clone, &display_ctx.active_sheet_name)
                        .and_then(|sheet| sheet.metadata.as_ref());

                    // Pre-fetch all linked column options
                    use crate::ui::widgets::linked_column_cache::{self, CacheResult};
                    let mut linked_column_options = std::collections::HashMap::new();

                    // For nested structures, use structure_schema; otherwise use sheet metadata columns
                    if display_ctx.in_structure_mode && !display_ctx.structure_schema.is_empty() {
                        // In structure detail mode: get validators from structure schema
                        for col_entry in &display_ctx.merged_columns {
                            if let ColumnEntry::Regular(actual_col) = col_entry {
                                if let Some(field_def) = display_ctx.structure_schema.get(*actual_col) {
                                    if let Some(ColumnValidator::Linked {
                                        target_sheet_name,
                                        target_column_index,
                                    }) = &field_def.validator
                                    {
                                        if let CacheResult::Success { raw, .. } =
                                            linked_column_cache::get_or_populate_linked_options(
                                                target_sheet_name,
                                                *target_column_index,
                                                registry,
                                                state,
                                            )
                                        {
                                            linked_column_options.insert(*actual_col, raw);
                                        }
                                    }
                                }
                            }
                        }
                    } else if let Some(meta) = sheet_metadata {
                        // Normal mode or virtual structure review: get validators from sheet metadata
                        for col_entry in &display_ctx.merged_columns {
                            if let ColumnEntry::Regular(actual_col) = col_entry {
                                if let Some(col_def) = meta.columns.get(*actual_col) {
                                    if let Some(ColumnValidator::Linked {
                                        target_sheet_name,
                                        target_column_index,
                                    }) = &col_def.validator
                                    {
                                        if let CacheResult::Success { raw, .. } =
                                            linked_column_cache::get_or_populate_linked_options(
                                                target_sheet_name,
                                                *target_column_index,
                                                registry,
                                                state,
                                            )
                                        {
                                            linked_column_options.insert(*actual_col, raw);
                                        }
                                    }
                                }
                            }
                        }
                    }

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
                            ai_structure_reviews: &ai_structure_reviews,
                            sheet_metadata,
                            registry,
                            linked_column_options: &linked_column_options,
                            structure_nav_clicked: &mut structure_nav_clicked,
                            structure_quick_accept: &mut structure_quick_accept,
                        },
                    );

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

                    if display_ctx.in_structure_mode {
                        if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
                            if let Some(entry_index) =
                                state.ai_structure_reviews.iter().position(|sr| {
                                    match (sr.parent_new_row_index, detail_ctx.parent_new_row_index)
                                    {
                                        (Some(a), Some(b)) if a == b => {
                                            sr.structure_path == detail_ctx.structure_path
                                        }
                                        (None, None) => {
                                            sr.parent_row_index
                                                == detail_ctx.parent_row_index.unwrap_or(usize::MAX)
                                                && sr.structure_path == detail_ctx.structure_path
                                        }
                                        _ => false,
                                    }
                                })
                            {
                                let entry_ptr: *mut _ =
                                    &mut state.ai_structure_reviews[entry_index];
                                // Safe because we don't move state.ai_structure_reviews while using entry_ptr
                                unsafe {
                                    let entry = &mut *entry_ptr;
                                    // Existing accepts
                                    for &idx in &existing_accept {
                                        if let Some(rr) = state.ai_row_reviews.get(idx) {
                                            structure_row_apply_existing(entry, rr, true);
                                        }
                                    }
                                    for &idx in &existing_cancel {
                                        if let Some(rr) = state.ai_row_reviews.get(idx) {
                                            structure_row_apply_existing(entry, rr, false);
                                        }
                                    }
                                    // Remove existing rows from temp view (reverse order to keep indices valid)
                                    if !existing_accept.is_empty() || !existing_cancel.is_empty() {
                                        let mut to_remove: Vec<usize> = Vec::new();
                                        to_remove.extend(existing_accept.iter().cloned());
                                        to_remove.extend(existing_cancel.iter().cloned());
                                        to_remove.sort_unstable();
                                        to_remove.dedup();
                                        for idx in to_remove.into_iter().rev() {
                                            if idx < state.ai_row_reviews.len() {
                                                state.ai_row_reviews.remove(idx);
                                            }
                                        }
                                        // CRITICAL: Update row_index in remaining RowReview entries after removal
                                        // Row indices must match their position in the arrays (original_rows, merged_rows, etc.)
                                        for (new_idx, rr) in
                                            state.ai_row_reviews.iter_mut().enumerate()
                                        {
                                            rr.row_index = new_idx;
                                        }
                                    }
                                    // New rows
                                    for &idx in &new_accept {
                                        structure_row_apply_new(
                                            entry,
                                            idx,
                                            &state.ai_new_row_reviews,
                                            true,
                                        );
                                    }
                                    for &idx in &new_cancel {
                                        structure_row_apply_new(
                                            entry,
                                            idx,
                                            &state.ai_new_row_reviews,
                                            false,
                                        );
                                    }
                                    // Remove accepted/declined new rows from temp view to mimic top-level behavior
                                    if !new_accept.is_empty() || !new_cancel.is_empty() {
                                        let mut to_remove: Vec<usize> = Vec::new();
                                        to_remove.extend(new_accept.iter().cloned());
                                        to_remove.extend(new_cancel.iter().cloned());
                                        to_remove.sort_unstable();
                                        to_remove.dedup();
                                        for idx in to_remove.into_iter().rev() {
                                            if idx < state.ai_new_row_reviews.len() {
                                                state.ai_new_row_reviews.remove(idx);
                                            }
                                        }
                                    }
                                    // Mark entry has changes
                                    entry.has_changes = true;
                                    // Auto-mark decided and accepted if no remaining temp rows left
                                    let no_temp_rows = state.ai_row_reviews.is_empty()
                                        && state.ai_new_row_reviews.is_empty();
                                    if no_temp_rows {
                                        entry.decided = true;
                                        if entry
                                            .differences
                                            .iter()
                                            .all(|row| row.iter().all(|f| !*f))
                                        {
                                            entry.accepted = true;
                                            entry.rejected = false;
                                        }

                                        // Exit structure detail mode and restore parent level
                                        if let Some(ref detail_ctx) =
                                            state.ai_structure_detail_context.clone()
                                        {
                                            state.ai_row_reviews =
                                                detail_ctx.saved_row_reviews.clone();
                                            state.ai_new_row_reviews =
                                                detail_ctx.saved_new_row_reviews.clone();
                                            state.ai_structure_detail_context = None;
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        if !existing_accept.is_empty() {
                            process_existing_accept(
                                &existing_accept,
                                state,
                                selected_category_clone,
                                &display_ctx.active_sheet_name,
                                cell_update_writer,
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
                            );
                        }

                        if !new_cancel.is_empty() {
                            process_new_decline(&new_cancel, state);
                        }
                    }

                    // Handle structure navigation click
                    if let Some((parent_row_idx, parent_new_row_idx, structure_path)) =
                        structure_nav_clicked
                    {
                        // Find the structure entry to get root sheet information
                        let structure_entry = state.ai_structure_reviews.iter().find(|sr| {
                            match (parent_row_idx, parent_new_row_idx) {
                                (Some(pr), None) => {
                                    sr.parent_row_index == pr
                                        && sr.parent_new_row_index.is_none()
                                        && sr.structure_path == structure_path
                                }
                                (None, Some(pnr)) => {
                                    sr.parent_new_row_index == Some(pnr)
                                        && sr.structure_path == structure_path
                                }
                                _ => false,
                            }
                        });

                        if let Some(entry) = structure_entry {
                            // Save current top-level reviews before entering structure mode
                            let saved_row_reviews = state.ai_row_reviews.clone();
                            let saved_new_row_reviews = state.ai_new_row_reviews.clone();
                            state.ai_structure_detail_context =
                                Some(crate::ui::elements::editor::state::StructureDetailContext {
                                    root_category: entry.root_category.clone(),
                                    root_sheet: entry.root_sheet.clone(),
                                    parent_row_index: parent_row_idx,
                                    parent_new_row_index: parent_new_row_idx,
                                    structure_path,
                                    hydrated: false,
                                    saved_row_reviews,
                                    saved_new_row_reviews,
                                });
                        }
                    }
                });
        });

    // Restore original reviews if we were in structure mode
    // Do NOT restore saved reviews; structure mode maintains its own working copy until exit

    finalize_if_empty(state);
    if !state.ai_batch_review_active {
        cancel_batch(state);
    }
}
