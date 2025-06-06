// src/ui/widgets/linked_column_handler.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Id};
use bevy_egui::egui::containers::popup::PopupCloseBehavior;
use std::collections::HashSet;

use crate::sheets::resources::SheetRegistry;
// Removed state import as it's no longer needed for cache mutation here
// use crate::ui::elements::editor::state::EditorWindowState;

// Removed sibling cache import as cache lookup happens outside now
// use super::linked_column_cache::{self, CacheResult};
// Import the specific function needed from the visualization module
use super::linked_column_visualization::add_linked_text_edit;

/// Main handler function for the linked column editor widget.
/// Orchestrates TextEdit drawing, popup management, validation, and value commitment.
/// Assumes allowed_values have been fetched previously.
pub fn handle_linked_column_edit(
    ui: &mut egui::Ui,
    id: egui::Id, // Base ID for the cell
    current_value: &str, // <-- Changed: Accept &str
    target_sheet_name: &str,
    target_column_index: usize,
    _registry: &SheetRegistry, // Mark as unused with underscore
    // state: &mut EditorWindowState, // Removed state
    allowed_values: &HashSet<String>, // Added allowed_values reference
) -> Option<String> {
    trace!(
        "handle_linked_column_edit START. Original value: '{}', Target: '{}[{}]'",
        current_value, target_sheet_name, target_column_index
    );

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
    let mut input_text = ui.memory_mut(|mem| {
        mem.data
            // --- MODIFIED: Initialize with current_value (&str) ---
            .get_temp_mut_or_insert_with::<String>(text_edit_id, || -> String { current_value.to_string() }) // Clone only once for initialization if needed
            .clone() // Clone the String buffer from memory for local use
    });
    trace!("Input text from memory: '{}'", input_text);

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
        trace!("TextEdit changed, updating temporary memory with: '{}'", input_text);
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
                            let input_lower = current_input.to_lowercase();

                            // Filter suggestions based on current input
                            let suggestions = allowed_values
                                .iter()
                                .filter(|v| v.to_lowercase().contains(&input_lower))
                                .take(20); // Limit suggestions shown

                            let mut any_suggestions = false;
                            for suggestion in suggestions {
                                any_suggestions = true;
                                // Highlight if current input exactly matches suggestion
                                let is_selected = current_input == *suggestion;
                                let response = list_ui.selectable_label(is_selected, suggestion);

                                // Handle click on suggestion
                                if response.clicked() {
                                    debug!("Suggestion Clicked: '{}'.", suggestion);
                                    clicked_suggestion_in_popup = Some(suggestion.clone());
                                    // Update temp memory immediately on click
                                    list_ui.memory_mut(|mem| mem.data.insert_temp(text_edit_id, suggestion.clone()));
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
        let buffer = ui.memory(|mem| mem.data.get_temp(text_edit_id).unwrap_or_else(|| current_value.to_string()));
        debug!("Commit on LostFocus: '{}'", buffer);
        committed_value = Some(buffer); // Clone buffer from memory
    } else if text_edit_response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
         // Commit value on Enter key press while focused
         // --- MODIFIED: Fallback clone from &str ---
         let buffer = ui.memory(|mem| mem.data.get_temp(text_edit_id).unwrap_or_else(|| current_value.to_string()));
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
    trace!("handle_linked_column_edit END. Returning: {:?}", final_new_value);
    final_new_value
}