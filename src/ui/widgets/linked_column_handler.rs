// src/ui/widgets/linked_column_handler.rs
use bevy::prelude::*;
use bevy_egui::egui::containers::popup::PopupCloseBehavior;
use bevy_egui::egui::{self, Id};
use std::collections::HashSet;

use crate::sheets::resources::SheetRegistry;
// Removed state import as it's no longer needed for cache mutation here
// use crate::ui::elements::editor::state::EditorWindowState;

// Removed sibling cache import as cache lookup happens outside now
// use super::linked_column_cache::{self, CacheResult};
// Import the specific function needed from the visualization module
use super::linked_column_visualization::add_linked_text_edit;
use crate::ui::validation::normalize_for_link_cmp;

/// Main handler function for the linked column editor widget.
/// Orchestrates TextEdit drawing, popup management, validation, and value commitment.
/// Assumes allowed_values have been fetched previously.
/// Returns (optional_new_value, text_edit_response)
pub fn handle_linked_column_edit(
    ui: &mut egui::Ui,
    id: egui::Id,        // Base ID for the cell
    current_value: &str, // <-- Changed: Accept &str
    _target_sheet_name: &str,
    _target_column_index: usize,
    _registry: &SheetRegistry, // Mark as unused with underscore
    // state: &mut EditorWindowState, // Removed state
    allowed_values: &HashSet<String>, // Added allowed_values reference
) -> (Option<String>, egui::Response) {
    let mut final_new_value: Option<String> = None;
    // --- REMOVED: original_string = current_value.to_string(); ---
    let text_edit_id = id.with("ac_text_edit");
    let popup_id = id.with("ac_popup");

    // --- 1. Get Allowed Values ---
    // Allowed values are now passed directly into the function.
    // Visual error state (red background) is handled outside this function in edit_cell_widget
    // based on whether the current_value is in the allowed_values passed to it.
    let link_error: Option<String> = None; // Assume link itself is okay if we got valid allowed_values

    // --- 2. Manage Input Text (using egui temporary memory) ---
    // To avoid "ghosting" when rows are inserted/deleted (IDs shift),
    // if the widget is NOT focused and the cached text differs from the
    // actual cell value for this frame, reset the cache to the true value.
    let has_focus = ui.memory(|mem| mem.has_focus(text_edit_id));
    let needs_reset = ui.memory(|mem| {
        mem.data
            .get_temp::<String>(text_edit_id)
            .map(|s| s != current_value)
            .unwrap_or(false)
    });
    if !has_focus && needs_reset {
        ui.memory_mut(|mem| {
            mem.data
                .insert_temp(text_edit_id, current_value.to_string())
        });
    }

    let mut input_text = ui.memory_mut(|mem| {
        mem.data
            // Initialize with current_value (&str) if no cache exists (first render)
            .get_temp_mut_or_insert_with::<String>(text_edit_id, || -> String {
                current_value.to_string()
            })
            .clone() // Clone the String buffer from memory for local use
    });

    // --- 3. Draw the TextEdit and Handle Focus for Popup ---
    let text_edit_response = add_linked_text_edit(
        ui,
        id,
        &mut input_text, // Pass mutable String buffer
        &link_error,
        current_value, // Pass the original &str for hover text
    );

    // Update temporary memory immediately on change
    if text_edit_response.changed() {
        ui.memory_mut(|mem| mem.data.insert_temp(text_edit_id, input_text.clone()));
    }

    if text_edit_response.gained_focus() && link_error.is_none() {
        debug!("TextEdit gained focus, opening popup.");
        ui.memory_mut(|mem| mem.open_popup(popup_id));
    }

    // --- 4. Show Popup and Handle Selection ---
    let mut clicked_suggestion_in_popup: Option<String> = None;
    egui::containers::popup::popup_below_widget(
        ui,
        popup_id,
        &text_edit_response,
        PopupCloseBehavior::CloseOnClickOutside, // Keep default close behavior
        |popup_ui| {
            popup_ui.set_min_width(text_edit_response.rect.width().max(150.0));
            let frame = egui::Frame::popup(popup_ui.style());
            frame.show(popup_ui, |frame_ui| {
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .auto_shrink([false; 2])
                    .show(frame_ui, |scroll_ui| {
                        scroll_ui.vertical(|list_ui| {
                            // Read current input text directly from memory for filtering
                            let current_input = list_ui.memory(|mem| {
                                mem.data
                                    .get_temp(text_edit_id)
                                    // --- MODIFIED: Fallback clone from &str ---
                                    .unwrap_or_else(|| current_value.to_string())
                            });
                            let input_norm = normalize_for_link_cmp(&current_input);

                            // Filter & collect suggestions based on current input, then sort deterministically (case-insensitive)
                            let mut suggestions: Vec<&String> = allowed_values
                                .iter()
                                .filter(|v| normalize_for_link_cmp(v).contains(&input_norm))
                                .collect();
                            suggestions.sort_unstable_by(|a, b| {
                                normalize_for_link_cmp(a).cmp(&normalize_for_link_cmp(b))
                            });
                            // Limit number displayed after sorting for stability across frames
                            let mut any_suggestions = false;
                            for suggestion in suggestions.into_iter().take(50) {
                                // raise cap a bit; UI scrolls anyway
                                any_suggestions = true;
                                // Highlight if current input exactly matches suggestion (normalized)
                                let is_selected = normalize_for_link_cmp(&current_input)
                                    == normalize_for_link_cmp(suggestion);
                                let response = list_ui.selectable_label(is_selected, suggestion);

                                // Handle click on suggestion
                                if response.clicked() {
                                    debug!("Suggestion Clicked: '{}'.", suggestion);
                                    clicked_suggestion_in_popup = Some(suggestion.clone());
                                    // Update temp memory immediately on click
                                    list_ui.memory_mut(|mem| {
                                        mem.data.insert_temp(text_edit_id, suggestion.clone())
                                    });
                                    // Close popup on selection
                                    list_ui.memory_mut(|mem| mem.close_popup());
                                }
                            }

                            // Show message if no suggestions match
                            if !any_suggestions {
                                list_ui.label(if allowed_values.is_empty() {
                                    "(No options available)" // Link might be valid but target column is empty
                                } else {
                                    "(No matching options)"
                                });
                            }
                        });
                    });
            });
        },
    );

    // --- 5. Determine Committed Value ---
    let mut committed_value: Option<String> = None;

    if let Some(clicked) = clicked_suggestion_in_popup {
        // Commit value if a suggestion was clicked
        debug!("Commit via popup click. Value: '{}'", clicked);
        committed_value = Some(clicked.clone()); // Clone the clicked suggestion
                                                 // Ensure local input_text variable matches clicked value for consistency this frame
                                                 // input_text = clicked.clone(); // Redundant: temp memory already updated
    } else if text_edit_response.lost_focus() {
        // Commit value when focus is lost
        // --- MODIFIED: Fallback clone from &str ---
        let buffer = ui.memory(|mem| {
            mem.data
                .get_temp(text_edit_id)
                .unwrap_or_else(|| current_value.to_string())
        });
        debug!("Commit on LostFocus: '{}'", buffer);
        committed_value = Some(buffer); // Clone buffer from memory
    } else if text_edit_response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        // Commit value on Enter key press while focused
        // --- MODIFIED: Fallback clone from &str ---
        let buffer = ui.memory(|mem| {
            mem.data
                .get_temp(text_edit_id)
                .unwrap_or_else(|| current_value.to_string())
        });
        debug!("Commit on Enter: '{}'", buffer);
        committed_value = Some(buffer); // Clone buffer from memory
                                        // Defocus and close popup on Enter commit
        ui.memory_mut(|mem| mem.request_focus(Id::NULL));
        ui.memory_mut(|mem| mem.close_popup());
    }

    // --- 6. Validate and Determine Final Output ---
    // Validation (checking if committed_value is in allowed_values) now happens
    // outside this function in `edit_cell_widget` to determine background color.
    // Here, we just check if the committed value is different from the original.
    if let Some(ref val) = committed_value {
        // --- MODIFIED: Compare with current_value (&str) ---
        if val != current_value {
            debug!("Change detected: '{}' -> '{}'", current_value, val);
            final_new_value = Some(val.clone()); // Clone the final committed value
        } else {
            debug!("No change: '{}' matches original.", val);
        }
    }
    // else: No commit event occurred this frame.

    // --- 7. Show Validation Error (Visual Only) ---
    // Visual validation (background color) handled by edit_cell_widget.

    // --- 8. Return the result ---
    (final_new_value, text_edit_response)
}
