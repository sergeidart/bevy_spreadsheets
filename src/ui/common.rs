// src/ui/common.rs
use bevy::prelude::*;
use bevy_egui::egui;

// Use ColumnDataType definition from the sheets module
use crate::sheets::definitions::ColumnDataType;

/// Renders UI for a single cell (String) based on expected ColumnDataType.
/// Handles parsing the string, showing the widget, and writing back to the string.
/// Returns true if the value was changed by the user interaction.
pub fn ui_for_cell(
    ui: &mut egui::Ui,
    id: egui::Id,
    cell_string: &mut String, // Takes mutable ref to update
    col_type: ColumnDataType,
) -> bool {
    let mut changed = false;
    let original_string = cell_string.clone();

    // --- Parsing Logic ---
    let mut error_indicator = false;
    let mut temp_bool: bool = false;
    let mut temp_opt_bool: Option<bool> = None;
    let mut temp_u8: u8 = 0;
    let mut temp_opt_u8: Option<u8> = None;
    let mut temp_u16: u16 = 0;
    let mut temp_opt_u16: Option<u16> = None;
    let mut temp_u32: u32 = 0;
    let mut temp_opt_u32: Option<u32> = None;
    // Added missing types based on ColumnDataType enum
    let mut temp_u64: u64 = 0;
    let mut temp_opt_u64: Option<u64> = None;
    let mut temp_i8: i8 = 0;
    let mut temp_opt_i8: Option<i8> = None;
    let mut temp_i16: i16 = 0;
    let mut temp_opt_i16: Option<i16> = None;
    let mut temp_i32: i32 = 0;
    let mut temp_opt_i32: Option<i32> = None;
    let mut temp_i64: i64 = 0;
    let mut temp_opt_i64: Option<i64> = None;
    let mut temp_f32: f32 = 0.0;
    let mut temp_opt_f32: Option<f32> = None;
    let mut temp_f64: f64 = 0.0;
    let mut temp_opt_f64: Option<f64> = None;


    // Comprehensive parsing logic block
    match col_type {
        ColumnDataType::String | ColumnDataType::OptionString => {} // No parsing needed
        ColumnDataType::Bool => match original_string.to_lowercase().as_str() { "true" | "1" => temp_bool = true, "false" | "0" => temp_bool = false, _ => if !original_string.is_empty() { error_indicator = true; } },
        ColumnDataType::OptionBool => match original_string.to_lowercase().as_str() { "true" | "1" => temp_opt_bool = Some(true), "false" | "0" => temp_opt_bool = Some(false), "" => temp_opt_bool = None, _ => { temp_opt_bool = None; if !original_string.is_empty() {error_indicator = true;} } },
        ColumnDataType::U8 => match original_string.parse::<u8>() { Ok(val) => temp_u8 = val, Err(_) => if !original_string.is_empty() { error_indicator = true; }},
        ColumnDataType::OptionU8 => match original_string.parse::<u8>() { Ok(val) => temp_opt_u8 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_u8 = None; error_indicator = true;} else { temp_opt_u8 = None; }},
        ColumnDataType::U16 => match original_string.parse::<u16>() { Ok(val) => temp_u16 = val, Err(_) => if !original_string.is_empty() { error_indicator = true; }},
        ColumnDataType::OptionU16 => match original_string.parse::<u16>() { Ok(val) => temp_opt_u16 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_u16 = None; error_indicator = true;} else { temp_opt_u16 = None; }},
        ColumnDataType::U32 => match original_string.parse::<u32>() { Ok(val) => temp_u32 = val, Err(_) => if !original_string.is_empty() { error_indicator = true; }},
        ColumnDataType::OptionU32 => match original_string.parse::<u32>() { Ok(val) => temp_opt_u32 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_u32 = None; error_indicator = true;} else { temp_opt_u32 = None; }},
        ColumnDataType::U64 => match original_string.parse::<u64>() { Ok(val) => temp_u64 = val, Err(_) => if !original_string.is_empty() { error_indicator = true; }},
        ColumnDataType::OptionU64 => match original_string.parse::<u64>() { Ok(val) => temp_opt_u64 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_u64 = None; error_indicator = true;} else { temp_opt_u64 = None; }},
        ColumnDataType::I8 => match original_string.parse::<i8>() { Ok(val) => temp_i8 = val, Err(_) => if !original_string.is_empty() { error_indicator = true; }},
        ColumnDataType::OptionI8 => match original_string.parse::<i8>() { Ok(val) => temp_opt_i8 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_i8 = None; error_indicator = true;} else { temp_opt_i8 = None; }},
        ColumnDataType::I16 => match original_string.parse::<i16>() { Ok(val) => temp_i16 = val, Err(_) => if !original_string.is_empty() { error_indicator = true; }},
        ColumnDataType::OptionI16 => match original_string.parse::<i16>() { Ok(val) => temp_opt_i16 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_i16 = None; error_indicator = true;} else { temp_opt_i16 = None; }},
        ColumnDataType::I32 => match original_string.parse::<i32>() { Ok(val) => temp_i32 = val, Err(_) => if !original_string.is_empty() { error_indicator = true; }},
        ColumnDataType::OptionI32 => match original_string.parse::<i32>() { Ok(val) => temp_opt_i32 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_i32 = None; error_indicator = true;} else { temp_opt_i32 = None; }},
        ColumnDataType::I64 => match original_string.parse::<i64>() { Ok(val) => temp_i64 = val, Err(_) => if !original_string.is_empty() { error_indicator = true; }},
        ColumnDataType::OptionI64 => match original_string.parse::<i64>() { Ok(val) => temp_opt_i64 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_i64 = None; error_indicator = true;} else { temp_opt_i64 = None; }},
        ColumnDataType::F32 => match original_string.parse::<f32>() { Ok(val) => temp_f32 = val, Err(_) => if !original_string.is_empty() { error_indicator = true; }},
        ColumnDataType::OptionF32 => match original_string.parse::<f32>() { Ok(val) => temp_opt_f32 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_f32 = None; error_indicator = true;} else { temp_opt_f32 = None; }},
        ColumnDataType::F64 => match original_string.parse::<f64>() { Ok(val) => temp_f64 = val, Err(_) => if !original_string.is_empty() { error_indicator = true; }},
        ColumnDataType::OptionF64 => match original_string.parse::<f64>() { Ok(val) => temp_opt_f64 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_f64 = None; error_indicator = true;} else { temp_opt_f64 = None; }},
    }


    // --- Drawing Logic ---
    let background_color = if error_indicator { egui::Color32::from_rgb(60, 10, 10) } else { ui.style().visuals.widgets.inactive.bg_fill };
    let frame = egui::Frame::none()
        .inner_margin(ui.style().spacing.item_spacing * 0.5)
        .fill(background_color);

    let resp = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let widget_changed: bool;

        match col_type {
            ColumnDataType::String | ColumnDataType::OptionString => {
                 // Use TextEdit for strings, allowing multiline if needed, but singleline is common for cells
                 widget_changed = ui.add(egui::TextEdit::singleline(cell_string)).changed();
            }
            ColumnDataType::Bool => {
                let response = ui.add(egui::Checkbox::new(&mut temp_bool, ""));
                if response.changed() { *cell_string = temp_bool.to_string(); widget_changed = true; } else { widget_changed = false; }
            }
            ColumnDataType::OptionBool => {
                if ui_option_bool(ui, id.with("opt_bool"), &mut temp_opt_bool) { *cell_string = temp_opt_bool.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; }
            }
            // Use DragValue for numeric types
            ColumnDataType::U8 => { let r = ui.add(egui::DragValue::new(&mut temp_u8)); if r.changed() { *cell_string = temp_u8.to_string(); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::OptionU8 => { if ui_option_numerical(ui, id.with("opt_u8"), &mut temp_opt_u8) { *cell_string = temp_opt_u8.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::U16 => { let r = ui.add(egui::DragValue::new(&mut temp_u16)); if r.changed() { *cell_string = temp_u16.to_string(); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::OptionU16 => { if ui_option_numerical(ui, id.with("opt_u16"), &mut temp_opt_u16) { *cell_string = temp_opt_u16.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::U32 => { let r = ui.add(egui::DragValue::new(&mut temp_u32)); if r.changed() { *cell_string = temp_u32.to_string(); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::OptionU32 => { if ui_option_numerical(ui, id.with("opt_u32"), &mut temp_opt_u32) { *cell_string = temp_opt_u32.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::U64 => { let r = ui.add(egui::DragValue::new(&mut temp_u64)); if r.changed() { *cell_string = temp_u64.to_string(); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::OptionU64 => { if ui_option_numerical(ui, id.with("opt_u64"), &mut temp_opt_u64) { *cell_string = temp_opt_u64.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::I8 => { let r = ui.add(egui::DragValue::new(&mut temp_i8)); if r.changed() { *cell_string = temp_i8.to_string(); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::OptionI8 => { if ui_option_numerical(ui, id.with("opt_i8"), &mut temp_opt_i8) { *cell_string = temp_opt_i8.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::I16 => { let r = ui.add(egui::DragValue::new(&mut temp_i16)); if r.changed() { *cell_string = temp_i16.to_string(); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::OptionI16 => { if ui_option_numerical(ui, id.with("opt_i16"), &mut temp_opt_i16) { *cell_string = temp_opt_i16.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::I32 => { let r = ui.add(egui::DragValue::new(&mut temp_i32)); if r.changed() { *cell_string = temp_i32.to_string(); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::OptionI32 => { if ui_option_numerical(ui, id.with("opt_i32"), &mut temp_opt_i32) { *cell_string = temp_opt_i32.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::I64 => { let r = ui.add(egui::DragValue::new(&mut temp_i64)); if r.changed() { *cell_string = temp_i64.to_string(); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::OptionI64 => { if ui_option_numerical(ui, id.with("opt_i64"), &mut temp_opt_i64) { *cell_string = temp_opt_i64.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::F32 => { let r = ui.add(egui::DragValue::new(&mut temp_f32).speed(0.1)); if r.changed() { *cell_string = temp_f32.to_string(); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::OptionF32 => { if ui_option_numerical(ui, id.with("opt_f32"), &mut temp_opt_f32) { *cell_string = temp_opt_f32.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::F64 => { let r = ui.add(egui::DragValue::new(&mut temp_f64).speed(0.1)); if r.changed() { *cell_string = temp_f64.to_string(); widget_changed = true; } else { widget_changed = false; } },
            ColumnDataType::OptionF64 => { if ui_option_numerical(ui, id.with("opt_f64"), &mut temp_opt_f64) { *cell_string = temp_opt_f64.map_or_else(String::new, |v| v.to_string()); widget_changed = true; } else { widget_changed = false; } },
        }
        changed = widget_changed;
    }).response;

    // --- Hover Text Logic ---
    if error_indicator {
        resp.on_hover_text(format!("Parse Error! Input: '{}'", original_string));
    } else if *cell_string != original_string && !(col_type == ColumnDataType::String || col_type == ColumnDataType::OptionString) {
         // Show original value on hover if it was parsed/changed (but not for raw strings)
         resp.on_hover_text(format!("Original: '{}'", original_string));
    }

    changed
}


// --- Helper UIs for Option<bool> and Option<Numeric> ---

/// Helper UI for Option<bool> types. Returns true if the value was changed.
pub fn ui_option_bool(
    ui: &mut egui::Ui,
    _base_id: egui::Id, // id might be needed if state is stored elsewhere
    opt_value: &mut Option<bool>
) -> bool {
     let mut changed = false;
     // Determine current state for checkboxes
     let mut is_some = opt_value.is_some();
     // Provide a default for the inner checkbox if Option is None
     let mut current_val = opt_value.unwrap_or(false);

     ui.horizontal(|ui| {
         // Checkbox to toggle Some/None
         let is_some_response = ui.add(egui::Checkbox::new(&mut is_some, ""));
         if is_some_response.changed() {
             // If toggled, update the Option state
             *opt_value = if is_some { Some(current_val) } else { None };
             changed = true;
         }
         // Inner checkbox, enabled only if the Option is Some
         ui.add_enabled_ui(is_some, |ui| {
              let current_val_response = ui.add(egui::Checkbox::new(&mut current_val, ""));
              if current_val_response.changed() {
                  // If the inner value changes and we are in Some state, update the Option
                  if is_some {
                      *opt_value = Some(current_val);
                      changed = true;
                  }
              }
         });
     });
     changed
}

/// Generic helper for Option<Numeric> types using DragValue. Returns true if the value was changed.
pub fn ui_option_numerical<T>(
    ui: &mut egui::Ui,
    _base_id: egui::Id, // id might be needed if state is stored elsewhere
    opt_value: &mut Option<T>
) -> bool
where
    T: egui::emath::Numeric + Default + Clone + Send + Sync + 'static + std::fmt::Display,
{
    let mut changed = false;
    // Determine current state
    let mut is_some = opt_value.is_some();
    // Provide a default value for the DragValue if Option is None
    let mut temp_val = opt_value.clone().unwrap_or_default();

    ui.horizontal(|ui| {
        // Checkbox to toggle Some/None
        let is_some_response = ui.add(egui::Checkbox::new(&mut is_some, ""));
        if is_some_response.changed() {
            // If toggled, update the Option state
            *opt_value = if is_some { Some(temp_val.clone()) } else { None };
            changed = true;
        }
         // Inner DragValue, enabled only if the Option is Some
         ui.add_enabled_ui(is_some, |ui|{
             // Adjust drag speed for floats
             let mut drag_speed = 1.0;
             if std::any::TypeId::of::<T>() == std::any::TypeId::of::<f32>() ||
                std::any::TypeId::of::<T>() == std::any::TypeId::of::<f64>() {
                 drag_speed = 0.1;
             }
             // Use DragValue for the numeric input
             let drag_resp = ui.add(egui::DragValue::new(&mut temp_val).speed(drag_speed));

             if drag_resp.changed() {
                  // If the inner value changes and we are in Some state, update the Option
                  if is_some {
                     *opt_value = Some(temp_val); // temp_val was updated by DragValue
                     changed = true;
                  }
             }
        });
    });
    changed
}