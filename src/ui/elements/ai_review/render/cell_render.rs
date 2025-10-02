// Cell rendering helpers for AI batch review UI
use crate::ui::elements::editor::state::ReviewChoice;
use crate::ui::validation::normalize_for_link_cmp;
use bevy_egui::egui::{self, Color32, RichText};
use std::collections::HashSet;

pub fn render_review_original_cell(
    ui: &mut egui::Ui,
    original_value: &str,
    ai_value: Option<&str>,
    choice_opt: Option<ReviewChoice>,
) {
    let display = if original_value.trim().is_empty() {
        "(empty)"
    } else {
        original_value
    };
    let mut text = RichText::new(display);
    if let (Some(ReviewChoice::AI), Some(ai_val)) = (choice_opt, ai_value) {
        if !ai_val.trim().is_empty() && ai_val != original_value {
            text = text.strikethrough();
        }
    }
    ui.label(text);
}

pub fn render_review_ai_cell(
    ui: &mut egui::Ui,
    original_value: &str,
    ai_cell_opt: Option<&mut String>,
) -> bool {
    if let Some(ai_cell) = ai_cell_opt {
        let is_diff = ai_cell.as_str() != original_value;
        ui.add(
            egui::TextEdit::singleline(ai_cell)
                .desired_width(f32::INFINITY)
                .text_color_opt(if is_diff {
                    Some(Color32::LIGHT_YELLOW)
                } else {
                    None
                }),
        )
        .changed()
    } else {
        ui.label("");
        false
    }
}

pub fn render_review_choice_cell(
    ui: &mut egui::Ui,
    choice_opt: Option<&mut ReviewChoice>,
    original_value: &str,
    ai_value: Option<&str>,
) -> bool {
    if let Some(choice) = choice_opt {
        let ai_val = ai_value.unwrap_or_default();
        if original_value == ai_val {
            ui.small(RichText::new("Same").color(Color32::GRAY));
            return false;
        }
        let mut changed = false;
        if ui
            .radio_value(choice, ReviewChoice::Original, "Orig")
            .clicked()
        {
            changed = true;
        }
        if ui.radio_value(choice, ReviewChoice::AI, "AI").clicked() {
            changed = true;
        }
        changed
    } else {
        ui.label("");
        false
    }
}

/// Render AI cell with linked column dropdown support
pub fn render_review_ai_cell_linked(
    ui: &mut egui::Ui,
    original_value: &str,
    ai_cell: &mut String,
    allowed_values: &HashSet<String>,
    cell_id: egui::Id,
) -> bool {
    let is_diff = ai_cell.as_str() != original_value;
    let is_valid = allowed_values.is_empty() 
        || allowed_values.iter().any(|v| normalize_for_link_cmp(v) == normalize_for_link_cmp(ai_cell));
    
    let mut changed = false;
    let text_color = if is_diff {
        Some(Color32::LIGHT_YELLOW)
    } else {
        None
    };
    
    let bg_color = if !is_valid && !ai_cell.is_empty() {
        Some(Color32::from_rgb(100, 40, 40))
    } else {
        None
    };
    
    // Text edit with styling - use full width like base sheets
    let text_edit_id = cell_id.with("text");
    let popup_id = cell_id.with("popup");
    
    let resp = ui.add(
        egui::TextEdit::singleline(ai_cell)
            .id(text_edit_id)
            .desired_width(f32::INFINITY)
            .text_color_opt(text_color)
    );
    
    // Paint background for invalid values
    if let Some(color) = bg_color {
        ui.painter().rect_filled(
            resp.rect,
            2.0,
            color.linear_multiply(0.3),
        );
    }
    
    changed = resp.changed();
    
    // Open popup on focus (like base sheets)
    if resp.gained_focus() {
        ui.memory_mut(|mem| mem.open_popup(popup_id));
    }
    
    // Popup with suggestions
    egui::popup_below_widget(ui, popup_id, &resp, egui::popup::PopupCloseBehavior::CloseOnClickOutside, |popup_ui| {
        popup_ui.set_min_width(resp.rect.width().max(200.0));
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .auto_shrink([false; 2])
            .show(popup_ui, |scroll_ui| {
                let current_norm = normalize_for_link_cmp(ai_cell);
                let mut suggestions: Vec<&String> = allowed_values
                    .iter()
                    .filter(|v| normalize_for_link_cmp(v).contains(&current_norm))
                    .collect();
                suggestions.sort_unstable_by(|a, b| {
                    normalize_for_link_cmp(a).cmp(&normalize_for_link_cmp(b))
                });
                
                if suggestions.is_empty() && !allowed_values.is_empty() {
                    scroll_ui.label("(No matching options)");
                } else if allowed_values.is_empty() {
                    scroll_ui.label("(No options available)");
                } else {
                    for suggestion in suggestions.into_iter().take(50) {
                        let is_selected = normalize_for_link_cmp(ai_cell) == normalize_for_link_cmp(suggestion);
                        if scroll_ui.selectable_label(is_selected, suggestion).clicked() {
                            *ai_cell = suggestion.clone();
                            changed = true;
                            scroll_ui.memory_mut(|mem| mem.close_popup());
                        }
                    }
                }
            });
    });
    
    changed
}
