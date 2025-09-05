// src/ui/elements/top_panel/sheet_management_bar.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::RequestInitiateFileUpload;
use crate::sheets::events::CloseStructureViewEvent;
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
    pub close_structure_writer: &'a mut EventWriter<'w, CloseStructureViewEvent>,
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
    // Use owned copies for popup closures to avoid holding an immutable borrow of `state`.
    let selected_category_text_owned = selected_category_text.to_string();

    // Filter for categories: render outside the ComboBox so focusing it won't close a popup.
    let category_combo_id = format!("category_selector_top_panel_refactored_{}", selected_category_text);
    let category_filter_key = format!("{}_filter", category_combo_id);
    // Show selection via a button that opens a popup below it. The popup contains
    // a small inline filter TextEdit (so focusing it doesn't close the anchor widget).
    let previous_selected_category = state.selected_category.clone();
    let category_button = ui.button(&selected_category_text_owned);
    let category_popup_id = egui::Id::new(category_combo_id.clone());
    if category_button.clicked() {
        ui.ctx().memory_mut(|mem| mem.open_popup(category_popup_id));
    }
    egui::containers::popup::popup_below_widget(
        ui,
        category_popup_id,
        &category_button,
        egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
        |popup_ui| {
                    // Read and size the popup based on longest category name so long names fit.
                    let mut filter_text = popup_ui
                        .memory(|mem| mem.data.get_temp::<String>(category_filter_key.clone().into()).unwrap_or_default());
                    // Compute a reasonable min width from the longest category name to fit long names.
                    // Size popup to max item name length + small padding
                    let char_w = 8.0_f32; // tighter per-character estimate
                    let max_name_len = categories
                        .iter()
                        .filter_map(|o| o.as_ref())
                        .map(|s| s.len())
                        .max()
                        .unwrap_or(12);
                    let padding = 24.0_f32; // small horizontal padding
                    let mut popup_min_width = (max_name_len as f32) * char_w + padding;
                    if popup_min_width < 120.0 { popup_min_width = 120.0; }
                    if popup_min_width > 900.0 { popup_min_width = 900.0; }
                    popup_ui.set_min_width(popup_min_width);

                    popup_ui.horizontal(|ui_h| {
                        ui_h.label("Filter:");
                        let avail = ui_h.available_width();
                        // Use a fixed comfortable default width for the filter input (doesn't grow with text)
                        let default_chars = 28usize; // default input width in characters
                        let desired = (default_chars as f32) * char_w;
                        // Keep input width not wider than popup_min_width minus padding and available width
                        let width = desired.min(avail).min(popup_min_width - 40.0);
                        let resp = ui_h.add(
                            egui::TextEdit::singleline(&mut filter_text)
                                .desired_width(width)
                                .hint_text("type to filter categories"),
                        );
                        if resp.changed() {
                            ui_h.memory_mut(|mem| mem.data.insert_temp(category_filter_key.clone().into(), filter_text.clone()));
                        }
                        if ui_h.small_button("x").clicked() {
                            filter_text.clear();
                            ui_h.memory_mut(|mem| mem.data.insert_temp(category_filter_key.clone().into(), filter_text.clone()));
                        }
                    });

            let is_selected_root = state.selected_category.is_none();
            let current_filter = filter_text.clone();
            if current_filter.is_empty() || "root (uncategorized)".contains(&current_filter.to_lowercase()) {
                if popup_ui
                    .selectable_label(is_selected_root, "Root (Uncategorized)")
                    .clicked()
                {
                    if !is_selected_root {
                        state.selected_category = None;
                        state.selected_sheet_name = None;
                        state.reset_interaction_modes_and_selections();
                        state.force_filter_recalculation = true;
                        state.ai_rule_popup_needs_init = true;
                        popup_ui.memory_mut(|mem| mem.close_popup());
                    }
                }
            }

            for cat_opt in categories.iter() {
                if let Some(cat_name) = cat_opt {
                    if !current_filter.is_empty() && !cat_name.to_lowercase().contains(&current_filter.to_lowercase()) {
                        continue;
                    }
                    let is_selected_cat = state.selected_category.as_deref() == Some(cat_name.as_str());
                    if popup_ui.selectable_label(is_selected_cat, cat_name).clicked() {
                        if !is_selected_cat {
                            state.selected_category = Some(cat_name.clone());
                            state.selected_sheet_name = None;
                            state.reset_interaction_modes_and_selections();
                            state.force_filter_recalculation = true;
                            state.ai_rule_popup_needs_init = true;
                        }
                        popup_ui.memory_mut(|mem| mem.close_popup());
                    }
                }
            }
        },
    );
    if state.selected_category != previous_selected_category {
        if state.selected_sheet_name.is_none() {
            state.reset_interaction_modes_and_selections();
        }
        state.force_filter_recalculation = true;
        state.ai_rule_popup_needs_init = true;
    }
    ui.separator(); 
    // Back button handled in top panel orchestrator for virtual structure stack.
    
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
    // Sheet picker filter (render outside the ComboBox so typing doesn't close any open popups)
    let sheet_combo_id = format!(
        "sheet_selector_top_panel_refactored_{}_{}",
        state.selected_category.as_deref().unwrap_or("Root"),
        state.selected_sheet_name.as_deref().unwrap_or("")
    );
    let sheet_filter_key = format!("{}_filter", sheet_combo_id);
    // Owned display text for sheet button to avoid borrowing state inside popup closures
    let selected_sheet_text_owned = state.selected_sheet_name.as_deref().unwrap_or("--Select--").to_string();
    ui.add_enabled_ui(!sheets_in_category.is_empty() || state.selected_sheet_name.is_some(), |ui| {
        let previous_sheet = state.selected_sheet_name.clone();
        let sheet_button = ui.button(&selected_sheet_text_owned);
        let sheet_popup_id = egui::Id::new(sheet_combo_id.clone());
        if sheet_button.clicked() {
            ui.ctx().memory_mut(|mem| mem.open_popup(sheet_popup_id));
        }
        egui::containers::popup::popup_below_widget(
            ui,
            sheet_popup_id,
            &sheet_button,
            egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
            |popup_ui| {
                // filter input inside popup; size popup according to longest sheet name
                let mut filter_text = popup_ui.memory(|mem| mem.data.get_temp::<String>(sheet_filter_key.clone().into()).unwrap_or_default());
                // compute desired popup width from longest sheet name
                let char_w = 10.0_f32;
                let max_name_len = sheets_in_category.iter().map(|s| s.len()).max().unwrap_or(12);
                let mut popup_min_width = (max_name_len.max(12) as f32) * char_w;
                if popup_min_width > 900.0 { popup_min_width = 900.0; }
                popup_ui.set_min_width(popup_min_width);

                popup_ui.horizontal(|ui_h| {
                    ui_h.label("Filter:");
                    let avail = ui_h.available_width();
                    let default_chars = 28usize;
                    let desired = (default_chars as f32) * char_w;
                    let width = desired.min(avail).min(popup_min_width * 0.95);
                    let resp = ui_h.add(egui::TextEdit::singleline(&mut filter_text).desired_width(width).hint_text("type to filter sheets"));
                    if resp.changed() { ui_h.memory_mut(|mem| mem.data.insert_temp(sheet_filter_key.clone().into(), filter_text.clone())); }
                    if ui_h.small_button("x").clicked() { filter_text.clear(); ui_h.memory_mut(|mem| mem.data.insert_temp(sheet_filter_key.clone().into(), filter_text.clone())); }
                });

                popup_ui.selectable_value(&mut state.selected_sheet_name, None, "--Select--");
                for name in sheets_in_category.iter().filter(|n| {
                    filter_text.is_empty() || n.to_lowercase().contains(&filter_text.to_lowercase())
                }) {
                    if popup_ui.selectable_label(state.selected_sheet_name.as_deref() == Some(name.as_str()), name).clicked() {
                        state.selected_sheet_name = Some(name.clone());
                        state.reset_interaction_modes_and_selections();
                        state.force_filter_recalculation = true;
                        state.ai_rule_popup_needs_init = true;
                        state.show_column_options_popup = false;
                        popup_ui.memory_mut(|mem| mem.close_popup());
                    }
                }
            },
        );
        // after popup usage, handle selection change
        if state.selected_sheet_name != previous_sheet {
            if state.selected_sheet_name.is_none() {
                if !state.virtual_structure_stack.is_empty() {
                    event_writers.close_structure_writer.write(CloseStructureViewEvent);
                }
                state.reset_interaction_modes_and_selections();
            }
            state.force_filter_recalculation = true;
            state.ai_rule_popup_needs_init = true;
            state.show_column_options_popup = false;
        }
        if let Some(current_sheet_name) = state.selected_sheet_name.as_ref() {
            if !registry.get_sheet_names_in_category(&state.selected_category).contains(current_sheet_name) {
                state.selected_sheet_name = None;
                state.reset_interaction_modes_and_selections();
                state.force_filter_recalculation = true;
                state.ai_rule_popup_needs_init = true;
                state.show_column_options_popup = false;
            }
        }
    });

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