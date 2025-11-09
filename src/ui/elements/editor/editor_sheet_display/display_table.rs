// src/ui/elements/editor/display/display_table.rs
// Table building and rendering logic

use crate::sheets::{
    events::{
        RequestBatchUpdateColumnAiInclude,
        RequestCopyCell, RequestPasteCell, RequestReorderColumn, RequestToggleAiRowGeneration,
        RequestUpdateAiSendSchema, RequestUpdateAiStructureSend, RequestUpdateColumnAiInclude,
        UpdateCellEvent,
    },
    resources::{ClipboardBuffer, SheetRegistry, SheetRenderCache},
};
use crate::ui::elements::editor::editor_sheet_display::display_helpers::{
    build_table_columns, render_control_cell, render_data_cell,
};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::elements::editor::table_body::get_filtered_row_indices_cached;
use crate::ui::elements::editor::table_header::sheet_table_header;
use bevy::prelude::*;
use bevy_egui::egui;
use egui_extras::{TableBody, TableBuilder};

/// Builds and renders the table with all columns, headers, and body
#[allow(clippy::too_many_arguments)]
pub fn build_and_render_table(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    row_height: f32,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    metadata: &crate::sheets::definitions::SheetMetadata,
    selected_name: &str,
    current_category: &Option<String>,
    visible_columns: &[usize],
    ancestor_key_columns: &[(String, String)],
    reorder_column_writer: EventWriter<RequestReorderColumn>,
    cell_update_writer: EventWriter<UpdateCellEvent>,
    toggle_add_rows_writer: EventWriter<RequestToggleAiRowGeneration>,
    column_include_writer: EventWriter<RequestUpdateColumnAiInclude>,
    batch_include_writer: EventWriter<RequestBatchUpdateColumnAiInclude>,
    send_schema_writer: EventWriter<RequestUpdateAiSendSchema>,
    structure_send_writer: EventWriter<RequestUpdateAiStructureSend>,
    copy_writer: EventWriter<RequestCopyCell>,
    paste_writer: EventWriter<RequestPasteCell>,
    clipboard_buffer: &ClipboardBuffer,
) -> egui::Pos2 {
    let table_start_pos = ui.next_widget_position();
    
    let _scroll_resp = egui::ScrollArea::both()
        .id_salt("main_sheet_table_scroll_area")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let mut table_builder = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Min))
                .min_scrolled_height(0.0);

            let prefix_count = ancestor_key_columns.len();
            let num_visible_cols = visible_columns.len();
            let total_cols = num_visible_cols + prefix_count;

            table_builder = build_table_columns(
                table_builder,
                state,
                metadata,
                visible_columns,
                prefix_count,
                total_cols,
            );

            // Handle scroll-to-row request
            if let Some(row_idx) = state.scroll_to_row_index {
                if total_cols > 0 {
                    table_builder = table_builder.scroll_to_row(row_idx, Some(egui::Align::TOP));
                }
                state.scroll_to_row_index = None;
            }

            // Reset header anchor each frame before header render
            state.last_header_right_edge_x = 0.0;

            table_builder
                .header(row_height, |mut header_row| {
                    render_table_header(
                        &mut header_row,
                        ctx,
                        state,
                        registry,
                        metadata,
                        selected_name,
                        current_category,
                        ancestor_key_columns,
                        total_cols,
                        reorder_column_writer,
                        column_include_writer,
                        batch_include_writer,
                        send_schema_writer,
                        structure_send_writer,
                    );
                })
                .body(|body: TableBody| {
                    render_table_body(
                        body,
                        state,
                        registry,
                        render_cache,
                        metadata,
                        selected_name,
                        current_category,
                        visible_columns,
                        ancestor_key_columns,
                        row_height,
                        prefix_count,
                        num_visible_cols,
                        cell_update_writer,
                        toggle_add_rows_writer,
                        copy_writer,
                        paste_writer,
                        clipboard_buffer,
                    );
                });
        });

    table_start_pos
}

/// Renders the table header row
#[allow(clippy::too_many_arguments)]
fn render_table_header(
    header_row: &mut egui_extras::TableRow,
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    metadata: &crate::sheets::definitions::SheetMetadata,
    selected_name: &str,
    current_category: &Option<String>,
    ancestor_key_columns: &[(String, String)],
    total_cols: usize,
    reorder_column_writer: EventWriter<RequestReorderColumn>,
    column_include_writer: EventWriter<RequestUpdateColumnAiInclude>,
    batch_include_writer: EventWriter<RequestBatchUpdateColumnAiInclude>,
    send_schema_writer: EventWriter<RequestUpdateAiSendSchema>,
    structure_send_writer: EventWriter<RequestUpdateAiStructureSend>,
) {
    // Left control header cell: keep minimal content and do NOT draw a separator
    header_row.col(|_ui| {
        // Intentionally empty: no line under the left control column header
    });

    // Render ancestor key headers (green, read-only)
    for (key_header, value) in ancestor_key_columns {
        header_row.col(|ui| {
            let r = ui.colored_label(egui::Color32::from_rgb(0, 170, 0), key_header);
            if !value.is_empty() {
                r.on_hover_text(format!("Key value: {}", value));
            } else {
                r.on_hover_text(format!("Key column: {}", key_header));
            }
            // bottom separator under each header cell
            let rect = ui.max_rect();
            let y = rect.bottom();
            ui.painter()
                .hline(rect.x_range(), y, ui.visuals().widgets.noninteractive.bg_stroke);
        });
    }

    // Render regular headers using existing helper
    sheet_table_header(
        header_row,
        ctx,
        metadata,
        selected_name,
        current_category,
        registry,
        state,
        reorder_column_writer,
        column_include_writer,
        batch_include_writer,
        send_schema_writer,
        structure_send_writer,
    );

    // Add empty header cell for the filler remainder column appended to the builder
    if total_cols > 0 {
        header_row.col(|_ui| {
            /* filler */
        });
    }
}

