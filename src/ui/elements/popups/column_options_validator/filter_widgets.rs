// src/ui/elements/popups/column_options_validator/filter_widgets.rs
// Reusable filtered popup and filter box widgets

use crate::ui::validation::normalize_for_link_cmp;
use bevy_egui::egui;

/// Renders a filtered popup selector (generic helper)
/// 
/// This creates a button that opens a popup with a searchable list of items.
/// Features:
/// - Type-to-filter search
/// - Memory-persisted filter text
/// - Clear button
/// - Responsive width (120-900px)
/// - Exclude specific items
pub fn show_filtered_popup_selector(
    ui: &mut egui::Ui,
    combo_id: &str,
    display_text: &str,
    items: &[String],
    selected: &mut Option<String>,
    exclude_item: Option<&str>,
    hint_text: &str,
) {
    let filter_key = format!("{}_filter", combo_id);
    let btn = ui.button(display_text);
    let popup_id = egui::Id::new(combo_id);
    
    if btn.clicked() {
        ui.ctx().memory_mut(|mem| mem.open_popup(popup_id));
    }
    
    egui::containers::popup::popup_below_widget(
        ui,
        popup_id,
        &btn,
        egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
        |popup_ui| {
            let mut filter_text = popup_ui.memory(|mem| {
                mem.data
                    .get_temp::<String>(filter_key.clone().into())
                    .unwrap_or_default()
            });
            
            render_filter_box(popup_ui, &mut filter_text, &filter_key, items, hint_text);
            let current = filter_text.to_lowercase();
            
            egui::ScrollArea::vertical().max_height(300.0).show(
                popup_ui,
                |list_ui| {
                    if list_ui
                        .selectable_label(selected.is_none(), "--Select--")
                        .clicked()
                    {
                        *selected = None;
                        list_ui.memory_mut(|mem| mem.close_popup());
                    }
                    for name in items.iter() {
                        if let Some(exclude) = exclude_item {
                            if name == exclude {
                                continue;
                            }
                        }
                        if !current.is_empty()
                            && !normalize_for_link_cmp(name)
                                .contains(&normalize_for_link_cmp(&current))
                        {
                            continue;
                        }
                        if list_ui
                            .selectable_label(
                                selected.as_deref() == Some(name.as_str()),
                                name,
                            )
                            .clicked()
                        {
                            *selected = Some(name.clone());
                            list_ui.memory_mut(|mem| mem.close_popup());
                        }
                    }
                },
            );
        },
    );
}

/// Renders the filter text box with clear button
/// 
/// Features:
/// - Auto-sized based on item length (120-900px)
/// - Memory-persisted filter text
/// - Clear button (x)
/// - Custom hint text
pub fn render_filter_box(
    popup_ui: &mut egui::Ui,
    filter_text: &mut String,
    filter_key: &str,
    items: &[String],
    hint_text: &str,
) {
    let char_w = 8.0_f32;
    let max_name_len = items.iter().map(|s| s.len()).max().unwrap_or(12);
    let padding = 24.0_f32;
    let mut popup_min_width = (max_name_len as f32) * char_w + padding;
    if popup_min_width < 120.0 {
        popup_min_width = 120.0;
    }
    if popup_min_width > 900.0 {
        popup_min_width = 900.0;
    }
    popup_ui.set_min_width(popup_min_width);
    
    popup_ui.horizontal(|ui_h| {
        ui_h.label("Filter:");
        let avail = ui_h.available_width();
        let default_chars = 28usize;
        let desired = (default_chars as f32) * char_w;
        let width = desired.min(avail).min(popup_min_width - 40.0);
        let resp = ui_h.add(
            egui::TextEdit::singleline(filter_text)
                .desired_width(width)
                .hint_text(hint_text),
        );
        if resp.changed() {
            ui_h.memory_mut(|mem| {
                mem.data.insert_temp(
                    filter_key.to_string().into(),
                    filter_text.clone(),
                )
            });
        }
        if ui_h.small_button("x").clicked() {
            filter_text.clear();
            ui_h.memory_mut(|mem| {
                mem.data.insert_temp(
                    filter_key.to_string().into(),
                    filter_text.clone(),
                )
            });
        }
    });
}
