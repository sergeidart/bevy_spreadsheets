// src/ui/elements/editor/editor_sheet_display.rs
// Main orchestrator for editor sheet display
// Delegates to specialized display modules

pub mod display_controls;
pub mod display_helpers;
pub mod display_table;

use crate::sheets::{
    events::{
        AddSheetRowRequest, OpenStructureViewEvent, RequestAddColumn,
        RequestBatchUpdateColumnAiInclude, RequestCopyCell, RequestPasteCell, RequestReorderColumn,
        RequestToggleAiRowGeneration, RequestUpdateAiSendSchema, RequestUpdateAiStructureSend,
        RequestUpdateColumnAiInclude, UpdateCellEvent,
    },
    resources::{ClipboardBuffer, SheetRegistry, SheetRenderCache},
    systems::ui_handlers,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use bevy_egui::egui;

use display_controls::render_floating_controls;
use display_helpers::{
    compute_visible_columns, restore_original_selection, try_apply_virtual_override,
    validate_virtual_structure_stack,
};
use display_table::build_and_render_table;

#[allow(clippy::too_many_arguments)]
pub(super) fn show_sheet_table(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    row_height: f32,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    reorder_column_writer: EventWriter<RequestReorderColumn>,
    cell_update_writer: EventWriter<UpdateCellEvent>,
    open_structure_writer: EventWriter<OpenStructureViewEvent>,
    toggle_add_rows_writer: EventWriter<RequestToggleAiRowGeneration>,
    column_include_writer: EventWriter<RequestUpdateColumnAiInclude>,
    batch_include_writer: EventWriter<RequestBatchUpdateColumnAiInclude>,
    send_schema_writer: EventWriter<RequestUpdateAiSendSchema>,
    structure_send_writer: EventWriter<RequestUpdateAiStructureSend>,
    add_row_writer: EventWriter<AddSheetRowRequest>,
    add_column_writer: EventWriter<RequestAddColumn>,
    copy_writer: EventWriter<RequestCopyCell>,
    paste_writer: EventWriter<RequestPasteCell>,
    clipboard_buffer: &ClipboardBuffer,
) {
    // Validate virtual structure stack
    validate_virtual_structure_stack(state);

    // Try to apply virtual sheet override if active
    let backup_sheet = state.selected_sheet_name.clone();
    let used_virtual_override = try_apply_virtual_override(state, registry);

    // Main rendering logic
    if let Some(selected_name) = &state.selected_sheet_name.clone() {
        let current_category = state.selected_category.clone();

        // Check if sheet exists
        let sheet_data_ref_opt = registry.get_sheet(&current_category, selected_name);

        if sheet_data_ref_opt.is_none() {
            render_sheet_not_found(ui, &current_category, selected_name, state);
            restore_original_selection(state, backup_sheet, used_virtual_override);
            return;
        }

        // Render sheet if it has metadata
        if let Some(sheet_data_ref) = sheet_data_ref_opt {
            if let Some(metadata) = &sheet_data_ref.metadata {
                render_sheet_with_metadata(
                    ui,
                    ctx,
                    row_height,
                    state,
                    registry,
                    render_cache,
                    metadata,
                    selected_name,
                    &current_category,
                    reorder_column_writer,
                    cell_update_writer,
                    open_structure_writer,
                    toggle_add_rows_writer,
                    column_include_writer,
                    batch_include_writer,
                    send_schema_writer,
                    structure_send_writer,
                    add_row_writer,
                    add_column_writer,
                    copy_writer,
                    paste_writer,
                    clipboard_buffer,
                );
            } else {
                render_metadata_missing(ui, &current_category, selected_name);
            }
        }
    } else {
        render_no_selection(ui, state);
    }

    // Restore original selection if virtual override was used
    restore_original_selection(state, backup_sheet, used_virtual_override);
}

/// Renders error message when sheet is not found
fn render_sheet_not_found(
    ui: &mut egui::Ui,
    category: &Option<String>,
    sheet_name: &str,
    state: &mut EditorWindowState,
) {
    warn!(
        "Selected sheet '{:?}/{}' not found in registry for rendering.",
        category, sheet_name
    );
    ui.vertical_centered(|ui| {
        ui.label(format!(
            "Sheet '{:?}/{}' no longer exists...",
            category, sheet_name
        ));
    });

    if state.selected_sheet_name.as_deref() == Some(sheet_name) {
        state.selected_sheet_name = None;
        state.reset_interaction_modes_and_selections();
        state.force_filter_recalculation = true;
    }
}

/// Renders the sheet with full metadata and table
#[allow(clippy::too_many_arguments)]
fn render_sheet_with_metadata(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    row_height: f32,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    metadata: &crate::sheets::definitions::SheetMetadata,
    selected_name: &str,
    current_category: &Option<String>,
    reorder_column_writer: EventWriter<RequestReorderColumn>,
    cell_update_writer: EventWriter<UpdateCellEvent>,
    open_structure_writer: EventWriter<OpenStructureViewEvent>,
    toggle_add_rows_writer: EventWriter<RequestToggleAiRowGeneration>,
    column_include_writer: EventWriter<RequestUpdateColumnAiInclude>,
    batch_include_writer: EventWriter<RequestBatchUpdateColumnAiInclude>,
    send_schema_writer: EventWriter<RequestUpdateAiSendSchema>,
    structure_send_writer: EventWriter<RequestUpdateAiStructureSend>,
    add_row_writer: EventWriter<AddSheetRowRequest>,
    add_column_writer: EventWriter<RequestAddColumn>,
    copy_writer: EventWriter<RequestCopyCell>,
    paste_writer: EventWriter<RequestPasteCell>,
    clipboard_buffer: &ClipboardBuffer,
) {
    let num_cols = metadata.columns.len();

    // Compute visible columns
    let visible_columns = compute_visible_columns(state, current_category, selected_name, metadata);
    let num_visible_cols = visible_columns.len();

    // Consistency check
    let total_filters_including_empty = metadata.get_filters().len();
    if total_filters_including_empty != num_cols && num_cols > 0 {
        error!(
            "Metadata inconsistency detected (cols vs filters) for sheet '{:?}/{}'. Revalidation might be needed.",
            current_category, selected_name
        );
        ui.colored_label(
            egui::Color32::RED,
            "Metadata inconsistency detected...",
        );
        return;
    }

    // Gather ancestor key columns for virtual structure sheets
    let ancestor_key_columns =
        ui_handlers::build_ancestor_key_columns(state, registry, selected_name);

    // Build and render the table
    let table_start_pos = build_and_render_table(
        ui,
        ctx,
        row_height,
        state,
        registry,
        render_cache,
        metadata,
        selected_name,
        current_category,
        &visible_columns,
        &ancestor_key_columns,
        reorder_column_writer,
        cell_update_writer,
        open_structure_writer,
        toggle_add_rows_writer,
        column_include_writer,
        batch_include_writer,
        send_schema_writer,
        structure_send_writer,
        copy_writer,
        paste_writer,
        clipboard_buffer,
    );

    // Render floating controls (Add Row, Add Column buttons)
    render_floating_controls(
        ctx,
        state,
        table_start_pos,
        row_height,
        num_visible_cols,
        add_row_writer,
        add_column_writer,
    );
}

/// Renders error message when metadata is missing
fn render_metadata_missing(ui: &mut egui::Ui, category: &Option<String>, sheet_name: &str) {
    warn!(
        "Metadata object missing for sheet '{:?}/{}' even though sheet data exists.",
        category, sheet_name
    );
    ui.colored_label(
        egui::Color32::YELLOW,
        format!("Metadata missing for sheet '{:?}/{}'.", category, sheet_name),
    );
}

/// Renders placeholder when no sheet is selected
fn render_no_selection(ui: &mut egui::Ui, state: &EditorWindowState) {
    if state.selected_category.is_some() {
        ui.vertical_centered(|ui| {
            ui.label("Select a sheet from the category, or upload a new one.");
        });
    } else {
        ui.vertical_centered(|ui| {
            ui.label("Select a category and sheet, or upload JSON.");
        });
    }
}
