// src/ui/common.rs
use bevy::prelude::*;
use bevy_egui::egui::{self};
use std::collections::HashMap; // Only HashMap needed if used elsewhere

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::EditorWindowState;
// Import the refactored handler function from the widgets module
use crate::ui::widgets::handle_linked_column_edit; // <-- UPDATED Import

/// Renders interactive UI for editing a single cell based on its validator.
/// Handles parsing and showing the appropriate widget (TextEdit, Checkbox, DragValue, or calls linked editor handler).
/// Returns Some(new_value) if the value was changed, None otherwise.
pub fn edit_cell_widget(
    ui: &mut egui::Ui,
    id: egui::Id,
    current_cell_string: &str, // Takes immutable ref
    validator_opt: &Option<ColumnValidator>,
    registry: &SheetRegistry, // Takes immutable registry
    state: &mut EditorWindowState,
) -> Option<String> {
    // This will store the final value determined by interaction this frame.
    let mut final_new_value: Option<String> = None;
    let original_string = current_cell_string.to_string();

    let basic_type = match validator_opt {
        Some(ColumnValidator::Basic(data_type)) => *data_type,
        // Use String as the interaction type for Linked, actual editing handled by handler
        Some(ColumnValidator::Linked { .. }) => ColumnDataType::String,
        None => ColumnDataType::String,
    };

    // --- Parsing Logic (only for Basic/None validators) ---
    let mut parse_error = false; // Renamed from error_indicator for clarity
    // Initialize temp variables for all types
    let mut temp_bool: bool = false; let mut temp_opt_bool: Option<bool> = None;
    let mut temp_u8: u8 = 0; let mut temp_opt_u8: Option<u8> = None;
    let mut temp_u16: u16 = 0; let mut temp_opt_u16: Option<u16> = None;
    let mut temp_u32: u32 = 0; let mut temp_opt_u32: Option<u32> = None;
    let mut temp_u64: u64 = 0; let mut temp_opt_u64: Option<u64> = None;
    let mut temp_i8: i8 = 0; let mut temp_opt_i8: Option<i8> = None;
    let mut temp_i16: i16 = 0; let mut temp_opt_i16: Option<i16> = None;
    let mut temp_i32: i32 = 0; let mut temp_opt_i32: Option<i32> = None;
    let mut temp_i64: i64 = 0; let mut temp_opt_i64: Option<i64> = None;
    let mut temp_f32: f32 = 0.0; let mut temp_opt_f32: Option<f32> = None;
    let mut temp_f64: f64 = 0.0; let mut temp_opt_f64: Option<f64> = None;

    // Only parse if not a linked column validator
    if !matches!(validator_opt, Some(ColumnValidator::Linked { .. })) {
         if !original_string.is_empty() || matches!(basic_type, ColumnDataType::Bool | ColumnDataType::OptionBool) {
              match basic_type {
                  ColumnDataType::String | ColumnDataType::OptionString => {} // No parsing needed
                  ColumnDataType::Bool => match original_string.to_lowercase().as_str() {
                      "true" | "1" => temp_bool = true,
                      "false" | "0" | "" => temp_bool = false,
                      _ => parse_error = true,
                  },
                  ColumnDataType::OptionBool => match original_string.to_lowercase().as_str() {
                      "true" | "1" => temp_opt_bool = Some(true),
                      "false" | "0" => temp_opt_bool = Some(false),
                      "" => temp_opt_bool = None,
                      _ => { temp_opt_bool = None; parse_error = true; }
                  },
                  ColumnDataType::U8 => match original_string.parse::<u8>() { Ok(val) => temp_u8 = val, Err(_) => if !original_string.is_empty() { parse_error = true; } },
                  ColumnDataType::OptionU8 => match original_string.parse::<u8>() { Ok(val) => temp_opt_u8 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_u8 = None; parse_error = true; } else { temp_opt_u8 = None; }},
                  ColumnDataType::U16 => match original_string.parse::<u16>() { Ok(val) => temp_u16 = val, Err(_) => if !original_string.is_empty() { parse_error = true; } },
                  ColumnDataType::OptionU16 => match original_string.parse::<u16>() { Ok(val) => temp_opt_u16 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_u16 = None; parse_error = true; } else { temp_opt_u16 = None; }},
                  ColumnDataType::U32 => match original_string.parse::<u32>() { Ok(val) => temp_u32 = val, Err(_) => if !original_string.is_empty() { parse_error = true; } },
                  ColumnDataType::OptionU32 => match original_string.parse::<u32>() { Ok(val) => temp_opt_u32 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_u32 = None; parse_error = true; } else { temp_opt_u32 = None; }},
                  ColumnDataType::U64 => match original_string.parse::<u64>() { Ok(val) => temp_u64 = val, Err(_) => if !original_string.is_empty() { parse_error = true; } },
                  ColumnDataType::OptionU64 => match original_string.parse::<u64>() { Ok(val) => temp_opt_u64 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_u64 = None; parse_error = true; } else { temp_opt_u64 = None; }},
                  ColumnDataType::I8 => match original_string.parse::<i8>() { Ok(val) => temp_i8 = val, Err(_) => if !original_string.is_empty() { parse_error = true; } },
                  ColumnDataType::OptionI8 => match original_string.parse::<i8>() { Ok(val) => temp_opt_i8 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_i8 = None; parse_error = true; } else { temp_opt_i8 = None; }},
                  ColumnDataType::I16 => match original_string.parse::<i16>() { Ok(val) => temp_i16 = val, Err(_) => if !original_string.is_empty() { parse_error = true; } },
                  ColumnDataType::OptionI16 => match original_string.parse::<i16>() { Ok(val) => temp_opt_i16 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_i16 = None; parse_error = true; } else { temp_opt_i16 = None; }},
                  ColumnDataType::I32 => match original_string.parse::<i32>() { Ok(val) => temp_i32 = val, Err(_) => if !original_string.is_empty() { parse_error = true; } },
                  ColumnDataType::OptionI32 => match original_string.parse::<i32>() { Ok(val) => temp_opt_i32 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_i32 = None; parse_error = true; } else { temp_opt_i32 = None; }},
                  ColumnDataType::I64 => match original_string.parse::<i64>() { Ok(val) => temp_i64 = val, Err(_) => if !original_string.is_empty() { parse_error = true; } },
                  ColumnDataType::OptionI64 => match original_string.parse::<i64>() { Ok(val) => temp_opt_i64 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_i64 = None; parse_error = true; } else { temp_opt_i64 = None; }},
                  ColumnDataType::F32 => match original_string.parse::<f32>() { Ok(val) => temp_f32 = val, Err(_) => if !original_string.is_empty() { parse_error = true; } },
                  ColumnDataType::OptionF32 => match original_string.parse::<f32>() { Ok(val) => temp_opt_f32 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_f32 = None; parse_error = true; } else { temp_opt_f32 = None; }},
                  ColumnDataType::F64 => match original_string.parse::<f64>() { Ok(val) => temp_f64 = val, Err(_) => if !original_string.is_empty() { parse_error = true; } },
                  ColumnDataType::OptionF64 => match original_string.parse::<f64>() { Ok(val) => temp_opt_f64 = Some(val), Err(_) => if !original_string.is_empty() { temp_opt_f64 = None; parse_error = true; } else { temp_opt_f64 = None; }},
              }
          }
     }
    // --- End Parsing Logic ---

    // --- Create Response Variable ---
    let mut final_response: Option<egui::Response> = None;
    // --- Error Indicator ---
    let visual_error_indicator = parse_error; // Only use parse error for basic types here

    // Add the actual widget directly to the ui
    match validator_opt {
        // --- Linked Column: Call the dedicated handler ---
        Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
            // Call the handler function from the widgets module
            final_new_value = handle_linked_column_edit( // <-- UPDATED Call
                ui,
                id,
                &original_string, // Pass original string
                target_sheet_name,
                *target_column_index,
                registry,
                state,
            );
            // The handler and its visualization sub-module handle drawing errors.
            // We don't need to capture `final_response` or draw background here.
        }

        // --- Basic or No Validator ---
        Some(ColumnValidator::Basic(_)) | None => {
             let mut temp_new_value : Option<String> = None;
             // Use vertical layout to allow widgets to fill width
             ui.vertical(|ui| {
                 match basic_type {
                      ColumnDataType::String | ColumnDataType::OptionString => {
                          let mut temp_string = original_string.clone();
                          let resp = ui.add(egui::TextEdit::singleline(&mut temp_string).desired_width(f32::INFINITY));
                          if resp.changed() { temp_new_value = Some(temp_string); }
                          final_response = Some(resp);
                      }
                      ColumnDataType::Bool => { let resp = ui.add(egui::Checkbox::new(&mut temp_bool, "")); if resp.changed() { temp_new_value = Some(temp_bool.to_string()); } final_response = Some(resp); }
                      ColumnDataType::OptionBool => { let (changed, resp) = ui_option_bool(ui, id.with("opt_bool"), &mut temp_opt_bool); if changed { temp_new_value = Some(temp_opt_bool.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp); }
                      ColumnDataType::U8 => { let r = ui.add(egui::DragValue::new(&mut temp_u8)); if r.changed() { temp_new_value = Some(temp_u8.to_string()); } final_response = Some(r);},
                      ColumnDataType::OptionU8 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_u8"), &mut temp_opt_u8); if changed { temp_new_value = Some(temp_opt_u8.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp);},
                      ColumnDataType::U16 => { let r = ui.add(egui::DragValue::new(&mut temp_u16)); if r.changed() { temp_new_value = Some(temp_u16.to_string()); } final_response = Some(r);},
                      ColumnDataType::OptionU16 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_u16"), &mut temp_opt_u16); if changed { temp_new_value = Some(temp_opt_u16.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp);},
                      ColumnDataType::U32 => { let r = ui.add(egui::DragValue::new(&mut temp_u32)); if r.changed() { temp_new_value = Some(temp_u32.to_string()); } final_response = Some(r);},
                      ColumnDataType::OptionU32 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_u32"), &mut temp_opt_u32); if changed { temp_new_value = Some(temp_opt_u32.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp);},
                      ColumnDataType::U64 => { let r = ui.add(egui::DragValue::new(&mut temp_u64)); if r.changed() { temp_new_value = Some(temp_u64.to_string()); } final_response = Some(r);},
                      ColumnDataType::OptionU64 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_u64"), &mut temp_opt_u64); if changed { temp_new_value = Some(temp_opt_u64.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp);},
                      ColumnDataType::I8 => { let r = ui.add(egui::DragValue::new(&mut temp_i8)); if r.changed() { temp_new_value = Some(temp_i8.to_string()); } final_response = Some(r);},
                      ColumnDataType::OptionI8 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_i8"), &mut temp_opt_i8); if changed { temp_new_value = Some(temp_opt_i8.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp);},
                      ColumnDataType::I16 => { let r = ui.add(egui::DragValue::new(&mut temp_i16)); if r.changed() { temp_new_value = Some(temp_i16.to_string()); } final_response = Some(r);},
                      ColumnDataType::OptionI16 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_i16"), &mut temp_opt_i16); if changed { temp_new_value = Some(temp_opt_i16.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp);},
                      ColumnDataType::I32 => { let r = ui.add(egui::DragValue::new(&mut temp_i32)); if r.changed() { temp_new_value = Some(temp_i32.to_string()); } final_response = Some(r);},
                      ColumnDataType::OptionI32 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_i32"), &mut temp_opt_i32); if changed { temp_new_value = Some(temp_opt_i32.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp);},
                      ColumnDataType::I64 => { let r = ui.add(egui::DragValue::new(&mut temp_i64)); if r.changed() { temp_new_value = Some(temp_i64.to_string()); } final_response = Some(r);},
                      ColumnDataType::OptionI64 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_i64"), &mut temp_opt_i64); if changed { temp_new_value = Some(temp_opt_i64.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp);},
                      ColumnDataType::F32 => { let r = ui.add(egui::DragValue::new(&mut temp_f32).speed(0.1)); if r.changed() { temp_new_value = Some(temp_f32.to_string()); } final_response = Some(r);},
                      ColumnDataType::OptionF32 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_f32"), &mut temp_opt_f32); if changed { temp_new_value = Some(temp_opt_f32.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp);},
                      ColumnDataType::F64 => { let r = ui.add(egui::DragValue::new(&mut temp_f64).speed(0.1)); if r.changed() { temp_new_value = Some(temp_f64.to_string()); } final_response = Some(r);},
                      ColumnDataType::OptionF64 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_f64"), &mut temp_opt_f64); if changed { temp_new_value = Some(temp_opt_f64.map_or_else(String::new, |v| v.to_string())); } final_response = Some(resp);},
                 }
             });
             if let Some(val) = temp_new_value {
                 final_new_value = Some(val);
             }

             // --- Draw Error Background or Add Hover Text (Only for Basic types) ---
             if let Some(resp) = final_response {
                 let desired_height = ui.style().spacing.interact_size.y;
                 let widget_rect = resp.rect;
                 let bg_rect = egui::Rect::from_min_size(
                     widget_rect.min,
                     egui::vec2(widget_rect.width(), desired_height.max(widget_rect.height())),
                 );

                 if visual_error_indicator {
                     let error_bg_color = egui::Color32::from_rgb(60, 10, 10);
                     ui.painter().add(egui::Shape::rect_filled(
                         bg_rect,
                         ui.style().visuals.widgets.inactive.rounding(),
                         error_bg_color,
                     ));
                     // Add hover text for basic type parse error
                     resp.on_hover_text(format!("Parse Error! Input: '{}'", original_string));
                 } else if final_new_value.is_some()
                    && !(basic_type == ColumnDataType::String || basic_type == ColumnDataType::OptionString)
                 {
                     // Hover text for basic types showing original value
                     resp.on_hover_text(format!("Original: '{}'", original_string));
                 }
             }
             // --- End Draw Error Background ---
        } // End Basic/None Case
    } // End match validator_opt

    // Return the value determined by interaction this frame
    final_new_value
}


