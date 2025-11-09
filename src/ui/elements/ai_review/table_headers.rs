use crate::sheets::definitions::SheetMetadata;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai_review::display_context::ReviewDisplayContext;
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use bevy_egui::egui::{self, RichText};

/// Renders table header columns for the AI batch review panel
pub(crate) fn render_table_headers(
    header_row: &mut egui_extras::TableRow,
    display_ctx: &ReviewDisplayContext,
    registry: &SheetRegistry,
    selected_category_clone: &Option<String>,
) {
    // First column: Action/Status header
    header_row.col(|ui| {
        ui.label(RichText::new("Action").strong());
        draw_header_underline(ui);
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
            draw_header_underline(ui);
        });
    }

    // Regular and structure columns
    let sheet_metadata = registry
        .get_sheet(selected_category_clone, &display_ctx.active_sheet_name)
        .and_then(|sheet| sheet.metadata.as_ref());

    for col_entry in &display_ctx.merged_columns {
        header_row.col(|ui| {
            let header_text = get_header_text(
                col_entry,
                display_ctx,
                sheet_metadata,
            );

            // Color parent_key header green to indicate non-interactable key column
            if header_text.eq_ignore_ascii_case("parent_key") {
                ui.label(
                    RichText::new(header_text)
                        .color(egui::Color32::from_rgb(0, 170, 0))
                        .strong(),
                );
            } else {
                ui.label(RichText::new(header_text).strong());
            }
            draw_header_underline(ui);
        });
    }
}

/// Gets the header text for a column entry based on mode and metadata
fn get_header_text<'a>(
    col_entry: &ColumnEntry,
    display_ctx: &'a ReviewDisplayContext,
    sheet_metadata: Option<&'a SheetMetadata>,
) -> &'a str {
    match col_entry {
        ColumnEntry::Regular(col_idx) => {
            if display_ctx.in_structure_mode {
                // In structure mode, use structure schema
                display_ctx
                    .structure_schema
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
                display_ctx
                    .structure_schema
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
    }
}

/// Draws the horizontal line under header cells
fn draw_header_underline(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let y = rect.bottom();
    ui.painter().hline(
        rect.x_range(),
        y,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}
