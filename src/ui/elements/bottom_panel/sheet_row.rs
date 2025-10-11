// src/ui/elements/bottom_panel/sheet_row.rs
use bevy_egui::egui;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ui_handlers::sheet_handlers;
use crate::ui::elements::editor::state::EditorWindowState;

/// Render the second bottom row: Sheet dropdown, controls, tabs, and New Sheet button
pub fn show_sheet_controls<'a, 'w>(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
    _event_writers: &mut super::SheetManagementEventWriters<'a, 'w>,
) {
    let sheets_in_category = registry
        .get_sheet_names_in_category_filtered(&state.selected_category, state.show_hidden_sheets);

    let line_h = ui.text_style_height(&egui::TextStyle::Body) + ui.style().spacing.item_spacing.y;
    let row_size = egui::Vec2::new(ui.available_width(), line_h + 6.0);
    
    ui.allocate_ui_with_layout(
        row_size,
        egui::Layout::right_to_left(egui::Align::Min),
        |ui_r| {
            ui_r.add_space(12.0);
            
            // New Sheet button
            if state.selected_category.is_some() {
                if ui_r
                    .button("➕ New Sheet")
                    .on_hover_text("Create a new table in the current database")
                    .clicked()
                {
                    sheet_handlers::handle_new_sheet_request(state);
                }
            }

            // Left side: sheet dropdown + controls + tabs
            ui_r.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                super::dropdowns::render_sheet_selector(ui, state, registry, &sheets_in_category);
                render_sheet_controls(ui, state);
                render_sheet_tabs(ui, state, registry, &sheets_in_category, line_h);
            });
        },
    );
}

/// Render sheet control buttons (rename, delete, expand)
fn render_sheet_controls(ui: &mut egui::Ui, state: &mut EditorWindowState) {
    let can_manage = sheet_handlers::can_manage_sheet(state);

    if ui
        .add_enabled(can_manage, egui::Button::new("✏"))
        .on_hover_text("Rename sheet")
        .clicked()
    {
        sheet_handlers::handle_rename_sheet_request(state);
    }
    
    if ui
        .add_enabled(can_manage, egui::Button::new("🗑"))
        .on_hover_text("Delete sheet")
        .clicked()
    {
        sheet_handlers::handle_delete_sheet_request(state);
    }

    ui.add_space(6.0);
    let s_expanded = state.sheet_picker_expanded;
    let s_toggle_label = if s_expanded { "<" } else { ">" };
    if ui
        .button(s_toggle_label)
        .on_hover_text(if s_expanded { "Shrink sheet row" } else { "Expand sheet row" })
        .clicked()
    {
        sheet_handlers::handle_sheet_picker_toggle(state);
    }
}

/// Render sheet tabs (horizontal scrollable list)
fn render_sheet_tabs(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
    sheets_in_category: &[String],
    line_h: f32,
) {
    if state.sheet_picker_expanded && !sheets_in_category.is_empty() {
        egui::ScrollArea::horizontal()
            .id_salt("sheet_tabs_list")
            .auto_shrink([false, false])
            .max_height(line_h + 6.0)
            .min_scrolled_height(0.0)
            .show(ui, |ui_tabs| {
                ui_tabs.horizontal(|ui_th| {
                    for name in sheets_in_category.iter() {
                        render_sheet_tab(ui_th, state, registry, name);
                    }
                });
            });
    }
}

/// Render a single sheet tab with drag-and-drop and context menu
fn render_sheet_tab(
    ui_th: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
    name: &str,
) {
    let is_sel = state.selected_sheet_name.as_deref() == Some(name);
    let resp = ui_th.selectable_label(is_sel, name).on_hover_text(name);
    
    // Context menu for hidden toggle
    resp.context_menu(|ctx_menu| {
        super::popups::handle_sheet_context_menu(ctx_menu, state, registry, name);
    });
    
    // Drag-and-drop
    let dnd_id_source = egui::Id::new("sheet_dnd_context").with(&state.selected_category);
    let item_id = dnd_id_source.with(name);
    let interact = resp.interact(egui::Sense::drag());
    
    if interact.drag_started_by(egui::PointerButton::Primary) {
        sheet_handlers::handle_sheet_drag_start(state, name.to_string());
        ui_th.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grabbing);
        ui_th.ctx().set_dragged_id(item_id);
    }
    
    // Drag preview
    super::drop_visuals::render_sheet_drag_preview(ui_th.ctx(), item_id, name, resp.rect);
    
    if resp.clicked() && !is_sel {
        sheet_handlers::handle_sheet_selection(state, Some(name.to_string()));
    }
}
