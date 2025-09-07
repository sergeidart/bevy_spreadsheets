// src/ui/elements/popups/column_options_ui.rs
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*; // Keep bevy prelude
use bevy_egui::egui;
use super::column_options_validator::{is_validator_config_valid, show_validator_section}; // Import helper

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
                egui::TextEdit::singleline(
                    &mut state.options_column_rename_input,
                )
                .desired_width(150.0)
                .lock_focus(true), // Keep focus on open
            );
            if rename_resp.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
            {
                if !state.options_column_rename_input.trim().is_empty()
                    && is_validator_config_valid(state)
                {
                    apply_clicked = true;
                }
            }
            ui.separator();

            // --- Filter Section (Multi-term OR) ---
            let _filter_title = ui.add(egui::Label::new(egui::RichText::new("Filter (OR)" ).strong()));
            // Ensure at least one term
            if state.options_column_filter_terms.is_empty() { state.options_column_filter_terms.push(String::new()); }
            let mut to_remove: Vec<usize> = Vec::new();
            for i in 0..state.options_column_filter_terms.len() {
                ui.horizontal(|ui_h| {
                    let resp = ui_h.add(
                        egui::TextEdit::singleline(&mut state.options_column_filter_terms[i])
                            .desired_width(150.0)
                            .hint_text("contains fragment"),
                    );
                    if resp.lost_focus() && ui_h.input(|inp| inp.key_pressed(egui::Key::Enter)) {
                        if is_validator_config_valid(state) { apply_clicked = true; }
                    }
                    if i + 1 < state.options_column_filter_terms.len() && ui_h.small_button("x").on_hover_text("Remove").clicked() {
                        to_remove.push(i);
                    }
                });
            }
            if !to_remove.is_empty() {
                for idx in to_remove.into_iter().rev() { if idx < state.options_column_filter_terms.len() { state.options_column_filter_terms.remove(idx); } }
                if state.options_column_filter_terms.is_empty() { state.options_column_filter_terms.push(String::new()); }
            }
            let need_new = state.options_column_filter_terms.last().map(|s| !s.is_empty()).unwrap_or(false);
            if need_new && state.options_column_filter_terms.len() < 12 { state.options_column_filter_terms.push(String::new()); }
            ui.horizontal(|ui_h| {
                if ui_h.button("Clear All").clicked() { state.options_column_filter_terms = vec![String::new()]; }
            });
            ui.separator();

            // AI Context Section
            ui.strong("AI Context");
            ui.add(
                egui::TextEdit::multiline(
                    &mut state.options_column_ai_context_input,
                )
                .desired_width(f32::INFINITY)
                .desired_rows(2),
            );
            ui.separator();

            // --- Validator Section (using helper) ---
            show_validator_section(ui, state, registry_immut);
            ui.separator();

            // Confirmation moved to dedicated popup window.

            // --- Action Buttons ---
            ui.horizontal(|ui| {
                let apply_enabled =
                    !state.options_column_rename_input.trim().is_empty()
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