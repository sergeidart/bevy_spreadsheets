// src/ui/elements/popups/column_options_popup.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::RequestUpdateColumnName;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::EditorWindowState;

pub fn show_column_options_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    column_rename_writer: &mut EventWriter<RequestUpdateColumnName>,
    registry: &mut SheetRegistry,
) {
    // Only proceed if the popup should be shown according to the state
    if !state.show_column_options_popup {
        return;
    }

    // --- Initialize popup state fields when it opens ---
    if state.column_options_popup_needs_init {
        state.column_options_popup_needs_init = false; // Reset flag immediately
        if let Some(sheet_data) = registry.get_sheet(&state.options_column_target_sheet) {
            if let Some(meta) = &sheet_data.metadata {
                let col_index = state.options_column_target_index;
                if col_index < meta.column_headers.len() {
                    state.options_column_rename_input = meta.column_headers[col_index].clone();
                    state.options_column_filter_input = meta.column_filters
                        .get(col_index).cloned().flatten().unwrap_or_default();
                } else {
                    warn!("Column index {} out of bounds during popup init.", col_index);
                    state.options_column_rename_input.clear();
                    state.options_column_filter_input.clear();
                }
            } else {
                 warn!("Metadata missing during popup init for sheet '{}'.", state.options_column_target_sheet);
                 state.options_column_rename_input.clear();
                 state.options_column_filter_input.clear();
            }
        } else {
             warn!("Sheet '{}' missing during popup init.", state.options_column_target_sheet);
             state.options_column_rename_input.clear();
             state.options_column_filter_input.clear();
        }
    }
    // --- End Initialization ---


    let mut popup_open = state.show_column_options_popup; // Sync with current state
    let mut cancel_clicked = false; // Flag for cancel
    let mut apply_clicked = false; // Flag for apply action


    egui::Window::new("Column Options")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut popup_open) // Bind to the temporary variable
        .show(ctx, |ui| {
            let header_text = registry
                .get_sheet(&state.options_column_target_sheet)
                .and_then(|s| s.metadata.as_ref())
                .and_then(|m| m.column_headers.get(state.options_column_target_index))
                .map(|s| s.as_str()).unwrap_or("?");

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
                        .desired_width(150.0).lock_focus(true),
                );
                if rename_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !state.options_column_rename_input.trim().is_empty() {
                        apply_clicked = true; // Set flag
                    }
                }
            });

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
                    apply_clicked = true; // Set flag
                }
                if ui.button("Clear").clicked() {
                    state.options_column_filter_input.clear();
                    apply_clicked = true; // Set flag (clearing is an apply action)
                }
            });
            ui.small("Leave empty or clear to disable filter.");

            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_enabled(!state.options_column_rename_input.trim().is_empty(), egui::Button::new("Apply")).clicked() {
                    apply_clicked = true; // Set flag
                }
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true; // Set flag
                }
            });
        }); // End .show()


    // --- Logic AFTER the window UI ---

    let mut close_popup = false;
    let mut changes_applied_this_frame = false; // Track if changes requiring save occurred

    // 1. Handle apply action if triggered
    if apply_clicked {
        let sheet_name = state.options_column_target_sheet.clone();
        let col_index = state.options_column_target_index;
        let mut rename_success = false;
        let mut filter_success = false;

        // --- Apply Rename ---
        let current_name_opt = registry.get_sheet(&sheet_name).and_then(|s| s.metadata.as_ref())
            .and_then(|m| m.column_headers.get(col_index)).cloned();
        let new_name_trimmed = state.options_column_rename_input.trim();

        if Some(new_name_trimmed) != current_name_opt.as_deref() && !new_name_trimmed.is_empty() {
             let is_duplicate = registry.get_sheet(&sheet_name).and_then(|s| s.metadata.as_ref())
                 .map_or(false, |m| m.column_headers.iter().enumerate()
                     .any(|(i, h)| i != col_index && h.eq_ignore_ascii_case(new_name_trimmed)));
             if !is_duplicate {
                 column_rename_writer.send(RequestUpdateColumnName {
                     sheet_name: sheet_name.clone(),
                     column_index: col_index,
                     new_name: new_name_trimmed.to_string(),
                 });
                 changes_applied_this_frame = true;
                 rename_success = true; // Indicate rename attempt was valid
             } else { warn!("Column rename aborted: Name '{}' already exists.", new_name_trimmed); }
        } else if new_name_trimmed.is_empty() {
             warn!("Column rename aborted: New name cannot be empty.");
        } else {
             rename_success = true; // No change needed, counts as "success" for closing popup
        }


        // --- Apply Filter ---
        if let Some(sheet_data) = registry.get_sheet_mut(&sheet_name) {
             if let Some(meta) = &mut sheet_data.metadata {
                 if col_index < meta.column_filters.len() {
                     let new_filter_trimmed = state.options_column_filter_input.trim();
                     let filter_to_store: Option<String> = if new_filter_trimmed.is_empty() { None } else { Some(new_filter_trimmed.to_string()) };
                     if meta.column_filters[col_index] != filter_to_store {
                         meta.column_filters[col_index] = filter_to_store;
                         changes_applied_this_frame = true;
                     }
                     filter_success = true; // Filter update always considered successful for closing
                 } else { warn!("Column index {} out of bounds for filters.", col_index); }
             } else { warn!("Metadata missing when applying filter for sheet '{}'.", sheet_name); }
         } else { warn!("Sheet '{}' missing when applying filter.", sheet_name); }


        // Only close if both actions were processed (or didn't need processing)
        if rename_success && filter_success {
             close_popup = true;
             if changes_applied_this_frame {
                 // Flag save for next frame in editor UI
                 state.sheet_needs_save = true;
                 state.sheet_to_save = sheet_name;
             }
        }
        // If rename or filter logic failed (e.g., validation error), close_popup remains false
    }

    // 2. Handle cancel action if clicked
    if cancel_clicked {
        close_popup = true;
    }

    // 3. Handle closing via 'x' button
    if !close_popup && !popup_open {
        close_popup = true; // Window was closed via 'x'
    }

    // 4. Update the actual state variable if closing is needed
    if close_popup {
        state.show_column_options_popup = false;
        // Reset internal state only when popup actually closes
        state.options_column_target_sheet.clear();
        state.options_column_target_index = 0;
        state.options_column_rename_input.clear();
        state.options_column_filter_input.clear();
        state.column_options_popup_needs_init = false; // Reset init flag
    } else {
        // Ensure state reflects the temporary variable if not closing
        state.show_column_options_popup = popup_open;
    }
}