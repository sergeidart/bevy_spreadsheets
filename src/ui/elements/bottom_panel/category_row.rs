// src/ui/elements/bottom_panel/category_row.rs
use bevy_egui::egui;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ui_handlers::{category_handlers, sheet_handlers};
use crate::ui::elements::editor::state::EditorWindowState;

/// Render the first bottom row: Category dropdown and controls
pub fn show_category_picker<'a, 'w>(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
    event_writers: &mut super::SheetManagementEventWriters<'a, 'w>,
) {
    let line_h = ui.text_style_height(&egui::TextStyle::Body) + ui.style().spacing.item_spacing.y;
    let row_size = egui::Vec2::new(ui.available_width(), line_h + 6.0);
    let categories = registry.get_categories();
    
    let previous_selected_category = state.selected_category.clone();
    let mut drop_consumed = false;
    
    ui.allocate_ui_with_layout(row_size, egui::Layout::left_to_right(egui::Align::Min), |row| {
        // Category selector button and popup
        super::dropdowns::render_category_selector(
            row,
            state,
            registry,
            &categories,
            event_writers,
            &mut drop_consumed,
        );


        // Hide / Extend button
        row.add_space(6.0);
        let expanded = state.category_picker_expanded;
        let toggle_label = if expanded { "<" } else { ">" };
        if row
            .button(toggle_label)
            .on_hover_text(if expanded { "Shrink category row" } else { "Expand category row" })
            .clicked()
        {
            category_handlers::handle_category_picker_toggle(state);
        }

        // Categories list (tabs) when expanded
        if state.category_picker_expanded {
            row.add_space(8.0);
            let avail_w = row.available_width();
            row.allocate_ui_with_layout(
                egui::Vec2::new(avail_w, line_h + 6.0),
                egui::Layout::left_to_right(egui::Align::Min),
                |inner| {
                    render_category_tabs(
                        inner,
                        state,
                        registry,
                        &categories,
                        line_h,
                        event_writers,
                        &mut drop_consumed,
                    );
                },
            );
        }
        
        // Right side buttons
        row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r| {
            r.add_space(12.0);
            let label = if state.ai_output_panel_visible { "Close" } else { "Log" };
            if r
                .button(label)
                .on_hover_text("Open/close the Log panel")
                .clicked()
            {
                category_handlers::handle_ai_output_panel_toggle(state);
            }
            
            if r
                .button("+ DB")
                .on_hover_text("Create a new database file")
                .clicked()
            {
                category_handlers::handle_new_category_request(state);
            }
        });
    });
    
    // Clear drag state if released outside drop target
    let released_primary = ui.input(|i| i.pointer.primary_released());
    if released_primary && state.dragged_sheet.is_some() && !drop_consumed {
        sheet_handlers::clear_drag_state(state);
        ui.ctx().set_dragged_id(egui::Id::NULL);
    }
    
    if state.selected_category != previous_selected_category {
        // Clear lineage cache when switching categories/databases
        state.parent_lineage_cache.clear();
        
        if state.selected_sheet_name.is_none() {
            state.reset_interaction_modes_and_selections();
        }
        state.force_filter_recalculation = true;
    }
}

/// Render category tabs (horizontal scrollable list)
fn render_category_tabs(
    inner: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    categories: &[Option<String>],
    line_h: f32,
    event_writers: &mut super::SheetManagementEventWriters,
    drop_consumed: &mut bool,
) {
    egui::ScrollArea::horizontal()
        .id_salt("category_tabs_list")
        .auto_shrink([false, false])
        .max_height(line_h + 6.0)
        .min_scrolled_height(0.0)
        .show(inner, |tabs_ui| {
            let primary_released_tabs = tabs_ui.ctx().input(|i| i.pointer.primary_released());
            let pointer_pos_tabs = tabs_ui.ctx().input(|i| i.pointer.hover_pos());
            
            tabs_ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui_th| {
                // Category tabs
                for cat_opt in categories.iter() {
                    if let Some(cat) = cat_opt {
                        let is_sel = state.selected_category.as_deref() == Some(cat.as_str());
                        let disp: String = cat.chars().take(32).collect();
                        let resp = ui_th.selectable_label(is_sel, disp).on_hover_text(cat);
                        // Right-click context menu on category tab
                        resp.context_menu(|menu_ui| {
                            if menu_ui.button("✏ Rename Category").clicked() {
                                crate::sheets::systems::ui_handlers::category_handlers::handle_rename_category_request(state);
                                menu_ui.close_menu();
                            }
                            if menu_ui.button("🗑 Delete Category").clicked() {
                                crate::sheets::systems::ui_handlers::category_handlers::handle_delete_category_request(state);
                                menu_ui.close_menu();
                            }
                        });
                        
                        super::drop_visuals::render_drop_target_highlight(
                            ui_th,
                            &resp,
                            state,
                            registry,
                            &Some(cat.clone()),
                            pointer_pos_tabs,
                        );
                        
                        if let Some(pos) = pointer_pos_tabs {
                            if resp.rect.contains(pos) {
                                if category_handlers::handle_drop_on_target(
                                    state,
                                    registry,
                                    Some(cat.clone()),
                                    event_writers.move_sheet_to_category,
                                    true,
                                    primary_released_tabs,
                                ) {
                                    *drop_consumed = true;
                                    ui_th.ctx().set_dragged_id(egui::Id::NULL);
                                }
                            }
                        }
                        
                        if resp.clicked() && !is_sel {
                            category_handlers::handle_category_selection(state, Some(cat.clone()), registry);
                        }
                    }
                }
            });
        });
}
