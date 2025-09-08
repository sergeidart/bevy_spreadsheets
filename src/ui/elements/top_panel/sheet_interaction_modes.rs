// src/ui/elements/top_panel/sheet_interaction_modes.rs
// no bevy prelude items used directly here
use bevy_egui::egui;

// no sheet events needed here after refactor
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState, SheetInteractionState};
use crate::sheets::resources::SheetRegistry;
// use crate::sheets::definitions::RandomPickerMode; // (Initialization now handled when Toybox is open)

#[allow(unused_variables)]
pub(super) fn show_sheet_interaction_mode_buttons<'a, 'w>(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    _registry: &SheetRegistry, // Mark as unused with underscore
) {
    let is_sheet_selected = state.current_sheet_context().1.is_some();
    // Removed: top-level Add Row / Add Column buttons (use inline '+' controls on the table instead)
    // No separator before AI row

    if state.current_interaction_mode == SheetInteractionState::AiModeActive {
        if ui.button("❌ Exit AI").clicked() {
            state.reset_interaction_modes_and_selections();
        }
    } else {
        // Always show 'AI Mode' button when not in AI mode
        let ai_btn = ui.add_enabled(is_sheet_selected, egui::Button::new("✨ AI Mode"))
            .on_hover_text("Enable row selection and AI controls");
        {
            let rect = ai_btn.rect;
            state.last_ai_button_min_x = rect.min.x;
        }
        if ai_btn.clicked() {
            // Exclusivity: leaving Delete/Toybox when entering AI
            state.show_edit_mode_panel = false;
            state.show_toybox_menu = false;
            state.current_interaction_mode = SheetInteractionState::AiModeActive;
            state.ai_mode = AiModeState::Preparing;
            state.ai_selected_rows.clear();
        }
    }
    // No separator between AI and Delete rows

    // Delete Mode toggler
    let edit_label = if state.show_edit_mode_panel { "🗑 Exit Delete" } else { "🗑 Delete" };
    // Record the x position so the below-row delete button can align under this toggle
    let edit_btn_resp = ui.add_enabled(is_sheet_selected, egui::Button::new(edit_label));
    if edit_btn_resp.clicked() {
        let will_show = !state.show_edit_mode_panel;
        state.show_edit_mode_panel = will_show;
        if will_show {
            // Exclusivity: hide AI and Toybox when entering Delete Mode
            state.show_toybox_menu = false;
            state.current_interaction_mode = SheetInteractionState::DeleteModeActive;
        } else if state.current_interaction_mode == SheetInteractionState::DeleteModeActive {
            state.reset_interaction_modes_and_selections();
        }
    }
    {
        let rect = edit_btn_resp.rect;
        state.last_edit_mode_button_min_x = rect.min.x;
    }

    // No separator before Toybox
    // Toybox menu button that groups Randomizer and Summarizer
    let toybox_label = if state.show_toybox_menu { "Exit Toybox" } else { "Toybox" };
    let toybox_btn = ui.add_enabled(is_sheet_selected, egui::Button::new(toybox_label));
    {
        let rect = toybox_btn.rect;
        state.last_toybox_button_min_x = rect.min.x;
    }
    if toybox_btn.clicked() {
        let will_show = !state.show_toybox_menu;
        state.show_toybox_menu = will_show;
        if will_show {
            // Exclusivity: hide AI and Delete Mode when opening Toybox
            state.show_edit_mode_panel = false;
            state.current_interaction_mode = SheetInteractionState::Idle;
        }
        // Sub-panels render on the same line as their UI below
    }
}
