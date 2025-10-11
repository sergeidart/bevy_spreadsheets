// src/ui/elements/editor/display/display_controls.rs
// Floating button controls for Add Row and Add Column

use crate::sheets::events::{AddSheetRowRequest, RequestAddColumn};
use crate::ui::elements::editor::state::{EditorWindowState, SheetInteractionState};
use bevy::prelude::*;
use bevy_egui::egui;

/// Renders floating Add Row and Add Column buttons
pub fn render_floating_controls(
    ctx: &egui::Context,
    state: &EditorWindowState,
    table_start_pos: egui::Pos2,
    header_height: f32,
    num_visible_cols: usize,
    mut add_row_writer: EventWriter<AddSheetRowRequest>,
    mut add_column_writer: EventWriter<RequestAddColumn>,
) {
    let any_toolbox_open =
        state.current_interaction_mode != SheetInteractionState::Idle || state.show_toybox_menu;
    let add_controls_visible = !any_toolbox_open;

    if !add_controls_visible {
        return;
    }

    // Add Row: place on the delimiter (bottom of header), near left edge of the table
    let pos_left = egui::pos2(
        table_start_pos.x + 6.0,
        table_start_pos.y + header_height - 9.0,
    );
    
    egui::Area::new("floating_add_row_btn".into())
        .order(egui::Order::Foreground)
        .fixed_pos(pos_left)
        .show(ctx, |ui_f| {
            let btn = egui::Button::new("+");
            if ui_f
                .add(btn)
                .on_hover_text("Add a new row to the sheet")
                .clicked()
            {
                if let (Some(cat), Some(sheet)) = (
                    &state.selected_category,
                    &state.selected_sheet_name,
                ) {
                    add_row_writer.write(AddSheetRowRequest {
                        category: Some(cat.clone()),
                        sheet_name: sheet.clone(),
                        initial_values: None,
                    });
                }
            }
        });

    // Add Column placement
    let right_x = if num_visible_cols == 0 {
        // No columns: keep button near the left header zone
        table_start_pos.x + 24.0
    } else if state.last_header_right_edge_x.is_finite() && state.last_header_right_edge_x > 0.0 {
        // Place just to the right of the last actual header
        state.last_header_right_edge_x + 6.0
    } else {
        // As a conservative fallback, keep it near the left header zone
        table_start_pos.x + 24.0
    };

    let pos_right = egui::pos2(right_x, table_start_pos.y + 2.0);
    
    egui::Area::new("floating_add_col_btn".into())
        .order(egui::Order::Foreground)
        .fixed_pos(pos_right)
        .show(ctx, |ui_f| {
            let btn = egui::Button::new("+");
            if ui_f
                .add(btn)
                .on_hover_text("Add a new column to the sheet")
                .clicked()
            {
                if let (Some(cat), Some(sheet)) = (
                    &state.selected_category,
                    &state.selected_sheet_name,
                ) {
                    add_column_writer.write(RequestAddColumn {
                        category: Some(cat.clone()),
                        sheet_name: sheet.clone(),
                    });
                }
            }
        });
}
