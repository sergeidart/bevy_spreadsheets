// src/ui/common.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Id, Response, Sense, Color32}; // Added Color32
use std::collections::{HashMap, HashSet};

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::widgets::handle_linked_column_edit;
use crate::ui::widgets::linked_column_cache::{self, CacheResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidationState {
    Empty,
    Valid,
    Invalid,
}

/// Renders interactive UI for editing a single cell based on its validator.
/// Handles parsing and showing the appropriate widget.
/// Returns Some(new_value) if the value was changed, None otherwise.
pub fn edit_cell_widget(
    ui: &mut egui::Ui,
    id: egui::Id,
    current_cell_string: &str,
    validator_opt: &Option<ColumnValidator>,
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
) -> Option<String> {
    let mut final_new_value: Option<String> = None;
    let original_string = current_cell_string.to_string();

    let basic_type = match validator_opt {
        Some(ColumnValidator::Basic(data_type)) => *data_type,
        Some(ColumnValidator::Linked { .. }) => ColumnDataType::String, // Base type for linked is string interaction
        None => ColumnDataType::String,
    };

    // --- Determine Validation State & Get Allowed Values for Linked ---
    let mut validation_state = ValidationState::Valid;
    let mut parse_error = false;
    let mut temp_allowed_values: Option<&HashSet<String>> = None;
    let empty_set_for_error = HashSet::new(); // Used if cache fails

    if current_cell_string.is_empty() {
        validation_state = ValidationState::Empty;
    } else {
        match validator_opt {
            Some(ColumnValidator::Basic(_)) | None => {
                 // Basic type parsing/validation
                 if !original_string.is_empty() {
                    match basic_type {
                         ColumnDataType::String | ColumnDataType::OptionString => {} // No parsing needed/possible for base string
                         ColumnDataType::Bool | ColumnDataType::OptionBool => { if !matches!(original_string.to_lowercase().as_str(), "true" | "1" | "false" | "0") { parse_error = true; } }
                         ColumnDataType::U8 | ColumnDataType::OptionU8 => if original_string.parse::<u8>().is_err() { parse_error = true; },
                         ColumnDataType::U16 | ColumnDataType::OptionU16 => if original_string.parse::<u16>().is_err() { parse_error = true; },
                         ColumnDataType::U32 | ColumnDataType::OptionU32 => if original_string.parse::<u32>().is_err() { parse_error = true; },
                         ColumnDataType::U64 | ColumnDataType::OptionU64 => if original_string.parse::<u64>().is_err() { parse_error = true; },
                         ColumnDataType::I8 | ColumnDataType::OptionI8 => if original_string.parse::<i8>().is_err() { parse_error = true; },
                         ColumnDataType::I16 | ColumnDataType::OptionI16 => if original_string.parse::<i16>().is_err() { parse_error = true; },
                         ColumnDataType::I32 | ColumnDataType::OptionI32 => if original_string.parse::<i32>().is_err() { parse_error = true; },
                         ColumnDataType::I64 | ColumnDataType::OptionI64 => if original_string.parse::<i64>().is_err() { parse_error = true; },
                         ColumnDataType::F32 | ColumnDataType::OptionF32 => if original_string.parse::<f32>().is_err() { parse_error = true; },
                         ColumnDataType::F64 | ColumnDataType::OptionF64 => if original_string.parse::<f64>().is_err() { parse_error = true; },
                    }
                 }
                if parse_error { validation_state = ValidationState::Invalid; }
            }
            Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                 // Get or populate cache for linked column options
                match linked_column_cache::get_or_populate_linked_options(
                    target_sheet_name, *target_column_index, registry, state,
                ) {
                    CacheResult::Success(allowed_values) => {
                        temp_allowed_values = Some(allowed_values); // Store reference
                        // Check if the current string is in the allowed set
                        if !allowed_values.contains(current_cell_string) {
                            validation_state = ValidationState::Invalid;
                        }
                    }
                    CacheResult::Error(_) => {
                         // If cache population failed (e.g., target invalid), mark as invalid
                         validation_state = ValidationState::Invalid;
                         temp_allowed_values = Some(&empty_set_for_error); // Pass empty set to widget on error
                    }
                }
            }
        }
    }
    // --- End Validation State Determination ---

    // --- Define the widget drawing logic as a closure ---
    // This closure draws the actual editing widget (TextEdit, Checkbox, etc.)
    // It expects to be run *inside* a Frame or other container that provides the background
    // It remains IDENTICAL to the previous version (widgets inside are frameless).
    let draw_widget_logic = |ui: &mut egui::Ui| -> (Option<Response>, Option<String>) {
        let mut widget_response: Option<Response> = None;
        let mut temp_new_value: Option<String> = None;

        // Initialize temporary variables for non-string types based on the original string
        // Only parse if not already marked as invalid (unless it's Linked, where we show the invalid text)
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
        if validation_state != ValidationState::Invalid || matches!( validator_opt, Some(ColumnValidator::Linked { .. }) ) {
             match basic_type {
                 ColumnDataType::String | ColumnDataType::OptionString => {} // No parsing needed
                 ColumnDataType::Bool => temp_bool = matches!(original_string.to_lowercase().as_str(), "true" | "1"),
                 ColumnDataType::OptionBool => temp_opt_bool = match original_string.to_lowercase().as_str() { "true" | "1" => Some(true), "false" | "0" => Some(false), _ => None },
                 ColumnDataType::U8 => temp_u8 = original_string.parse().unwrap_or(0),
                 ColumnDataType::OptionU8 => temp_opt_u8 = original_string.parse().ok(),
                 ColumnDataType::U16 => temp_u16 = original_string.parse().unwrap_or(0),
                 ColumnDataType::OptionU16 => temp_opt_u16 = original_string.parse().ok(),
                 ColumnDataType::U32 => temp_u32 = original_string.parse().unwrap_or(0),
                 ColumnDataType::OptionU32 => temp_opt_u32 = original_string.parse().ok(),
                 ColumnDataType::U64 => temp_u64 = original_string.parse().unwrap_or(0),
                 ColumnDataType::OptionU64 => temp_opt_u64 = original_string.parse().ok(),
                 ColumnDataType::I8 => temp_i8 = original_string.parse().unwrap_or(0),
                 ColumnDataType::OptionI8 => temp_opt_i8 = original_string.parse().ok(),
                 ColumnDataType::I16 => temp_i16 = original_string.parse().unwrap_or(0),
                 ColumnDataType::OptionI16 => temp_opt_i16 = original_string.parse().ok(),
                 ColumnDataType::I32 => temp_i32 = original_string.parse().unwrap_or(0),
                 ColumnDataType::OptionI32 => temp_opt_i32 = original_string.parse().ok(),
                 ColumnDataType::I64 => temp_i64 = original_string.parse().unwrap_or(0),
                 ColumnDataType::OptionI64 => temp_opt_i64 = original_string.parse().ok(),
                 ColumnDataType::F32 => temp_f32 = original_string.parse().unwrap_or(0.0),
                 ColumnDataType::OptionF32 => temp_opt_f32 = original_string.parse().ok(),
                 ColumnDataType::F64 => temp_f64 = original_string.parse().unwrap_or(0.0),
                 ColumnDataType::OptionF64 => temp_opt_f64 = original_string.parse().ok(),
             }
        }

        // Draw the actual widget centered vertically
        ui.vertical_centered(|ui| {
            match validator_opt {
                 // --- Linked Column Handling ---
                Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                    let allowed_values_ref = temp_allowed_values.unwrap_or(&empty_set_for_error);
                    // This call uses add_linked_text_edit internally, which uses .frame(false)
                    temp_new_value = handle_linked_column_edit(
                        ui, id, &original_string, target_sheet_name,
                        *target_column_index, registry,
                        allowed_values_ref,
                    );
                    // Allocate space even if invalid, for consistent layout/hover
                    if validation_state == ValidationState::Invalid {
                       widget_response = Some(ui.allocate_rect(ui.available_rect_before_wrap(), Sense::hover()));
                    }
                }
                // --- Basic/None Case ---
                Some(ColumnValidator::Basic(_)) | None => {
                     match basic_type {
                         // --- String Handling ---
                         ColumnDataType::String | ColumnDataType::OptionString => {
                             let mut temp_string = original_string.clone();
                             // Use add_sized to ensure TextEdit fills the allocated space
                             let resp = ui.add_sized(
                                 ui.available_size(), // Fill the space provided by the frame
                                 egui::TextEdit::singleline(&mut temp_string)
                                    .frame(false) // TextEdit itself is frameless
                             );
                             if resp.changed() {
                                 temp_new_value = Some(temp_string);
                             }
                             widget_response = Some(resp);
                         }
                         // --- Other Basic Types (Checkbox, DragValue, etc.) ---
                         ColumnDataType::Bool => { let resp = ui.add(egui::Checkbox::new(&mut temp_bool, "")); if resp.changed() { temp_new_value = Some(temp_bool.to_string()); } widget_response = Some(resp); }
                         ColumnDataType::OptionBool => { let (changed, resp) = ui_option_bool(ui, id.with("opt_bool"), &mut temp_opt_bool); if changed { temp_new_value = Some( temp_opt_bool.map_or_else(String::new, |v| v.to_string()), ); } widget_response = Some(resp); }
                         ColumnDataType::U8 => { let r = ui.add(egui::DragValue::new(&mut temp_u8)); if r.changed() { temp_new_value = Some(temp_u8.to_string()); } widget_response = Some(r);},
                         ColumnDataType::OptionU8 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_u8"), &mut temp_opt_u8); if changed { temp_new_value = Some(temp_opt_u8.map_or_else(String::new, |v| v.to_string())); } widget_response = Some(resp);},
                         ColumnDataType::U16 => { let r = ui.add(egui::DragValue::new(&mut temp_u16)); if r.changed() { temp_new_value = Some(temp_u16.to_string()); } widget_response = Some(r);},
                         ColumnDataType::OptionU16 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_u16"), &mut temp_opt_u16); if changed { temp_new_value = Some(temp_opt_u16.map_or_else(String::new, |v| v.to_string())); } widget_response = Some(resp);},
                         ColumnDataType::U32 => { let r = ui.add(egui::DragValue::new(&mut temp_u32)); if r.changed() { temp_new_value = Some(temp_u32.to_string()); } widget_response = Some(r);},
                         ColumnDataType::OptionU32 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_u32"), &mut temp_opt_u32); if changed { temp_new_value = Some(temp_opt_u32.map_or_else(String::new, |v| v.to_string())); } widget_response = Some(resp);},
                         ColumnDataType::U64 => { let r = ui.add(egui::DragValue::new(&mut temp_u64)); if r.changed() { temp_new_value = Some(temp_u64.to_string()); } widget_response = Some(r);},
                         ColumnDataType::OptionU64 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_u64"), &mut temp_opt_u64); if changed { temp_new_value = Some(temp_opt_u64.map_or_else(String::new, |v| v.to_string())); } widget_response = Some(resp);},
                         ColumnDataType::I8 => { let r = ui.add(egui::DragValue::new(&mut temp_i8)); if r.changed() { temp_new_value = Some(temp_i8.to_string()); } widget_response = Some(r);},
                         ColumnDataType::OptionI8 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_i8"), &mut temp_opt_i8); if changed { temp_new_value = Some(temp_opt_i8.map_or_else(String::new, |v| v.to_string())); } widget_response = Some(resp);},
                         ColumnDataType::I16 => { let r = ui.add(egui::DragValue::new(&mut temp_i16)); if r.changed() { temp_new_value = Some(temp_i16.to_string()); } widget_response = Some(r);},
                         ColumnDataType::OptionI16 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_i16"), &mut temp_opt_i16); if changed { temp_new_value = Some(temp_opt_i16.map_or_else(String::new, |v| v.to_string())); } widget_response = Some(resp);},
                         ColumnDataType::I32 => { let r = ui.add(egui::DragValue::new(&mut temp_i32)); if r.changed() { temp_new_value = Some(temp_i32.to_string()); } widget_response = Some(r);},
                         ColumnDataType::OptionI32 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_i32"), &mut temp_opt_i32); if changed { temp_new_value = Some(temp_opt_i32.map_or_else(String::new, |v| v.to_string())); } widget_response = Some(resp);},
                         ColumnDataType::I64 => { let r = ui.add(egui::DragValue::new(&mut temp_i64)); if r.changed() { temp_new_value = Some(temp_i64.to_string()); } widget_response = Some(r);},
                         ColumnDataType::OptionI64 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_i64"), &mut temp_opt_i64); if changed { temp_new_value = Some(temp_opt_i64.map_or_else(String::new, |v| v.to_string())); } widget_response = Some(resp);},
                         ColumnDataType::F32 => { let r = ui.add(egui::DragValue::new(&mut temp_f32).speed(0.1)); if r.changed() { temp_new_value = Some(temp_f32.to_string()); } widget_response = Some(r);},
                         ColumnDataType::OptionF32 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_f32"), &mut temp_opt_f32); if changed { temp_new_value = Some(temp_opt_f32.map_or_else(String::new, |v| v.to_string())); } widget_response = Some(resp);},
                         ColumnDataType::F64 => { let r = ui.add(egui::DragValue::new(&mut temp_f64).speed(0.1)); if r.changed() { temp_new_value = Some(temp_f64.to_string()); } widget_response = Some(r);},
                         ColumnDataType::OptionF64 => { let (changed, resp) = ui_option_numerical(ui, id.with("opt_f64"), &mut temp_opt_f64); if changed { temp_new_value = Some(temp_opt_f64.map_or_else(String::new, |v| v.to_string())); } widget_response = Some(resp);},
                     }
                }
            }
        }); // End vertical_centered

        // Return the widget response and any new value detected
        (widget_response, temp_new_value)
    };
    // --- End widget drawing closure ---


    // --- Allocate space ---
    let desired_size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
    let (_id, frame_rect) = ui.allocate_space(desired_size);

    // --- Determine Background Color for the Frame ---
    // NEW: Define distinct colors for Empty, Valid, Invalid
    let bg_color = match validation_state {
        ValidationState::Empty => Color32::TRANSPARENT, // Lets table background/stripes show
        ValidationState::Valid => Color32::from_gray(40), // Slightly lighter gray for valid content cells
        ValidationState::Invalid => Color32::from_rgba_unmultiplied(80, 20, 20, 180), // Semi-transparent red
    };

    // --- Always Draw the Frame with the Determined Background ---
    let frame = egui::Frame::none()
        // Add a small inner margin so the widget inside doesn't touch the frame edge
        .inner_margin(egui::Margin::symmetric(2, 1))
        .fill(bg_color);

    // Allocate UI in the rect and show the frame, running the drawing logic inside
    let inner_response = ui.allocate_ui_at_rect(frame_rect, |frame_ui| {
         frame.show(frame_ui, |ui_inside_frame| {
             // Run the logic that draws the actual (frameless) widget inside the frame
             draw_widget_logic(ui_inside_frame)
         }).inner // Get the result from frame.show
    }).inner; // Get the result from allocate_ui_at_rect

    let (_resp, new_val) = inner_response; // Result from draw_widget_logic
    final_new_value = new_val;

    // Add hover text for invalid state (remains the same)
    if validation_state == ValidationState::Invalid {
        let hover_text = if parse_error {
            format!("Parse Error! Invalid input: '{}'", original_string)
        } else {
            format!("Invalid Link! Value '{}' not found or link broken.", original_string)
        };
        // Use the allocated rect for hover sensing on the whole cell area
        ui.interact(frame_rect, id.with("hover_invalid"), Sense::hover())
            .on_hover_text(hover_text);
    }
    // --- End Frame Drawing ---

    final_new_value
}


// --- Helper UIs for Option<bool> and Option<Numeric> ---
// (These functions remain unchanged from previous version)

/// Helper UI for Option<bool> types. Returns (changed, response).
pub fn ui_option_bool(
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
pub fn ui_option_numerical<T>(
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
        None => (false, T::default()), // Use default if None
    };

    let inner_response = ui.horizontal(|ui| {
        // Checkbox to toggle Some/None state
        let is_some_response = ui.add(egui::Checkbox::without_text(&mut is_some));
        if is_some_response.changed() {
            *opt_value = if is_some { Some(temp_val.clone()) } else { None };
            changed = true;
        }
        // DragValue for the numeric value, enabled only if is_some is true
         ui.add_enabled_ui(is_some, |ui|{
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