// src/ui/elements/bottom_panel/popups.rs
use bevy_egui::egui;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::database::daemon_client::DaemonClient;
use crate::sheets::systems::ui_handlers::{category_handlers, sheet_handlers, ui_cache};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::sheets::systems::io::metadata_persistence;

/// Render category popup with filter and drop handling
pub fn render_category_popup(
    popup_ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    categories: &[Option<String>],
    filter_key: &str,
    event_writers: &mut super::SheetManagementEventWriters,
    drop_consumed: &mut bool,
) {
    let primary_released_popup = popup_ui.ctx().input(|i| i.pointer.primary_released());
    let mut filter_text = ui_cache::get_filter_text(popup_ui, filter_key);
    
    // Calculate popup width
    let category_names: Vec<&str> = categories
        .iter()
        .filter_map(|o| o.as_ref().map(|s| s.as_str()))
        .collect();
    let popup_min_width = ui_cache::calc_popup_width(&category_names, 8.0);
    popup_ui.set_min_width(popup_min_width);

    // Filter input
    popup_ui.horizontal(|ui_h| {
        ui_h.label("Filter:");
        let avail = ui_h.available_width();
        let width = (28.0_f32 * 8.0).min(avail).min(popup_min_width - 40.0);
        let resp = ui_h.add(
            egui::TextEdit::singleline(&mut filter_text)
                .desired_width(width)
                .hint_text("type to filter categories"),
        );
        if resp.changed() {
            ui_cache::save_filter_text(ui_h, filter_key, filter_text.clone());
        }
        if ui_h.small_button("x").clicked() {
            ui_cache::clear_filter_text(ui_h, filter_key);
            filter_text.clear();
        }
    });

    let pointer_pos_popup = popup_ui.ctx().input(|i| i.pointer.hover_pos());

    // Category entries
    for cat_opt in categories.iter() {
        if let Some(cat_name) = cat_opt {
            if !ui_cache::matches_filter(cat_name, &filter_text) {
                continue;
            }
            
            let is_selected_cat = state.selected_category.as_deref() == Some(cat_name.as_str());
            let display_name: String = cat_name.chars().take(32).collect();
            let cat_resp = popup_ui.selectable_label(is_selected_cat, display_name).on_hover_text(cat_name);
            
            super::drop_visuals::render_drop_target_highlight(
                popup_ui,
                &cat_resp,
                state,
                registry,
                &Some(cat_name.clone()),
                pointer_pos_popup,
            );
            
            if handle_category_drop(
                popup_ui,
                &cat_resp,
                state,
                registry,
                Some(cat_name.clone()),
                event_writers,
                primary_released_popup,
                pointer_pos_popup,
                drop_consumed,
            ) {
                continue;
            }
            
            if cat_resp.clicked() && !is_selected_cat {
                category_handlers::handle_category_selection(state, Some(cat_name.clone()));
                popup_ui.memory_mut(|mem| mem.close_popup());
            }
        }
    }
}

/// Handle drop on category item in popup
fn handle_category_drop(
    ui: &mut egui::Ui,
    resp: &egui::Response,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    target_category: Option<String>,
    event_writers: &mut super::SheetManagementEventWriters,
    primary_released: bool,
    pointer_pos: Option<egui::Pos2>,
    drop_consumed: &mut bool,
) -> bool {
    if primary_released {
        if let Some(pos) = pointer_pos {
            if resp.rect.contains(pos) {
                if category_handlers::handle_drop_on_target(
                    state,
                    registry,
                    target_category,
                    event_writers.move_sheet_to_category,
                    true,
                    true,
                ) {
                    ui.memory_mut(|mem| mem.close_popup());
                    *drop_consumed = true;
                    ui.ctx().set_dragged_id(egui::Id::NULL);
                    return true;
                } else {
                    ui.ctx().set_dragged_id(egui::Id::NULL);
                }
            }
        }
    }
    false
}

/// Render sheet popup with filter
pub fn render_sheet_popup(
    popup_ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    sheets_in_category: &[String],
    filter_key: &str,
) {
    let mut filter_text = ui_cache::get_filter_text(popup_ui, filter_key);
    let popup_min_width = ui_cache::calc_popup_width(sheets_in_category, 10.0);
    popup_ui.set_min_width(popup_min_width);

    popup_ui.horizontal(|ui_h| {
        ui_h.label("Filter:");
        let avail = ui_h.available_width();
        let width = (28.0_f32 * 10.0).min(avail).min(popup_min_width * 0.95);
        let resp = ui_h.add(
            egui::TextEdit::singleline(&mut filter_text)
                .desired_width(width)
                .hint_text("type to filter sheets"),
        );
        if resp.changed() {
            ui_cache::save_filter_text(ui_h, filter_key, filter_text.clone());
        }
        if ui_h.small_button("x").clicked() {
            ui_cache::clear_filter_text(ui_h, filter_key);
        }
    });

    popup_ui.selectable_value(&mut state.selected_sheet_name, None, "--Select--");
    
    for name in sheets_in_category.iter().filter(|n| ui_cache::matches_filter(n, &filter_text)) {
        let truncated: String = name.chars().take(32).collect();
        if popup_ui
            .selectable_label(
                state.selected_sheet_name.as_deref() == Some(name.as_str()),
                truncated,
            )
            .on_hover_text(name)
            .clicked()
        {
            sheet_handlers::handle_sheet_selection(state, Some(name.clone()));
            popup_ui.memory_mut(|mem| mem.close_popup());
        }
    }
}

/// Handle sheet context menu (for hidden toggle)
pub fn handle_sheet_context_menu(
    ctx_menu: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
    sheet_name: &str,
    daemon_client: &DaemonClient,
) {
    let mut to_save: Option<crate::sheets::definitions::SheetMetadata> = None;
    let selected_category = state.selected_category.clone();
    
    {
        if let Some(data) = registry.get_sheet_mut(&state.selected_category, sheet_name) {
            if let Some(meta) = &mut data.metadata {
                let mut hidden = meta.hidden;
                if ctx_menu.checkbox(&mut hidden, "Hidden").changed() {
                    meta.hidden = hidden;
                    to_save = Some(meta.clone());
                }
            } else {
                ctx_menu.label("No metadata available");
            }
        } else {
            ctx_menu.label("Sheet not found");
        }
    }
    
    if let Some(meta_to_save) = to_save {
        metadata_persistence::save_sheet_metadata(registry, &meta_to_save, selected_category, daemon_client);
        state.force_filter_recalculation = true;
        ctx_menu.close_menu();
    }
}
