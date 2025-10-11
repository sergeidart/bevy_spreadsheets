// src/sheets/systems/ui_handlers/ui_cache.rs
use bevy_egui::egui;
use crate::ui::validation::normalize_for_link_cmp;

/// Get filter text from egui memory for a given filter key
pub fn get_filter_text(ui: &egui::Ui, filter_key: &str) -> String {
    let id = egui::Id::new(filter_key);
    ui.memory(|mem| {
        mem.data
            .get_temp::<String>(id)
            .unwrap_or_default()
    })
}

/// Save filter text to egui memory
pub fn save_filter_text(ui: &mut egui::Ui, filter_key: &str, text: String) {
    let id = egui::Id::new(filter_key);
    ui.memory_mut(|mem| {
        mem.data.insert_temp(id, text)
    });
}

/// Clear filter text in egui memory
pub fn clear_filter_text(ui: &mut egui::Ui, filter_key: &str) {
    save_filter_text(ui, filter_key, String::new());
}

/// Check if an item matches the current filter
pub fn matches_filter(item_name: &str, filter_text: &str) -> bool {
    filter_text.is_empty()
        || normalize_for_link_cmp(item_name).contains(&normalize_for_link_cmp(filter_text))
}

/// Check if root category matches filter
pub fn root_matches_filter(filter_text: &str) -> bool {
    if filter_text.is_empty() {
        return true;
    }
    let root_match = "--root--".to_string();
    normalize_for_link_cmp(&root_match).contains(&normalize_for_link_cmp(filter_text))
        || normalize_for_link_cmp("root (uncategorized)").contains(&normalize_for_link_cmp(filter_text))
}

/// Generate a unique combo box ID for category selector
pub fn get_category_combo_id(selected_category: &str) -> String {
    format!("category_selector_top_panel_refactored_{}", selected_category)
}

/// Generate a unique combo box ID for sheet selector
pub fn get_sheet_combo_id(category_name: Option<&str>) -> String {
    format!(
        "sheet_selector_{}",
        category_name.unwrap_or("root")
    )
}

/// Get filter key for a combo box
pub fn get_filter_key(combo_id: &str) -> String {
    format!("{}_filter", combo_id)
}

/// Calculate popup minimum width based on longest name
pub fn calc_popup_width(names: &[impl AsRef<str>], char_width: f32) -> f32 {
    let max_name_len = names
        .iter()
        .map(|s| s.as_ref().len())
        .max()
        .unwrap_or(12);
    let width = (max_name_len as f32) * char_width + 24.0;
    width.clamp(160.0, 900.0)
}
