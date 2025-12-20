// src/ui/elements/popups/column_options_ui.rs
use super::column_options_validator::{is_validator_config_valid, show_validator_section};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*; // Keep bevy prelude
use bevy_egui::egui; // Import helper

// Structure to hold the results of the UI interaction
pub(super) struct ColumnOptionsUiResult {
    pub apply_clicked: bool,
    pub cancel_clicked: bool,
    pub close_via_x: bool,
}

/// Renders the main UI elements for the column options popup window.
pub(super) fn show_column_options_window_ui(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    registry_immut: &SheetRegistry, // Immutable borrow for display
) -> ColumnOptionsUiResult {
    let mut popup_open = state.show_column_options_popup; // Use state value
    let mut apply_clicked = false;
    let mut cancel_clicked = false;

    // Cache category/sheet name for use inside closure
    let popup_category = state.options_column_target_category.clone();
    let popup_sheet_name = state.options_column_target_sheet.clone();

    egui::Window::new("Column Options")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut popup_open) // Control opening via state flag
        .show(ctx, |ui| {
            // Get column definition using index (unused for minimal header, kept for potential future use)
            let _column_def_opt = registry_immut
                .get_sheet(&popup_category, &popup_sheet_name) // Use cached category/name
                .and_then(|s| s.metadata.as_ref())
                .and_then(|m| m.columns.get(state.options_column_target_index));

            // Minimal header only (no verbose subtitles)

            // Name field
            ui.strong("Name");
            let rename_resp = ui.add(
                egui::TextEdit::singleline(&mut state.options_column_rename_input)
                    .desired_width(150.0)
                    .lock_focus(true), // Keep focus on open
            );
            if rename_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if !state.options_column_rename_input.trim().is_empty()
                    && is_validator_config_valid(state)
                {
                    apply_clicked = true;
                }
            }
            ui.separator();

            // --- Filter Section (Multi-term OR) with stacking ---
            let _filter_title = ui.add(egui::Label::new(
                egui::RichText::new("Filter (OR)").strong(),
            ));
            
            // Build stacked representation: (normalized_key, display_value, count)
            // We preserve insertion order by using the first occurrence for display
            let mut stacked_filters: Vec<(String, String, usize)> = Vec::new();
            let mut seen_normalized: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            
            for term in state.options_column_filter_terms.iter() {
                let trimmed = term.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let normalized = trimmed.to_lowercase();
                if let Some(&idx) = seen_normalized.get(&normalized) {
                    // Increment count for existing entry
                    stacked_filters[idx].2 += 1;
                } else {
                    // Add new entry
                    seen_normalized.insert(normalized.clone(), stacked_filters.len());
                    stacked_filters.push((normalized, trimmed.to_string(), 1));
                }
            }
            
            // Sort by display name (alphabetically) for easier finding
            stacked_filters.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
            
            // Track changes to apply back
            let mut term_to_add: Option<String> = None;
            let mut entry_to_reduce: Option<String> = None;
            let mut entry_to_remove: Option<String> = None;
            
            // Scrollable area for filter entries
            const FILTER_TERM_MAX_WIDTH: f32 = 180.0;
            
            egui::ScrollArea::vertical()
                .id_salt("filter_scroll")
                .max_height(150.0)
                .show(ui, |scroll_ui| {
                    for (normalized, display, count) in stacked_filters.iter() {
                        scroll_ui.horizontal(|row_ui| {
                            // Fixed-width area for filter term with text wrapping
                            row_ui.allocate_ui_with_layout(
                                egui::vec2(FILTER_TERM_MAX_WIDTH, 0.0),
                                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                                |term_ui| {
                                    term_ui.label(display.as_str());
                                },
                            );
                            
                            // Show count if 2+
                            if *count >= 2 {
                                row_ui.label(format!("({})", count));
                                // Show "-" button to reduce count
                                if row_ui.small_button("-").on_hover_text("Reduce count by 1").clicked() {
                                    entry_to_reduce = Some(normalized.clone());
                                }
                            }
                            
                            // Always show x button to remove entirely
                            if row_ui.small_button("x").on_hover_text("Remove all").clicked() {
                                entry_to_remove = Some(normalized.clone());
                            }
                        });
                    }
                });
            
            // Input field for adding new filter terms
            ui.horizontal(|ui_h| {
                // Create a temp string for input - we'll add it when Enter is pressed
                let input_id = egui::Id::new("filter_new_term_input");
                let mut input_text = ui_h.memory(|mem| {
                    mem.data.get_temp::<String>(input_id).unwrap_or_default()
                });
                
                let resp = ui_h.add(
                    egui::TextEdit::singleline(&mut input_text)
                        .desired_width(150.0)
                        .hint_text("add filter term"),
                );
                
                ui_h.memory_mut(|mem| {
                    mem.data.insert_temp(input_id, input_text.clone());
                });
                
                // Add on Enter or button click
                let add_clicked = ui_h.small_button("+").on_hover_text("Add filter term").clicked();
                if (resp.lost_focus() && ui_h.input(|inp| inp.key_pressed(egui::Key::Enter))) || add_clicked {
                    let trimmed = input_text.trim();
                    if !trimmed.is_empty() {
                        term_to_add = Some(trimmed.to_string());
                        // Clear the input
                        ui_h.memory_mut(|mem| {
                            mem.data.insert_temp::<String>(input_id, String::new());
                        });
                    } else if resp.lost_focus() && ui_h.input(|inp| inp.key_pressed(egui::Key::Enter)) {
                        // Enter on valid filter state triggers apply
                        if is_validator_config_valid(state) {
                            apply_clicked = true;
                        }
                    }
                }
            });
            
            // Apply changes
            if let Some(term) = term_to_add {
                state.options_column_filter_terms.push(term);
            }
            
            if let Some(normalized_to_reduce) = entry_to_reduce {
                // Remove one occurrence of this normalized term
                let mut found = false;
                state.options_column_filter_terms.retain(|t| {
                    if !found && t.trim().to_lowercase() == normalized_to_reduce {
                        found = true;
                        false // Remove this one
                    } else {
                        true
                    }
                });
            }
            
            if let Some(normalized_to_remove) = entry_to_remove {
                // Remove all occurrences of this normalized term
                state.options_column_filter_terms.retain(|t| {
                    t.trim().to_lowercase() != normalized_to_remove
                });
            }
            
            // Ensure at least one empty slot if all removed
            if state.options_column_filter_terms.is_empty() {
                state.options_column_filter_terms.push(String::new());
            }
            
            ui.horizontal(|ui_h| {
                if ui_h.button("Clear All").clicked() {
                    state.options_column_filter_terms = vec![String::new()];
                }
            });
            ui.separator();

            // AI Context Section - starts at 2 rows, grows to max 5 based on content
            ui.strong("AI Context");
            let row_height = ui.text_style_height(&egui::TextStyle::Body);
            // Count approximate lines in the content
            let content_lines = state.options_column_ai_context_input.lines().count().max(1);
            // Use 2 rows minimum, grow up to 5 based on content
            let display_rows = (content_lines as f32).clamp(2.0, 5.0);
            let max_height = row_height * 5.0 + 16.0; // max 5 rows
            
            egui::ScrollArea::vertical()
                .id_salt("ai_context_scroll")
                .max_height(max_height)
                .show(ui, |scroll_ui| {
                    scroll_ui.add(
                        egui::TextEdit::multiline(&mut state.options_column_ai_context_input)
                            .desired_width(f32::INFINITY)
                            .desired_rows(display_rows as usize),
                    );
                });
            ui.separator();

            // --- Hidden Column Checkbox (only for non-structure columns) ---
            // Check if this column is a structure column (validator == Structure)
            let is_structure_column = matches!(
                state.options_validator_type,
                Some(crate::ui::elements::editor::state::ValidatorTypeChoice::Structure)
            );
            
            if !is_structure_column {
                ui.horizontal(|ui_h| {
                    ui_h.checkbox(&mut state.options_column_hidden_input, "Hidden")
                        .on_hover_text("Hide this column from the default view. Use 'Show hidden' in Settings to reveal.");
                });
                ui.separator();
            }

            // --- Validator Section (using helper) ---
            show_validator_section(ui, state, registry_immut);
            ui.separator();

            // Confirmation moved to dedicated popup window.

            // --- Action Buttons ---
            ui.horizontal(|ui| {
                let apply_enabled = !state.options_column_rename_input.trim().is_empty()
                    && is_validator_config_valid(state)
                    && !state.pending_validator_change_requires_confirmation; // disable while awaiting confirm
                if ui
                    .add_enabled(apply_enabled, egui::Button::new("Apply"))
                    .clicked()
                {
                    apply_clicked = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true;
                }
            });
        }); // End .show()

    // Determine if closed via 'x' button
    let close_via_x = state.show_column_options_popup && !popup_open;

    ColumnOptionsUiResult {
        apply_clicked,
        cancel_clicked,
        close_via_x,
    }
}
