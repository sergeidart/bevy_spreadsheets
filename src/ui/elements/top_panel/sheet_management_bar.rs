// src/ui/elements/top_panel/sheet_management_bar.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::RequestInitiateFileUpload;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{EditorWindowState, SheetInteractionState};
use crate::visual_copier::events::RequestAppExit;

// Helper struct signature might need to use owned EventWriter if cloned,
// or keep &mut if that's the pattern. Let's assume it needs to match what orchestrator passes.
// If orchestrator passes cloned owned writers, this struct changes.
// If orchestrator continues to pass &mut EventWriter<T> (by reborrowing from &mut SheetEventWriters),
// this struct definition is fine.

// Sticking to the previous pattern of this struct taking mutable references to EventWriters
pub(super) struct SheetManagementEventWriters<'a, 'w> {
    pub upload_req_writer: &'a mut EventWriter<'w, RequestInitiateFileUpload>,
    pub request_app_exit_writer: &'a mut EventWriter<'w, RequestAppExit>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn show_sheet_management_controls<'a, 'w>(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    event_writers: SheetManagementEventWriters<'a, 'w>, // Takes the struct of mutable refs
) {
    ui.label("Category:");
    let categories = registry.get_categories();
    let selected_category_text = state
        .selected_category
        .as_deref()
        .unwrap_or("Root (Uncategorized)");

    let category_response = egui::ComboBox::from_id_source("category_selector_top_panel_refactored")
        .selected_text(selected_category_text)
        .show_ui(ui, |ui| {
            let is_selected_root = state.selected_category.is_none();
            if ui
                .selectable_label(is_selected_root, "Root (Uncategorized)")
                .clicked()
            {
                if !is_selected_root {
                    state.selected_category = None;
                    state.selected_sheet_name = None;
                    state.reset_interaction_modes_and_selections();
                    state.force_filter_recalculation = true;
                    state.ai_rule_popup_needs_init = true;
                }
            }
            for cat_opt in categories.iter() {
                if let Some(cat_name) = cat_opt {
                    let is_selected_cat =
                        state.selected_category.as_deref() == Some(cat_name.as_str());
                    if ui.selectable_label(is_selected_cat, cat_name).clicked() {
                        if !is_selected_cat {
                            state.selected_category = Some(cat_name.clone());
                            state.selected_sheet_name = None;
                            state.reset_interaction_modes_and_selections();
                            state.force_filter_recalculation = true;
                            state.ai_rule_popup_needs_init = true;
                        }
                    }
                }
            }
        });

    if category_response.response.changed() {
        if state.selected_sheet_name.is_none() {
            state.reset_interaction_modes_and_selections();
        }
        state.force_filter_recalculation = true;
        state.ai_rule_popup_needs_init = true;
    }
    ui.separator(); 
    
    if ui
        .button("➕ New Sheet")
        .on_hover_text("Create a new empty sheet in the current category")
        .clicked()
    {
        state.new_sheet_target_category = state.selected_category.clone();
        state.new_sheet_name_input.clear();
        state.show_new_sheet_popup = true;
    }
    ui.separator();

    if ui
        .button("⬆ Upload JSON")
        .on_hover_text("Upload a JSON file (will be placed in Root category)")
        .clicked()
    {
        event_writers.upload_req_writer.write(RequestInitiateFileUpload); // Use directly
    }
    ui.separator();

    ui.label("Sheet:");
    let sheets_in_category = registry.get_sheet_names_in_category(&state.selected_category);
    ui.add_enabled_ui(
        !sheets_in_category.is_empty() || state.selected_sheet_name.is_some(),
        |ui| {
            let selected_sheet_text = state.selected_sheet_name.as_deref().unwrap_or("--Select--");
            let sheet_response: egui::InnerResponse<Option<()>> =
                egui::ComboBox::from_id_source("sheet_selector_top_panel_refactored") 
                    .selected_text(selected_sheet_text)
                    .show_ui(ui, |ui| {
                        let original_selection = state.selected_sheet_name.clone();
                        ui.selectable_value(&mut state.selected_sheet_name, None, "--Select--");
                        for name in sheets_in_category {
                            ui.selectable_value(
                                &mut state.selected_sheet_name,
                                Some(name.clone()),
                                &name,
                            );
                        }
                        if state.selected_sheet_name != original_selection {
                            state.reset_interaction_modes_and_selections();
                            state.force_filter_recalculation = true;
                            state.ai_rule_popup_needs_init = true;
                        }
                    });
            if sheet_response.response.changed() {
                if state.selected_sheet_name.is_none() {
                    state.reset_interaction_modes_and_selections();
                }
                state.force_filter_recalculation = true;
                state.ai_rule_popup_needs_init = true;
            }
            if let Some(current_sheet_name) = state.selected_sheet_name.as_ref() {
                if !registry
                    .get_sheet_names_in_category(&state.selected_category)
                    .contains(current_sheet_name)
                {
                    state.selected_sheet_name = None;
                    state.reset_interaction_modes_and_selections();
                    state.force_filter_recalculation = true;
                    state.ai_rule_popup_needs_init = true;
                }
            }
        },
    );

    let is_sheet_selected = state.selected_sheet_name.is_some();
    let can_manage_sheet =
        is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;

    if ui
        .add_enabled(can_manage_sheet, egui::Button::new("✏ Rename"))
        .clicked()
    {
        if let Some(ref name_to_rename) = state.selected_sheet_name {
            state.rename_target_category = state.selected_category.clone();
            state.rename_target_sheet = name_to_rename.clone();
            state.new_name_input = state.rename_target_sheet.clone();
            state.show_rename_popup = true;
        }
    }
    if ui
        .add_enabled(can_manage_sheet, egui::Button::new("🗑 Delete Sheet"))
        .clicked()
    {
        if let Some(ref name_to_delete) = state.selected_sheet_name {
            state.delete_target_category = state.selected_category.clone();
            state.delete_target_sheet = name_to_delete.clone();
            state.show_delete_confirm_popup = true;
        }
    }

    let copy_button_text = if state.show_quick_copy_bar {
        "❌ Close Copy"
    } else {
        "📋 Copy"
    };
    if ui.button(copy_button_text).clicked() {
        state.show_quick_copy_bar = !state.show_quick_copy_bar;
    }

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.add(egui::Button::new("❌ App Exit")).clicked() {
            info!("'App Exit' button clicked. Sending RequestAppExit event.");
            event_writers.request_app_exit_writer.write(RequestAppExit); // Use directly
        }
    });
}