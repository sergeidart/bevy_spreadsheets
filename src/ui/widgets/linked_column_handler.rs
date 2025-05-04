// src/ui/widgets/linked_column_handler.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Id};
use std::collections::HashSet;

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

// Import sibling modules
use super::linked_column_cache::{self, CacheResult};
use super::linked_column_visualization::{
    self, LinkedEditorVisParams, LinkedEditorVisOutput,
};

/// Main handler function for the linked column editor widget.
/// Orchestrates caching, interaction handling, validation, and visualization.
///
/// # Arguments
///
/// * `ui` - The egui UI context.
/// * `id` - A unique ID for the widget instance.
/// * `current_value` - The current persistent string value of the cell.
/// * `target_sheet_name` - The name of the sheet the link points to.
/// * `target_column_index` - The index of the column in the target sheet.
/// * `registry` - Immutable reference to the `SheetRegistry`.
/// * `state` - Mutable reference to the `EditorWindowState` (for cache).
///
/// # Returns
///
/// * `Option<String>` - The new string value if it was changed and validated, otherwise `None`.
pub fn handle_linked_column_edit(
    ui: &mut egui::Ui,
    id: egui::Id,
    current_value: &str,
    target_sheet_name: &str,
    target_column_index: usize,
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
) -> Option<String> {
    let mut final_new_value: Option<String> = None;
    let original_string = current_value.to_string();
    let text_edit_id = id.with("ac_text_edit"); // Consistent ID for temp memory

    // --- 1. Get Allowed Values (from cache or populate) ---
    let (allowed_values, link_error) = match linked_column_cache::get_or_populate_linked_options(
        target_sheet_name,
        target_column_index,
        registry,
        state,
    ) {
        CacheResult::Success(values) => (values, None),
        CacheResult::Error(err_msg) => (&HashSet::new(), Some(err_msg)), // Use empty set on error
    };

    // --- 2. Manage Input Text (using egui temporary memory) ---
    // Get or initialize the text in egui's temporary memory for the TextEdit
    let mut input_text = ui
        .memory_mut(|mem| {
            mem.data
                .get_temp_mut_or_insert_with(text_edit_id, || original_string.clone())
                .clone()
        });

    // Reset input_text to original if focus is lost *before* drawing the UI this frame,
    // but only if the temporary memory differs from the original value.
    // This prevents stale temporary memory from persisting visually if no interaction happens.
    if !ui.memory(|mem| mem.has_focus(text_edit_id)) && input_text != original_string {
        input_text = original_string.clone();
        ui.memory_mut(|mem| mem.data.insert_temp(text_edit_id, input_text.clone()));
    }

    // --- 3. Prepare Suggestions ---
    let mut filtered_suggestions: Vec<String> = Vec::new();
    let show_popup = ui.memory(|mem| mem.has_focus(text_edit_id)) && link_error.is_none();
    if show_popup {
        let input_lower = input_text.to_lowercase();
        if !input_text.is_empty() {
            filtered_suggestions = allowed_values
                .iter()
                .filter(|v| v.to_lowercase().contains(&input_lower))
                .cloned()
                .collect::<Vec<_>>();
        } else {
            // Show all allowed values if input is empty
            filtered_suggestions = allowed_values.iter().cloned().collect::<Vec<_>>();
        }
        filtered_suggestions.sort_unstable(); // Sort for consistent display
        filtered_suggestions.truncate(10); // Limit displayed suggestions
    }

    // --- 4. Render the UI ---
    let vis_params = LinkedEditorVisParams {
        ui,
        id,
        input_text: &mut input_text, // Pass mutable ref to allow modification by visualization
        original_value: &original_string,
        filtered_suggestions: &filtered_suggestions,
        show_popup,
        validation_error: None, // Validation happens after UI interaction
        link_error: link_error.clone(), // Pass link error for display
    };
    let vis_output = linked_column_visualization::show_linked_editor_ui(vis_params);

    // --- 5. Handle Interaction Results ---
    let mut committed_value: Option<String> = None;

    if let Some(clicked_suggestion) = vis_output.clicked_suggestion {
        // A suggestion was clicked, this is the committed value.
        committed_value = Some(clicked_suggestion);
        // The visual input_text was already updated by the visualization function.
    } else if let Some(response) = vis_output.text_edit_response {
        // No suggestion click, check for lost focus or Enter key press on TextEdit.
        if response.lost_focus()
            || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
        {
            // Commit the value currently in the input_text buffer.
            committed_value = Some(input_text.clone());

            // Remove focus if Enter was pressed while focused.
            if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ui.memory_mut(|mem| mem.request_focus(Id::NULL));
            }

            // Ensure the committed text persists in temporary memory after focus loss/Enter.
            ui.memory_mut(|mem| mem.data.insert_temp(text_edit_id, input_text.clone()));
        }
    }

    // --- 6. Validate and Determine Final Output ---
    let mut validation_error_msg: Option<String> = None;
    if let Some(ref committed) = committed_value {
        // Only validate non-empty committed values against the allowed set.
        // Empty string might be implicitly valid or invalid based on column definition (e.g., OptionString vs String).
        // For linked columns, we generally treat empty as valid unless specific rules are added.
        if !committed.is_empty() && !allowed_values.contains(committed) {
            validation_error_msg = Some(format!(
                "Invalid: '{}' not in target col {} of '{}'",
                committed,
                target_column_index + 1,
                target_sheet_name
            ));
        } else {
            // Value is valid (or empty), check if it differs from the original.
            if *committed != original_string {
                final_new_value = Some(committed.clone());
            }
        }
    }

    // --- 7. Re-render UI if Validation Error Occurred (Optional but good UX) ---
    // If validation failed *after* the initial render, we might want to redraw
    // the widget immediately with the error indicator. This requires re-calling
    // the visualization function with the updated validation status.
    if validation_error_msg.is_some() && link_error.is_none() {
        // Need to get input_text again as it might have been modified by the first UI pass
         let mut current_input_text = ui.memory(|mem| mem.data.get_temp(text_edit_id).unwrap_or_else(|| original_string.clone()));

        let vis_params_rerender = LinkedEditorVisParams {
            ui, // ui is already &mut
            id,
            input_text: &mut current_input_text, // Pass potentially updated text
            original_value: &original_string,
            filtered_suggestions: &filtered_suggestions, // Suggestions remain the same
            show_popup: false, // Don't show popup during re-render for error
            validation_error: validation_error_msg, // Pass the error message
            link_error: link_error, // Pass link error status again
        };
        // We don't need the output of the re-render, just the visual effect.
        linked_column_visualization::show_linked_editor_ui(vis_params_rerender);
    }

    // --- 8. Return the final validated and changed value ---
    final_new_value
}
