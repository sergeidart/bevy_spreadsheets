// src/ui/widgets/option_widgets.rs
use bevy_egui::egui::{self};

/// Adds a numeric DragValue widget with dark theme styling.
///
/// This helper provides consistent styling for numeric input across all cell types.
/// The dark background color matches the cell background.
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `value` - Mutable reference to the numeric value
/// * `speed` - Drag speed for the widget
///
/// # Returns
/// The response from the DragValue widget
pub(crate) fn add_numeric_drag_value<T>(
    ui: &mut egui::Ui,
    value: &mut T,
    speed: f64,
) -> egui::Response
where
    T: egui::emath::Numeric,
{
    let size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
    ui.scope(|ui_num| {
        let dark = egui::Color32::from_rgb(45, 45, 45);
        let visuals = &mut ui_num.style_mut().visuals;
        visuals.widgets.inactive.weak_bg_fill = dark;
        visuals.widgets.inactive.bg_fill = dark;
        visuals.widgets.hovered.weak_bg_fill = dark;
        visuals.widgets.hovered.bg_fill = dark;
        visuals.widgets.active.weak_bg_fill = dark;
        visuals.widgets.active.bg_fill = dark;
        ui_num.add_sized(size, egui::DragValue::new(value).speed(speed))
    })
    .inner
}

/// Adds a centered checkbox widget.
///
/// This helper provides consistent checkbox layout with vertical centering,
/// matching the alignment of other cell widgets.
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `value` - Mutable reference to the boolean value
///
/// # Returns
/// The response from the checkbox widget
pub(crate) fn add_centered_checkbox(
    ui: &mut egui::Ui,
    value: &mut bool,
) -> egui::Response {
    let mut resp_opt: Option<egui::Response> = None;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |row_ui| {
            row_ui.vertical_centered(|vc| {
                let r = vc.add(egui::Checkbox::new(value, ""));
                resp_opt = Some(r);
            });
        },
    );
    resp_opt.expect("checkbox response should be set")
}