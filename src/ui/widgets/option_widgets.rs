// src/ui/widgets/option_widgets.rs
use bevy_egui::egui::{self};
 // Keep import for potential future use maybe? Not directly needed now.

/// Helper UI for Option<bool> types. Returns (changed, response).
pub(crate) fn ui_option_bool(
    ui: &mut egui::Ui,
    _base_id: egui::Id, // often unused, kept for signature consistency
    opt_value: &mut Option<bool>,
) -> (bool, egui::Response) {
    let mut changed = false;
    // Use pattern matching for clarity
    let (mut is_some, mut current_val) = match opt_value {
        Some(val) => (true, *val),
        None => (false, false), // Default to false if None
    };

    let inner_response = ui.horizontal(|ui| {
        // Checkbox to toggle Some/None state
        let is_some_response = ui.add(egui::Checkbox::without_text(&mut is_some));
        if is_some_response.changed() {
            *opt_value = if is_some { Some(current_val) } else { None };
            changed = true;
        }
        // Checkbox for the actual bool value, enabled only if is_some is true
        ui.add_enabled_ui(is_some, |ui| {
            let current_val_response = ui.add(egui::Checkbox::without_text(&mut current_val));
            // Update the Option only if the inner checkbox changes *and* we are in the Some state
            if current_val_response.changed() && is_some {
                *opt_value = Some(current_val);
                changed = true;
            }
        });
    });
    (changed, inner_response.response)
}

/// Generic helper for Option<Numeric> types using DragValue. Returns (changed, response).
pub(crate) fn ui_option_numerical<T>(
    ui: &mut egui::Ui,
    _base_id: egui::Id, // often unused, kept for signature consistency
    opt_value: &mut Option<T>,
) -> (bool, egui::Response)
where
    T: egui::emath::Numeric + Default + Clone + Send + Sync + 'static + std::fmt::Display,
{
    let mut changed = false;
    // Use pattern matching for clarity
    let (mut is_some, mut temp_val) = match opt_value {
        Some(val) => (true, val.clone()), // Clone the value if Some
        None => (false, T::default()),    // Use default if None
    };

    let inner_response = ui.horizontal(|ui| {
        // Checkbox to toggle Some/None state
        let is_some_response = ui.add(egui::Checkbox::without_text(&mut is_some));
        if is_some_response.changed() {
            *opt_value = if is_some { Some(temp_val.clone()) } else { None };
            changed = true;
        }
        // DragValue for the numeric value, enabled only if is_some is true
        ui.add_enabled_ui(is_some, |ui| {
            // Determine drag speed based on type (float vs integer)
            let mut drag_speed = 1.0;
            if std::any::TypeId::of::<T>() == std::any::TypeId::of::<f32>()
                || std::any::TypeId::of::<T>() == std::any::TypeId::of::<f64>()
            {
                drag_speed = 0.1;
            }
            // Add the DragValue widget
            let drag_resp = ui.add(egui::DragValue::new(&mut temp_val).speed(drag_speed));
            // Update the Option only if DragValue changes *and* we are in the Some state
            if drag_resp.changed() && is_some {
                *opt_value = Some(temp_val.clone()); // Clone the changed value back
                changed = true;
            }
        });
    });
    (changed, inner_response.response)
}

/// Returns the appropriate drag speed for a numeric column data type.
///
/// - f64: 0.1 for fine-grained control
/// - i64: 1.0 for integer increments
pub(crate) fn get_numeric_drag_speed<T>() -> f64
where
    T: 'static,
{
    if std::any::TypeId::of::<T>() == std::any::TypeId::of::<f32>()
        || std::any::TypeId::of::<T>() == std::any::TypeId::of::<f64>()
    {
        0.1
    } else {
        1.0
    }
}

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