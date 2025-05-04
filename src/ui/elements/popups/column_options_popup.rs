// src/ui/elements/popups/column_options_popup.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::RequestUpdateColumnName;
use crate::sheets::resources::SheetRegistry; // Need registry access
use crate::ui::elements::editor::EditorWindowState; // Use state defined in editor

/// Displays the "Column Options" popup window.
/// Handles renaming and filtering for the selected column.
pub fn show_column_options_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    column_rename_writer: &mut EventWriter<RequestUpdateColumnName>,
    // Use mutable registry for direct filter update & getting current values
    registry: &mut SheetRegistry,
    // feedback_writer: &mut EventWriter<SheetOperationFeedback>, // Optional for direct feedback
) {
    let mut popup_open = state.show_column_options_popup;

    // --- Initialize popup state fields when it opens ---
    if state.show_column_options_popup && state.column_options_popup_needs_init {
        // Reset flag
        state.column_options_popup_needs_init = false;

        // Load current values from metadata
        if let Some(sheet_data) = registry.get_sheet(&state.options_column_target_sheet) {
            if let Some(meta) = &sheet_data.metadata {
                let col_index = state.options_column_target_index;
                if col_index < meta.column_headers.len() {
                    // Load current rename value
                    state.options_column_rename_input = meta.column_headers[col_index].clone();
                    // Load current filter value
                    state.options_column_filter_input = meta.column_filters
                        .get(col_index)
                        .cloned()
                        .flatten() // Handle Option<&Option<String>> -> Option<String>
                        .unwrap_or_default(); // Use empty string if None
                } else {
                    // Index out of bounds, clear inputs
                     warn!("Column index {} out of bounds during popup init.", col_index);
                     state.options_column_rename_input.clear();
                     state.options_column_filter_input.clear();
                }
            } else {
                 // Metadata missing, clear inputs
                 warn!("Metadata missing during popup init for sheet '{}'.", state.options_column_target_sheet);
                 state.options_column_rename_input.clear();
                 state.options_column_filter_input.clear();
            }
        } else {
             // Sheet missing, clear inputs
             warn!("Sheet '{}' missing during popup init.", state.options_column_target_sheet);
             state.options_column_rename_input.clear();
             state.options_column_filter_input.clear();
        }
    }


    if state.show_column_options_popup {
        let mut trigger_apply = false; // Flag to apply changes

        egui::Window::new("Column Options")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut popup_open)
            .show(ctx, |ui| {
                // Safely get current header name for display using immutable borrow first
                let header_text = registry
                    .get_sheet(&state.options_column_target_sheet)
                    .and_then(|s| s.metadata.as_ref())
                    .and_then(|m| m.column_headers.get(state.options_column_target_index))
                    .map(|s| s.as_str())
                    .unwrap_or("?"); // Fallback if sheet/metadata/column disappears

                ui.label(format!(
                    "Options for column '{}' (#{})",
                    header_text,
                    state.options_column_target_index + 1,
                ));
                ui.separator();

                // --- Rename Section ---
                ui.strong("Rename");
                ui.horizontal(|ui| {
                    ui.label("New Name:");
                    let rename_response = ui.add(
                        egui::TextEdit::singleline(&mut state.options_column_rename_input)
                            .desired_width(150.0)
                            .lock_focus(true), // Focus rename first
                    );
                    if rename_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if !state.options_column_rename_input.trim().is_empty() {
                            trigger_apply = true;
                        }
                    }
                });
                // TODO: Add validation feedback display if needed using ui_feedback Res

                ui.separator();

                // --- Filter Section ---
                ui.strong("Filter (Contains)");
                ui.horizontal(|ui| {
                    ui.label("Text:");
                    let filter_response = ui.add(
                        egui::TextEdit::singleline(&mut state.options_column_filter_input)
                            .desired_width(150.0)
                    );
                    if filter_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        trigger_apply = true; // Apply on Enter too
                    }
                    // Clear button applies immediately by setting flag
                    if ui.button("Clear").clicked() {
                        state.options_column_filter_input.clear();
                        trigger_apply = true;
                    }
                });
                ui.small("Leave empty or clear to disable filter.");


                // --- Options Section (Placeholder) ---
                // ui.separator();
                // ui.strong("Options");
                // ...

                ui.separator();
                ui.horizontal(|ui| {
                    // Only enable Apply if rename input is valid (not empty)
                    if ui.add_enabled(!state.options_column_rename_input.trim().is_empty(), egui::Button::new("Apply")).clicked() {
                        trigger_apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        state.show_column_options_popup = false; // Close immediately
                    }
                });
            }); // End window show

        // --- Apply Changes Logic ---
        if trigger_apply {
            let sheet_name = state.options_column_target_sheet.clone();
            let col_index = state.options_column_target_index;
            let mut changes_applied = false; // Track if any actual change happened

            // --- 1. Apply Rename (Send event if name actually changed) ---
            let current_name_opt = registry
                .get_sheet(&sheet_name)
                .and_then(|s| s.metadata.as_ref())
                .and_then(|m| m.column_headers.get(col_index))
                .cloned();

            let new_name_trimmed = state.options_column_rename_input.trim();
            if Some(new_name_trimmed) != current_name_opt.as_deref() && !new_name_trimmed.is_empty() {
                 // Check for duplicate names (optional but recommended)
                 let is_duplicate = registry
                     .get_sheet(&sheet_name)
                     .and_then(|s| s.metadata.as_ref())
                     .map_or(false, |m| m.column_headers.iter().enumerate()
                         .any(|(i, h)| i != col_index && h.eq_ignore_ascii_case(new_name_trimmed)));

                 if !is_duplicate {
                     info!("Sending rename request for column {} of sheet '{}'.", col_index + 1, sheet_name);
                     column_rename_writer.send(RequestUpdateColumnName {
                         sheet_name: sheet_name.clone(),
                         column_index: col_index,
                         new_name: new_name_trimmed.to_string(), // Send trimmed name
                     });
                     changes_applied = true;
                 } else {
                     warn!("Column rename aborted: Name '{}' already exists (ignoring case).", new_name_trimmed);
                     // TODO: Send feedback via UiFeedbackState
                     // feedback_writer.send(SheetOperationFeedback { message: format!("Name '{}' already exists", new_name_trimmed), is_error: true });
                 }

            } else if new_name_trimmed.is_empty() {
                 warn!("Column rename aborted: New name cannot be empty.");
                 // TODO: Send feedback
            }

            // --- 2. Apply Filter (Directly modify metadata) ---
            // Get mutable access again if needed (might be tricky with borrow checker)
             if let Some(sheet_data) = registry.get_sheet_mut(&sheet_name) {
                 if let Some(meta) = &mut sheet_data.metadata {
                     if col_index < meta.column_filters.len() {
                         let new_filter_trimmed = state.options_column_filter_input.trim();
                         let filter_to_store: Option<String> = if new_filter_trimmed.is_empty() {
                             None
                         } else {
                             Some(new_filter_trimmed.to_string())
                         };

                         // Check if filter actually changed
                         if meta.column_filters[col_index] != filter_to_store {
                             info!("Updating filter for column {} of sheet '{}' to: {:?}", col_index + 1, sheet_name, filter_to_store);
                             meta.column_filters[col_index] = filter_to_store;
                             changes_applied = true; // Mark that a change occurred
                         }
                     } else { warn!("Column index {} out of bounds for filters.", col_index); }
                 } else { warn!("Metadata missing when applying filter for sheet '{}'.", sheet_name); }
             } else { warn!("Sheet '{}' missing when applying filter.", sheet_name); }


            // --- 3. Trigger Save if Changes Applied ---
            if changes_applied {
                 // Flag save for next frame in editor UI
                 state.sheet_needs_save = true;
                 state.sheet_to_save = sheet_name; // Store which sheet to save
                 // save_event_writer.send(RequestSaveSheet { sheet_name }); // Alternative: send event
            }

            // Close popup after applying
            state.show_column_options_popup = false;
        }

        // Update state based on window interaction (closing via 'x')
        state.show_column_options_popup = popup_open;

        // Reset internal state if the popup is no longer shown
        if !state.show_column_options_popup {
            state.options_column_target_sheet.clear();
            state.options_column_target_index = 0;
            state.options_column_rename_input.clear();
            state.options_column_filter_input.clear();
            state.column_options_popup_needs_init = false; // Reset init flag too
        }
    }
}