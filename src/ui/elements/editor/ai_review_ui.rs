// src/ui/elements/editor/ai_review_ui.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::{events::UpdateCellEvent, resources::SheetRegistry};
use super::state::{AiModeState, EditorWindowState};
// Import helpers from the new ai_helpers module
use super::ai_helpers::{advance_review_queue, exit_review_mode};

/// Shows the UI for reviewing AI suggestions one by one.
pub(super) fn show_ai_review_ui(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>, // Pass as immutable ref
    selected_sheet_name_clone: &Option<String>, // Pass as immutable ref
    registry: &SheetRegistry, // Pass as immutable ref
    cell_update_writer: &mut EventWriter<UpdateCellEvent>, // Pass mutable ref
) {
    // Flags to defer state changes
    let mut action_accept = false;
    let mut action_reject = false;
    let mut action_skip = false;
    let mut action_cancel = false;
    let mut row_idx_for_action: Option<usize> = None;

    // --- Draw the UI elements within the centered vertical layout ---
    ui.vertical_centered(|ui| {
        ui.heading("AI Review Mode");
        ui.separator();

        let current_queue_index_opt = state.ai_current_review_index;
        let mut maybe_original_row_idx: Option<usize> = None;
        if let Some(queue_idx) = current_queue_index_opt {
            maybe_original_row_idx = state.ai_review_queue.get(queue_idx).cloned();
        }

        // Store the row index if valid, needed for deferred actions
        row_idx_for_action = maybe_original_row_idx;

        if let Some(original_row_idx) = maybe_original_row_idx {
            ui.label(format!(
                "Reviewing suggestion for original row index: {}",
                original_row_idx
            ));

            // --- Immutable Reads for Drawing ---
            // Fetch original data and suggestion immutably
            let sheet_name = selected_sheet_name_clone.as_ref().unwrap(); // Assume valid
            let original_data_opt = registry
                .get_sheet(selected_category_clone, sheet_name)
                .and_then(|d| d.grid.get(original_row_idx));

            // Immutable borrow here
            let suggestion_opt = state.ai_suggestions.get(&original_row_idx);
            // --- End Immutable Reads ---

            match (original_data_opt, suggestion_opt) {
                (Some(original_row), Some(suggestion_row)) => {
                    // Display comparison (simple example)
                    ui.group(|ui| {
                        egui::Grid::new("ai_review_grid")
                            .num_columns(3)
                            .spacing([10.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Column");
                                ui.label("Original");
                                ui.label("Suggestion");
                                ui.end_row();

                                let num_cols =
                                    original_row.len().max(suggestion_row.len());
                                for i in 0..num_cols {
                                     let header = registry
                                        .get_sheet(selected_category_clone, sheet_name)
                                        .and_then(|d| d.metadata.as_ref())
                                        .and_then(|m| m.columns.get(i))
                                        .map_or_else(
                                            || format!("Col {}", i + 1),
                                            |c| c.header.clone(),
                                        );
                                    ui.label(&header);
                                    let original_cell =
                                        original_row.get(i).cloned().unwrap_or_default();
                                    let suggested_cell = suggestion_row
                                        .get(i)
                                        .cloned()
                                        .unwrap_or_default();

                                    // Highlight differences
                                    let original_label = egui::RichText::new(&original_cell);
                                    let suggested_label = if original_cell != suggested_cell {
                                        egui::RichText::new(&suggested_cell)
                                            .color(egui::Color32::LIGHT_YELLOW)
                                            .strong()
                                    } else {
                                         egui::RichText::new(&suggested_cell)
                                    };
                                    ui.label(original_label);
                                    ui.label(suggested_label);
                                    ui.end_row();
                                }
                            });
                    });

                    // --- Action Buttons: Set Flags ---
                    ui.horizontal(|ui| {
                        if ui.button("✅ Accept Suggestion").clicked() {
                             // --- Send update events immediately (this is fine) ---
                             for (col_idx, suggested_value) in suggestion_row.iter().enumerate() {
                                if let Some(original_value) = original_row.get(col_idx) {
                                    if original_value != suggested_value {
                                        cell_update_writer.send(UpdateCellEvent {
                                            category: selected_category_clone.clone(),
                                            sheet_name: sheet_name.clone(),
                                            row_index: original_row_idx,
                                            col_index: col_idx,
                                            new_value: suggested_value.clone(),
                                        });
                                    }
                                } else {
                                     cell_update_writer.send(UpdateCellEvent {
                                        category: selected_category_clone.clone(),
                                        sheet_name: sheet_name.clone(),
                                        row_index: original_row_idx,
                                        col_index: col_idx,
                                        new_value: suggested_value.clone(),
                                    });
                                }
                            }
                            // --- Defer state change ---
                            action_accept = true;
                        }
                        if ui.button("❌ Reject Suggestion").clicked() {
                            action_reject = true;
                        }
                        if ui.button("⏩ Skip Review").clicked() {
                            action_skip = true;
                        }
                    });
                }
                _ => {
                    ui.colored_label(
                        egui::Color32::RED,
                        "Error: Could not retrieve original data or suggestion.",
                    );
                    // Skip problematic item by setting flag
                    action_skip = true;
                }
            }
        } else if current_queue_index_opt.is_some() {
            // Queue index exists but no corresponding row index found (error state)
            ui.colored_label(egui::Color32::RED, "Error: Invalid review index!");
            action_cancel = true; // Treat as cancel to exit mode
        } else {
            // Queue index is None, review finished or queue was empty
            ui.label("Review queue processed or empty.");
             if state.ai_mode == AiModeState::Reviewing { // Only trigger exit if actually in review mode
                 action_cancel = true;
             }
        }

        ui.separator();
        if ui.button("Cancel Review").clicked() {
            action_cancel = true;
        }
    }); // --- End ui.vertical_centered ---

    // --- Deferred State Modifications (After UI Scope) ---
    if action_accept || action_reject || action_skip {
        if let Some(idx) = row_idx_for_action {
            state.ai_suggestions.remove(&idx); // Remove from suggestions map
            advance_review_queue(state);       // Mutably borrow state HERE
        } else {
             // Should not happen if flags were set correctly
             warn!("Review action triggered but row_idx_for_action was None.");
             exit_review_mode(state); // Exit defensively
        }
    } else if action_cancel {
        exit_review_mode(state); // Mutably borrow state HERE
    }
}