// src/ui/elements/bottom_panel/dropdowns.rs
use bevy_egui::egui;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ui_handlers::{sheet_handlers, ui_cache};
use crate::ui::elements::editor::state::EditorWindowState;

/// Compute a fixed button width approximating `chars` visible characters for Button text
pub fn fixed_button_width(ui: &egui::Ui, chars: usize) -> f32 {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let ten = "0".repeat(10);
    let ten_w = ui
        .fonts(|f| f.layout_no_wrap(ten, font_id.clone(), ui.style().visuals.text_color()))
        .rect
        .width();
    let per = (ten_w / 10.0).max(4.0);
    let w = per * (chars as f32) + 2.0;
    w.clamp(80.0, 420.0)
}

/// Render sheet selector dropdown with button and popup
pub fn render_sheet_selector(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
    sheets_in_category: &[String],
) {
    let sheet_combo_id = ui_cache::get_sheet_combo_id(state.selected_category.as_deref());
    let sheet_filter_key = ui_cache::get_filter_key(&sheet_combo_id);

    let selected_sheet_text_owned = state.selected_sheet_name.as_deref().unwrap_or("--Select--");
    const MAX_LABEL_CHARS: usize = 32;
    let display_trunc: String = selected_sheet_text_owned.chars().take(MAX_LABEL_CHARS).collect();
    let target_width = fixed_button_width(ui, MAX_LABEL_CHARS);

    ui.add_enabled_ui(
        !sheets_in_category.is_empty() || state.selected_sheet_name.is_some(),
        |ui| {
            let previous_sheet = state.selected_sheet_name.clone();
            let sheet_button = ui.add_sized([target_width, 0.0], egui::Button::new(&display_trunc));
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
                    super::popups::render_sheet_popup(
                        popup_ui,
                        state,
                        sheets_in_category,
                        &sheet_filter_key,
                    );
                },
            );

            // Handle selection change
            if state.selected_sheet_name != previous_sheet {
                sheet_handlers::handle_sheet_selection(state, state.selected_sheet_name.clone());
            }

            sheet_handlers::validate_sheet_selection(state, registry);
        },
    );
}

/// Render category selector dropdown with button and popup
pub fn render_category_selector(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
    categories: &[Option<String>],
    event_writers: &mut super::SheetManagementEventWriters,
    drop_consumed: &mut bool,
) {
    const MAX_LABEL_CHARS: usize = 32;
    let selected_category_full: String = state
        .selected_category
        .clone()
        .unwrap_or_else(|| "--Root--".to_string());
    let selected_category_text_owned: String = selected_category_full
        .chars()
        .take(MAX_LABEL_CHARS)
        .collect();

    let category_combo_id = ui_cache::get_category_combo_id(&selected_category_full);
    let category_filter_key = ui_cache::get_filter_key(&category_combo_id);

    let btn_w = fixed_button_width(ui, MAX_LABEL_CHARS);
    let category_button = ui.add_sized([btn_w, 0.0], egui::Button::new(&selected_category_text_owned));
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
            super::popups::render_category_popup(
                popup_ui,
                state,
                registry,
                categories,
                &category_filter_key,
                event_writers,
                drop_consumed,
            );
        },
    );
}