// --- Helper UIs for Option<bool> and Option<Numeric> ---
// These remain unchanged in common.rs

/// Helper UI for Option<bool> types. Returns (changed, response).
pub fn ui_option_bool(
    ui: &mut egui::Ui,
    _base_id: egui::Id, // base_id is unused now, but kept for signature consistency
    opt_value: &mut Option<bool>
) -> (bool, egui::Response) {
     let mut changed = false;
     let mut is_some = opt_value.is_some();
     let mut current_val = opt_value.unwrap_or(false);
     let inner_response = ui.horizontal(|ui| {
         let is_some_response = ui.add(egui::Checkbox::new(&mut is_some, ""));
         if is_some_response.changed() { *opt_value = if is_some { Some(current_val) } else { None }; changed = true; }
         ui.add_enabled_ui(is_some, |ui| {
              let current_val_response = ui.add(egui::Checkbox::new(&mut current_val, ""));
              if current_val_response.changed() { if is_some { *opt_value = Some(current_val); changed = true; } }
         });
     });
     (changed, inner_response.response)
}

/// Generic helper for Option<Numeric> types using DragValue. Returns (changed, response).
pub fn ui_option_numerical<T>(
    ui: &mut egui::Ui,
    _base_id: egui::Id, // base_id is unused now, but kept for signature consistency
    opt_value: &mut Option<T>
) -> (bool, egui::Response)
where
    T: egui::emath::Numeric + Default + Clone + Send + Sync + 'static + std::fmt::Display,
{
    let mut changed = false;
    let mut is_some = opt_value.is_some();
    let mut temp_val = opt_value.clone().unwrap_or_default();
    let inner_response = ui.horizontal(|ui| {
        let is_some_response = ui.add(egui::Checkbox::new(&mut is_some, ""));
        if is_some_response.changed() { *opt_value = if is_some { Some(temp_val.clone()) } else { None }; changed = true; }
         ui.add_enabled_ui(is_some, |ui|{
             let mut drag_speed = 1.0; if std::any::TypeId::of::<T>() == std::any::TypeId::of::<f32>() || std::any::TypeId::of::<T>() == std::any::TypeId::of::<f64>() { drag_speed = 0.1; }
             let drag_resp = ui.add(egui::DragValue::new(&mut temp_val).speed(drag_speed));
             if drag_resp.changed() { if is_some { *opt_value = Some(temp_val); changed = true; } }
        });
    });
    (changed, inner_response.response)
}
