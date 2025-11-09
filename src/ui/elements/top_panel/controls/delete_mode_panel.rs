// src/ui/elements/top_panel/controls/delete_mode_panel.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::{RequestDeleteColumns, RequestDeleteRows};
use crate::ui::elements::editor::state::EditorWindowState;

// MODIFIED: Helper struct generic over borrow lifetime 'a, and EventWriter world lifetime 'w
pub(crate) struct DeleteModeEventWriters<'a, 'w> {
    pub delete_rows_event_writer: &'a mut EventWriter<'w, RequestDeleteRows>,
    pub delete_columns_event_writer: &'a mut EventWriter<'w, RequestDeleteColumns>,
}

// MODIFIED: Function generic over 'a and 'w. Make it `pub` to be callable from main_editor.
pub fn show_delete_mode_active_controls<'a, 'w>(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    event_writers: DeleteModeEventWriters<'a, 'w>,
) {
    ui.horizontal_wrapped(|ui| {
        // Indent under the Delete toggle position for unified second-row layout
        if state.last_edit_mode_button_min_x > 0.0 {
            let panel_left = ui.max_rect().min.x;
            let indent = (state.last_edit_mode_button_min_x - panel_left).max(0.0);
            ui.add_space(indent);
        }
        // No description or separators per new design

        let is_sheet_selected = state.selected_sheet_name.is_some();
        let rows_selected_count = state.ai_selected_rows.len();
        let cols_selected_count = state.selected_columns_for_deletion.len();
        let can_delete_anything =
            is_sheet_selected && (rows_selected_count > 0 || cols_selected_count > 0);

        let mut button_text = "Delete Selected".to_string();
        if rows_selected_count > 0 && cols_selected_count > 0 {
            button_text = format!(
                "Delete ({} Rows, {} Cols)",
                rows_selected_count, cols_selected_count
            );
        } else if rows_selected_count > 0 {
            button_text = format!("Delete {} Row(s)", rows_selected_count);
        } else if cols_selected_count > 0 {
            button_text = format!("Delete {} Col(s)", cols_selected_count);
        }

        if ui
            .add_enabled(can_delete_anything, egui::Button::new(button_text))
            .clicked()
        {
            // Use selected sheet directly (virtual structures deprecated)
            let effective_sheet_name = state.selected_sheet_name.as_ref().unwrap();
            if state.selected_sheet_name.is_some() {
                if rows_selected_count > 0 {
                    event_writers
                        .delete_rows_event_writer
                        .write(RequestDeleteRows {
                            category: state.selected_category.clone(),
                            sheet_name: effective_sheet_name.clone(),
                            row_indices: state.ai_selected_rows.clone(),
                        });
                }
                if cols_selected_count > 0 {
                    event_writers
                        .delete_columns_event_writer
                        .write(RequestDeleteColumns {
                            category: state.selected_category.clone(),
                            sheet_name: effective_sheet_name.clone(),
                            column_indices: state.selected_columns_for_deletion.clone(),
                        });
                }

                // Always exit Delete Mode after a delete action is performed
                state.reset_interaction_modes_and_selections();
                state.show_edit_mode_panel = false;
                state.force_filter_recalculation = true;
            }
        }
    });
}
