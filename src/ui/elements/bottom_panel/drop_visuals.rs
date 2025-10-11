// src/ui/elements/bottom_panel/drop_visuals.rs
use bevy_egui::egui;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ui_handlers::category_handlers;
use crate::ui::elements::editor::state::EditorWindowState;

/// Render visual highlight for drop target when dragging a sheet
pub fn render_drop_target_highlight(
    ui: &mut egui::Ui,
    resp: &egui::Response,
    state: &EditorWindowState,
    registry: &SheetRegistry,
    target_category: &Option<String>,
    pointer_pos: Option<egui::Pos2>,
) {
    if let Some((from_cat, ref sheet_name)) = state.dragged_sheet.as_ref() {
        if let Some(pos) = pointer_pos {
            if resp.rect.contains(pos) {
                let droppable = category_handlers::is_drop_valid(
                    from_cat,
                    target_category,
                    sheet_name,
                    registry,
                );
                
                let (fill, stroke, cursor) = if droppable {
                    (
                        egui::Color32::from_rgba_premultiplied(60, 200, 60, 40),
                        egui::Stroke::new(3.0, egui::Color32::from_rgb(60, 200, 60)),
                        None,
                    )
                } else {
                    (
                        egui::Color32::from_rgba_premultiplied(200, 60, 60, 40),
                        egui::Stroke::new(3.0, egui::Color32::from_rgb(200, 60, 60)),
                        Some(egui::CursorIcon::NotAllowed),
                    )
                };
                
                if let Some(icon) = cursor {
                    ui.output_mut(|o| o.cursor_icon = icon);
                }
                let painter = ui.ctx().debug_painter();
                painter.rect(
                    resp.rect,
                    egui::CornerRadius::same(4),
                    fill,
                    stroke,
                    egui::StrokeKind::Outside,
                );
            }
        }
    }
}

/// Render drag preview that follows the cursor when dragging a sheet
pub fn render_sheet_drag_preview(
    ui: &egui::Context,
    item_id: egui::Id,
    sheet_name: &str,
    resp_rect: egui::Rect,
) {
    if ui.is_being_dragged(item_id) {
        egui::Area::new(item_id.with("drag_preview"))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .current_pos(ui.input(|i| {
                i.pointer.hover_pos().unwrap_or(resp_rect.center())
            }))
            .movable(false)
            .show(ui, |ui_preview| {
                let frame = egui::Frame::popup(ui_preview.style());
                frame.show(ui_preview, |fui| {
                    fui.label(format!("Moving: {}", sheet_name));
                });
            });
    }
}
