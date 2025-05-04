// src/ui/common.rs
// FINAL VERSION AFTER REFACTORING
use bevy::prelude::*;
use bevy_egui::egui::{self, Id, Response, Sense, Color32};
use std::collections::HashSet;

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::EditorWindowState;
// Import new validation functions and state
use crate::ui::validation::{validate_basic_cell, validate_linked_cell, ValidationState};
use crate::ui::widgets::handle_linked_column_edit;
// Import moved option widgets
use crate::ui::widgets::option_widgets::{ui_option_bool, ui_option_numerical};

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
    let mut parse_error = false;
    let mut temp_allowed_values: Option<&HashSet<String>> = None;
    let empty_set_for_error = HashSet::new(); // Used if cache fails

    let validation_state = match validator_opt {
        Some(ColumnValidator::Basic(_)) | None => {
            // Basic type parsing/validation handled by the new function
            let (state, basic_parse_error) = validate_basic_cell(current_cell_string, basic_type);
            parse_error = basic_parse_error; // Store parse error flag
            state // Return the state determined by the validator function
        }
        Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
            // Linked validation handled by the new function
            let (state, maybe_allowed) = validate_linked_cell(
                current_cell_string, target_sheet_name, *target_column_index, registry, state
            );
            if maybe_allowed.is_some() {
                temp_allowed_values = maybe_allowed; // Store reference if cache success
            } else if state == ValidationState::Invalid {
                // If cache errored or value invalid, ensure we have a non-None (but potentially empty) set for the widget
                temp_allowed_values = Some(&empty_set_for_error);
            }
            state
        }
    };
    // --- End Validation State Determination ---


    // --- Define the widget drawing logic as a closure ---
    let draw_widget_logic = |ui: &mut egui::Ui| -> (Option<Response>, Option<String>) {
        let mut widget_response: Option<Response> = None;
        let mut temp_new_value: Option<String> = None;

        // Initialize temporary variables for non-string types based on the original string
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

        // Only attempt parsing if the validation didn't mark it as explicitly Invalid
        // OR if it's a Linked type (where we still show the text edit even if invalid)
        if validation_state != ValidationState::Invalid || matches!( validator_opt, Some(ColumnValidator::Linked { .. }) ) {
             if !current_cell_string.is_empty() { // Avoid parsing empty strings
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
        }

        // Draw the actual widget centered vertically
        ui.vertical_centered(|ui| {
            match validator_opt {
                 // --- Linked Column Handling ---
                Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                    // Default to empty set if temp_allowed_values is None (due to cache error)
                    let allowed_values_ref = temp_allowed_values.unwrap_or(&empty_set_for_error);
                    temp_new_value = handle_linked_column_edit(
                        ui, id, &original_string, target_sheet_name,
                        *target_column_index, registry,
                        // state, // state no longer passed here
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
                             let resp = ui.add_sized(
                                 ui.available_size(),
                                 egui::TextEdit::singleline(&mut temp_string).frame(false)
                             );
                             if resp.changed() { temp_new_value = Some(temp_string); }
                             widget_response = Some(resp);
                         }
                         // --- Other Basic Types (Checkbox, DragValue, etc.) ---
                         // Calls ui_option_bool and ui_option_numerical from widgets::option_widgets
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

        (widget_response, temp_new_value)
    };
    // --- End widget drawing closure ---


    // --- Allocate space ---
    let desired_size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
    let (_id, frame_rect) = ui.allocate_space(desired_size);

    // --- Determine Background Color for the Frame ---
    let bg_color = match validation_state {
        ValidationState::Empty => Color32::TRANSPARENT,
        ValidationState::Valid => Color32::from_gray(40),
        ValidationState::Invalid => Color32::from_rgba_unmultiplied(80, 20, 20, 180),
    };

    // --- Always Draw the Frame with the Determined Background ---
    let frame = egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(2, 1))
        .fill(bg_color);

    // Allocate UI in the rect and show the frame, running the drawing logic inside
    let inner_response = ui.allocate_ui_at_rect(frame_rect, |frame_ui| {
         frame.show(frame_ui, |ui_inside_frame| {
             draw_widget_logic(ui_inside_frame)
         }).inner
    }).inner;

    let (_resp, new_val) = inner_response;
    final_new_value = new_val;

    // Add hover text for invalid state
    if validation_state == ValidationState::Invalid {
        let hover_text = if parse_error {
            format!("Parse Error! Invalid input: '{}'", original_string)
        } else {
            format!("Invalid Link! Value '{}' not found or link broken.", original_string)
        };
        ui.interact(frame_rect, id.with("hover_invalid"), Sense::hover())
            .on_hover_text(hover_text);
    }
    // --- End Frame Drawing ---

    final_new_value
}