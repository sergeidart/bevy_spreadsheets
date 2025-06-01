// src/ui/elements/editor/ai_review_ui.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, RichText, TextStyle, Align};
use egui_extras::{TableBuilder, Column}; // Import TableBuilder and Column

use crate::sheets::{events::UpdateCellEvent, resources::SheetRegistry};
use super::state::{EditorWindowState, ReviewChoice};
use super::ai_helpers::{advance_review_queue, exit_review_mode};

pub(super) fn draw_inline_ai_review_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    selected_sheet_name_clone: &Option<String>,
    registry: &SheetRegistry,
    cell_update_writer: &mut EventWriter<UpdateCellEvent>,
) {
    let sheet_name_str = match selected_sheet_name_clone {
        Some(s) => s.as_str(),
        None => {
            exit_review_mode(state);
            return;
        }
    };

    let original_row_index = match state.current_ai_suggestion_edit_buffer {
        Some((idx, _)) => idx,
        None => {
            warn!("Inline review panel: edit buffer is empty. Attempting to advance/exit.");
            advance_review_queue(state);
            return;
        }
    };

    let original_data_cloned: Option<Vec<String>> = registry
        .get_sheet(selected_category_clone, sheet_name_str)
        .and_then(|d| d.grid.get(original_row_index))
        .cloned();

    let metadata_opt = registry
        .get_sheet(selected_category_clone, sheet_name_str)
        .and_then(|d| d.metadata.as_ref());

    // --- Wrap the entire panel content in a Horizontal ScrollArea ---
    egui::ScrollArea::horizontal() // Changed to horizontal
        .id_salt("ai_review_panel_scroll_area") // Unique ID for the scroll area
        .auto_shrink([false, true]) // Don't shrink width, shrink height to content
        .show(ui, |ui| {
            // --- Main Panel Frame ---
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(5))
                .show(ui, |ui| {
                    ui.label(RichText::new(format!("Reviewing AI Suggestion for Original Row Index: {}", original_row_index)).heading()); // User-facing index
                    ui.separator();

                    if state.current_ai_suggestion_edit_buffer.is_none() || state.current_ai_suggestion_edit_buffer.as_ref().map_or(true, |(idx, _)| *idx != original_row_index) {
                        ui.colored_label(Color32::YELLOW, "Review item changed, refreshing...");
                        return;
                    }
                    
                    // Get mutable access to the suggestion being edited
                    let (_, current_suggestion_mut) = state.current_ai_suggestion_edit_buffer.as_mut().unwrap();

                    match (original_data_cloned.as_ref(), metadata_opt) {
                        (Some(original_row), Some(metadata)) => {
                            let num_cols = metadata.columns.len().max(current_suggestion_mut.len());

                            // Ensure choice and suggestion buffers are correctly sized
                            if state.ai_review_column_choices.len() != num_cols {
                                 state.ai_review_column_choices = vec![ReviewChoice::AI; num_cols];
                            }
                            if current_suggestion_mut.len() != num_cols {
                                 current_suggestion_mut.resize(num_cols, String::new());
                            }

                            let text_style = TextStyle::Body;
                            let row_height = ui.text_style_height(&text_style);

                            // --- Use TableBuilder for the review grid ---
                            // The TableBuilder itself will handle scrolling for its content if it exceeds the space given to it.
                            let table = TableBuilder::new(ui)
                                .striped(true)
                                .resizable(true)
                                .cell_layout(egui::Layout::left_to_right(Align::Center))
                                .columns(Column::auto().at_least(80.0), num_cols) // Auto-sized columns
                                .min_scrolled_height(0.0); // Let the outer ScrollArea manage height primarily

                            table
                                .header(20.0, |mut header| {
                                    for c_idx in 0..num_cols {
                                        header.col(|ui| {
                                            let col_header = metadata.columns.get(c_idx)
                                                .map_or_else(|| format!("Col {}", c_idx + 1), |c| c.header.clone());
                                            ui.strong(col_header);
                                        });
                                    }
                                })
                                .body(|mut body| {
                                    // --- Original Row ---
                                    body.row(row_height, |mut row| {
                                        for c_idx in 0..num_cols {
                                            row.col(|ui| {
                                                let original_value = original_row.get(c_idx).cloned().unwrap_or_default();
                                                let current_choice = state.ai_review_column_choices[c_idx];
                                                let is_different = original_value != current_suggestion_mut.get(c_idx).cloned().unwrap_or_default();
                                                
                                                let display_text = if is_different && current_choice == ReviewChoice::AI {
                                                    RichText::new(&original_value).strikethrough()
                                                } else {
                                                    RichText::new(&original_value)
                                                };
                                                ui.label(display_text).on_hover_text("Original Value");
                                            });
                                        }
                                    });

                                    // --- AI Suggestion Row (Editable) ---
                                    body.row(row_height, |mut row| {
                                        for c_idx in 0..num_cols {
                                            row.col(|ui| {
                                                let original_value = original_row.get(c_idx).cloned().unwrap_or_default();
                                                let ai_value_mut = current_suggestion_mut.get_mut(c_idx).expect("Suggestion vec exists");
                                                let is_different = original_value != *ai_value_mut;

                                                ui.add(
                                                    egui::TextEdit::singleline(ai_value_mut)
                                                        .desired_width(f32::INFINITY) // Fill cell
                                                        .text_color_opt(if is_different { Some(Color32::LIGHT_YELLOW) } else { None })
                                                );
                                            });
                                        }
                                    });

                                    // --- Choice Row (Checkboxes) ---
                                    body.row(row_height, |mut row| {
                                        for c_idx in 0..num_cols {
                                            row.col(|ui| {
                                                ui.horizontal_centered(|ui| {
                                                    let mut choice = state.ai_review_column_choices[c_idx];
                                                    if ui.radio_value(&mut choice, ReviewChoice::Original, "Original").clicked() {
                                                        state.ai_review_column_choices[c_idx] = ReviewChoice::Original;
                                                    }
                                                    if ui.radio_value(&mut choice, ReviewChoice::AI, "AI").clicked() {
                                                        state.ai_review_column_choices[c_idx] = ReviewChoice::AI;
                                                    }
                                                });
                                            });
                                        }
                                    });
                                }); // End Table Body

                            // --- Action Buttons ---
                            ui.add_space(10.0);
                            let mut apply_action = false;
                            let mut skip_action = false;
                            let mut cancel_action = false;

                            ui.horizontal(|ui| {
                                if ui.button("✅ Apply Chosen Changes").clicked() {
                                    apply_action = true;
                                }
                                if ui.button("⏩ Skip This Row").clicked() {
                                    skip_action = true;
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("❌ Cancel Review Mode").clicked() {
                                        cancel_action = true;
                                    }
                                });
                            });

                            // --- Defer actions to avoid borrow issues ---
                            if apply_action {
                                for (c_idx, choice) in state.ai_review_column_choices.iter().enumerate() {
                                    let original_cell_value = original_row.get(c_idx).cloned().unwrap_or_default();
                                    // Important: Re-fetch current_suggestion_mut here as it might have been edited
                                    let ai_cell_value = state.current_ai_suggestion_edit_buffer.as_ref()
                                        .and_then(|(_, buf)| buf.get(c_idx))
                                        .cloned()
                                        .unwrap_or_default();

                                    let value_to_apply = match choice {
                                        ReviewChoice::Original => &original_cell_value,
                                        ReviewChoice::AI => &ai_cell_value,
                                    };
                                    
                                    let current_grid_value = registry.get_sheet(selected_category_clone, sheet_name_str)
                                        .and_then(|s| s.grid.get(original_row_index))
                                        .and_then(|r| r.get(c_idx))
                                        .cloned()
                                        .unwrap_or_default();

                                    if *value_to_apply != current_grid_value {
                                         info!("Applying change for row {}, col {}: '{}' from choice {:?}", original_row_index, c_idx, value_to_apply, choice);
                                         cell_update_writer.write(UpdateCellEvent {
                                             category: selected_category_clone.clone(),
                                             sheet_name: sheet_name_str.to_string(),
                                             row_index: original_row_index,
                                             col_index: c_idx,
                                             new_value: value_to_apply.clone(),
                                         });
                                     }
                                 }
                                advance_review_queue(state);
                            } else if skip_action {
                                advance_review_queue(state);
                            } else if cancel_action {
                                exit_review_mode(state);
                            }

                        } // End match (Some(original_row), Some(metadata))
                        (_, None) => {
                            ui.colored_label(Color32::RED, "Error: Missing metadata for sheet.");
                            if ui.button("Cancel Review Mode").clicked() { exit_review_mode(state); }
                        }
                        (None, _) => {
                            ui.colored_label(Color32::RED, "Error: Original row data not found (was it deleted?).");
                             ui.horizontal(|ui| {
                                if ui.button("Skip Problematic Row").clicked() { advance_review_queue(state); }
                                if ui.button("Cancel Review Mode").clicked() { exit_review_mode(state); }
                             });
                        }
                    } // End match
                }); // End Frame::group
        }); // End ScrollArea
}