/// Renders the table body with all rows
#[allow(clippy::too_many_arguments)]
fn render_table_body(
    body: TableBody,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    metadata: &crate::sheets::definitions::SheetMetadata,
    selected_name: &str,
    current_category: &Option<String>,
    visible_columns: &[usize],
    ancestor_key_columns: &[(String, String)],
    row_height: f32,
    prefix_count: usize,
    num_visible_cols: usize,
    cell_update_writer: EventWriter<UpdateCellEvent>,
    toggle_add_rows_writer: EventWriter<RequestToggleAiRowGeneration>,
    copy_writer: EventWriter<RequestCopyCell>,
    paste_writer: EventWriter<RequestPasteCell>,
    clipboard_buffer: &ClipboardBuffer,
) {
    state.ensure_ai_included_columns_cache(registry, current_category, selected_name);

    let sheet_ref = registry
        .get_sheet(current_category, selected_name)
        .unwrap();
    let grid = &sheet_ref.grid;

    let filtered_indices =
        get_filtered_row_indices_cached(state, current_category, selected_name, grid, metadata);

    // If there are absolutely no columns, show a friendly hint row
    if num_visible_cols == 0 && prefix_count == 0 {
        body.rows(row_height, 1, |mut row| {
            // control col (empty hover area to keep row height consistent)
            row.col(|ui| {
                ui.allocate_exact_size(egui::vec2(18.0, row_height), egui::Sense::hover());
            });
            // message
            row.col(|ui| {
                ui.label("(No columns)");
            });
        });
        return;
    }

    if ancestor_key_columns.is_empty() {
        render_standard_body(
            body,
            state,
            registry,
            render_cache,
            metadata,
            selected_name,
            current_category,
            visible_columns,
            &filtered_indices,
            row_height,
            cell_update_writer,
            toggle_add_rows_writer,
            copy_writer,
            paste_writer,
            clipboard_buffer,
        );
    }
}

/// Renders standard table body without virtual structure keys
#[allow(clippy::too_many_arguments)]
fn render_standard_body(
    body: TableBody,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    metadata: &crate::sheets::definitions::SheetMetadata,
    selected_name: &str,
    current_category: &Option<String>,
    visible_columns: &[usize],
    filtered_indices: &[usize],
    row_height: f32,
    mut cell_update_writer: EventWriter<UpdateCellEvent>,
    mut toggle_add_rows_writer: EventWriter<RequestToggleAiRowGeneration>,
    mut copy_writer: EventWriter<RequestCopyCell>,
    mut paste_writer: EventWriter<RequestPasteCell>,
    clipboard_buffer: &ClipboardBuffer,
) {
    use crate::sheets::definitions::ColumnValidator;

    let sheet_ref = registry
        .get_sheet(current_category, selected_name)
        .unwrap();
    let grid = &sheet_ref.grid;
    let num_cols = metadata.columns.len();

    let validators: Vec<Option<ColumnValidator>> =
        metadata.columns.iter().map(|c| c.validator.clone()).collect();

    body.rows(row_height, filtered_indices.len(), |mut row| {
        let idx_in_list = row.index();
        let original_row_index = *filtered_indices.get(idx_in_list).unwrap_or(&0);

        // Left control cell
        render_control_cell(&mut row, state, original_row_index, row_height);

        if let Some(row_data) = grid.get(original_row_index) {
            if row_data.len() != num_cols {
                row.col(|ui| {
                    ui.colored_label(egui::Color32::RED, "Row Len Err");
                });
                return;
            }

            for c_idx in visible_columns.iter().copied() {
                row.col(|ui| {
                    render_data_cell(
                        ui,
                        state,
                        registry,
                        render_cache,
                        current_category,
                        selected_name,
                        original_row_index,
                        c_idx,
                        &validators,
                        &mut cell_update_writer,
                        &mut toggle_add_rows_writer,
                        &mut copy_writer,
                        &mut paste_writer,
                        clipboard_buffer,
                    );
                });
            }

            // filler remainder cell to avoid stretching
            row.col(|ui| {
                ui.allocate_exact_size(egui::vec2(0.0, row_height), egui::Sense::hover());
            });
        } else {
            row.col(|ui| {
                ui.colored_label(egui::Color32::RED, "Row Idx Err");
            });
        }
    });
}
