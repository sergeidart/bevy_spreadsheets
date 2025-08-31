// src/ui/common.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Response, Sense};
use std::collections::HashSet;
// use std::str::FromStr; // Not strictly needed if parsing logic moves

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    resources::{SheetRegistry, SheetRenderCache},
    events::OpenStructureViewEvent,
};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::validation::ValidationState; // Keep for enum access
use crate::ui::widgets::handle_linked_column_edit;
use crate::ui::widgets::linked_column_cache::{self, CacheResult};
use crate::ui::widgets::option_widgets::{ui_option_bool, ui_option_numerical};

/// Renders interactive UI for editing a single cell based on its validator.
/// Handles displaying the appropriate widget and visual validation state.
/// Returns Some(new_value) if the value was changed by the user interaction *this frame*, None otherwise.
#[allow(clippy::too_many_arguments)] // We accept the number of args for this central function
pub fn edit_cell_widget(
    ui: &mut egui::Ui,
    id: egui::Id,
    // current_cell_string: &str, // No longer need direct cell string from grid
    validator_opt: &Option<ColumnValidator>, // Still need validator for widget type
    // ADDED parameters for context
    category: &Option<String>,
    sheet_name: &str,
    row_index: usize,
    col_index: usize,
    // --- Resources ---
    registry: &SheetRegistry, // For metadata lookup for widget type
    render_cache: &SheetRenderCache, // Use the new render cache
    // Still need EditorWindowState mutably for linked column cache access (for dropdowns)
    state: &mut EditorWindowState,
    // NEW: event writer for structure navigation
    structure_open_events: &mut EventWriter<OpenStructureViewEvent>,
) -> Option<String> { // Return type remains Option<String> for committed changes

    // --- 1. Read Pre-calculated RenderableCellData ---
    let render_cell_data_opt = render_cache.get_cell_data(category, sheet_name, row_index, col_index);

    let (current_display_text, cell_validation_state) = match render_cell_data_opt {
        Some(data) => (data.display_text.as_str(), data.validation_state),
        None => {
            warn!("Render cache miss for cell [{}/{}, {}/{}]. Defaulting.",
                category.as_deref().unwrap_or("root"), sheet_name, row_index, col_index);
            ("", ValidationState::default())
        }
    };


    // Determine basic type for widget selection from metadata (still needed)
    let basic_type = registry.get_sheet(category, sheet_name)
        .and_then(|sd| sd.metadata.as_ref())
        .and_then(|meta| meta.columns.get(col_index))
        .map_or(ColumnDataType::String, |col_def| col_def.data_type);


    // --- 2. Allocate Space & Draw Frame ---
    let desired_size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
    let (frame_id, frame_rect) = ui.allocate_space(desired_size);

    // Determine Background Color based on pre-calculated state
    let bg_color = match cell_validation_state {
        ValidationState::Empty => Color32::TRANSPARENT,
        ValidationState::Valid => Color32::from_gray(40),
        ValidationState::Invalid => Color32::from_rgba_unmultiplied(80, 20, 20, 180),
    };

    let frame = egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(2, 1))
        .fill(bg_color);

    // --- 3. Draw the Frame and Widget Logic Inside ---
    let inner_response = ui
        .allocate_ui_at_rect(frame_rect, |frame_ui| {
            frame.show(frame_ui, |widget_ui| {
                let mut response_opt: Option<Response> = None;
                let mut temp_new_value: Option<String> = None;

                macro_rules! handle_numeric {
                    ($ui:ident, $T:ty, $id_suffix:expr, $default:expr, $speed:expr) => {
                        {
                            let mut value_for_widget: $T = current_display_text.parse().unwrap_or($default);
                            // CORRECTED: Removed .frame(false) from DragValue
                            let resp = $ui.add(egui::DragValue::new(&mut value_for_widget).speed($speed));
                            if resp.changed() {
                                temp_new_value = Some(value_for_widget.to_string());
                            }
                            response_opt = Some(resp);
                        }
                    };
                }
                macro_rules! handle_option_numeric {
                     ($ui:ident, $T:ty, $id_suffix:expr) => {
                        {
                            let mut value_for_widget: Option<$T> = if current_display_text.is_empty() {
                                None
                            } else {
                                current_display_text.parse().ok()
                            };
                            let (changed, resp) = ui_option_numerical($ui, id.with($id_suffix), &mut value_for_widget);
                            if changed {
                                temp_new_value = Some(value_for_widget.map_or_else(String::new, |v| v.to_string()));
                            }
                            response_opt = Some(resp);
                        }
                     };
                }

                widget_ui.vertical_centered(|centered_widget_ui| {
                    match validator_opt {
                        Some(ColumnValidator::Linked {
                            target_sheet_name,
                            target_column_index,
                        }) => {
                            let allowed_values = match linked_column_cache::get_or_populate_linked_options(
                                target_sheet_name,
                                *target_column_index,
                                registry,
                                state,
                            ) {
                                CacheResult::Success(values) => values,
                                CacheResult::Error(_) => &HashSet::new(),
                            };
                            temp_new_value = handle_linked_column_edit(
                                centered_widget_ui,
                                id,
                                current_display_text,
                                target_sheet_name,
                                *target_column_index,
                                registry,
                                allowed_values,
                            );
                            if temp_new_value.is_none() && response_opt.is_none() {
                                response_opt = Some(
                                    centered_widget_ui.allocate_rect(
                                        centered_widget_ui.available_rect_before_wrap(),
                                        Sense::hover(),
                                    ),
                                );
                            }
                        }
                        Some(ColumnValidator::Basic(_)) | None => { // Basic or No Validator
                            match basic_type {
                                ColumnDataType::String | ColumnDataType::OptionString => {
                                    let mut temp_string = current_display_text.to_string();
                                    let resp = centered_widget_ui.add_sized(
                                        centered_widget_ui.available_size(),
                                        egui::TextEdit::singleline(&mut temp_string).frame(false),
                                    );
                                    if resp.changed() {
                                        temp_new_value = Some(temp_string);
                                    }
                                    response_opt = Some(resp);
                                }
                                ColumnDataType::Bool => {
                                    let mut value_for_widget = matches!(current_display_text.to_lowercase().as_str(), "true" | "1");
                                    let resp = centered_widget_ui.add(egui::Checkbox::new(&mut value_for_widget, ""));
                                    if resp.changed() {
                                        temp_new_value = Some(value_for_widget.to_string());
                                    }
                                    response_opt = Some(resp);
                                }
                                ColumnDataType::OptionBool => {
                                     let mut value_for_widget: Option<bool> = match current_display_text.to_lowercase().as_str() {
                                        "" => None,
                                        "true" | "1" => Some(true),
                                        "false" | "0" => Some(false),
                                        _ => None,
                                    };
                                    let (changed, resp) = ui_option_bool(centered_widget_ui, id.with("opt_bool"), &mut value_for_widget);
                                    if changed {
                                        temp_new_value = Some(value_for_widget.map_or_else(String::new, |v| v.to_string()));
                                    }
                                    response_opt = Some(resp);
                                }
                                ColumnDataType::U8 => { handle_numeric!(centered_widget_ui, u8, "u8", 0, 1.0) },
                                ColumnDataType::OptionU8 => { handle_option_numeric!(centered_widget_ui, u8, "opt_u8") },
                                ColumnDataType::U16 => { handle_numeric!(centered_widget_ui, u16, "u16", 0, 1.0) },
                                ColumnDataType::OptionU16 => { handle_option_numeric!(centered_widget_ui, u16, "opt_u16") },
                                ColumnDataType::U32 => { handle_numeric!(centered_widget_ui, u32, "u32", 0, 1.0) },
                                ColumnDataType::OptionU32 => { handle_option_numeric!(centered_widget_ui, u32, "opt_u32") },
                                ColumnDataType::U64 => { handle_numeric!(centered_widget_ui, u64, "u64", 0, 1.0) },
                                ColumnDataType::OptionU64 => { handle_option_numeric!(centered_widget_ui, u64, "opt_u64") },
                                ColumnDataType::I8 => { handle_numeric!(centered_widget_ui, i8, "i8", 0, 1.0) },
                                ColumnDataType::OptionI8 => { handle_option_numeric!(centered_widget_ui, i8, "opt_i8") },
                                ColumnDataType::I16 => { handle_numeric!(centered_widget_ui, i16, "i16", 0, 1.0) },
                                ColumnDataType::OptionI16 => { handle_option_numeric!(centered_widget_ui, i16, "opt_i16") },
                                ColumnDataType::I32 => { handle_numeric!(centered_widget_ui, i32, "i32", 0, 1.0) },
                                ColumnDataType::OptionI32 => { handle_option_numeric!(centered_widget_ui, i32, "opt_i32") },
                                ColumnDataType::I64 => { handle_numeric!(centered_widget_ui, i64, "i64", 0, 1.0) },
                                ColumnDataType::OptionI64 => { handle_option_numeric!(centered_widget_ui, i64, "opt_i64") },
                                ColumnDataType::F32 => { handle_numeric!(centered_widget_ui, f32, "f32", 0.0, 0.1) },
                                ColumnDataType::OptionF32 => { handle_option_numeric!(centered_widget_ui, f32, "opt_f32") },
                                ColumnDataType::F64 => { handle_numeric!(centered_widget_ui, f64, "f64", 0.0, 0.1) },
                                ColumnDataType::OptionF64 => { handle_option_numeric!(centered_widget_ui, f64, "opt_f64") },
                            }
                        }
                        Some(ColumnValidator::Structure) => {
                            // Multi-row aware preview: cells store positional arrays (single row: [..], multi: [[..],[..]]) or legacy JSON.
                            // MUST parse raw cell value (not display_text which may already be a preview) to avoid false parse errors.
                            let raw_cell_json = registry
                                .get_sheet(category, sheet_name)
                                .and_then(|sd| sd.grid.get(row_index))
                                .and_then(|r| r.get(col_index))
                                .map(|s| s.as_str())
                                .unwrap_or(current_display_text); // fallback to display text
                            let trimmed = raw_cell_json.trim();
                            let mut summary: String;
                            if trimmed.is_empty() || trimmed == "{}" || trimmed == "[]" { summary = "(empty)".to_string(); }
                            else if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                                match val {
                                    serde_json::Value::Array(rows) => {
                                        let row_count = rows.len();
                                        if row_count == 0 { summary = "(empty)".to_string(); }
                                        else {
                                            // Determine first row representation depending on format
                                            let mut parts: Vec<String> = Vec::new();
                                            if rows.iter().all(|e| e.is_string()) { // single-row new format
                                                for (i,v) in rows.iter().enumerate().take(6) {
                                                    let s = v.as_str().unwrap_or("");
                                                    let mut val_c = s.to_string(); if val_c.len()>24 { val_c.truncate(24); val_c.push_str("…"); }
                                                    parts.push(val_c);
                                                }
                                            } else if rows.iter().all(|e| e.is_array()) { // multi-row new format
                                                if let Some(first_arr) = rows.get(0).and_then(|v| v.as_array()) {
                                                    for (i,v) in first_arr.iter().enumerate().take(6) {
                                                        let s = v.as_str().unwrap_or("");
                                                        let mut val_c = s.to_string(); if val_c.len()>24 { val_c.truncate(24); val_c.push_str("…"); }
                                                        parts.push(val_c);
                                                    }
                                                }
                                            } else if let Some(first_obj) = rows.get(0).and_then(|r| r.as_object()) { // legacy multi-row objects
                                                for (k,v) in first_obj.iter().take(6) {
                                                    let val_str = v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string());
                                                    let mut val_c = val_str; if val_c.len()>24 { val_c.truncate(24); val_c.push_str("…"); }
                                                    parts.push(format!("{}={}", k, val_c));
                                                }
                                            }
                                            let multi = row_count > 1;
                                            // Use " | " delimiter and skip empty items for cleaner preview
                                            parts.retain(|p| !p.trim().is_empty());
                                            summary = parts.join(" | ");
                                            if multi { summary.push_str("..."); }
                                        }
                                    }
                                    serde_json::Value::Object(map) => {
                                        if map.is_empty() { summary = "(empty)".to_string(); } else {
                                            let mut parts: Vec<String> = map.iter().take(4).map(|(k,v)| {
                                                let val_str = v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string());
                                                let mut val_c = val_str;
                                                if val_c.len() > 24 { val_c.truncate(24); val_c.push_str("…"); }
                                                format!("{}={}", k, val_c)
                                            }).collect();
                                            parts.retain(|p| !p.ends_with('='));
                                            let extra = if map.len() > 4 { format!(" (+{} more)", map.len()-4) } else { String::new() };
                                            summary = parts.join(" | ") + &extra;
                                        }
                                    }
                                    other => {
                                        let mut raw = other.to_string();
                                        if raw.len() > 64 { raw.truncate(64); raw.push_str("…"); }
                                        summary = raw;
                                    }
                                }
                            } else {
                                summary = "(parse err)".to_string();
                            }
                            if summary.is_empty() { summary = "(empty)".to_string(); }
                            if summary.len() > 64 { summary.truncate(64); summary.push_str("…"); }
                            let btn = centered_widget_ui.button(summary);
                            if btn.clicked() {
                                structure_open_events.write(OpenStructureViewEvent {
                                    parent_category: category.clone(),
                                    parent_sheet: sheet_name.to_string(),
                                    row_index,
                                    col_index,
                                });
                            }
                            response_opt = Some(btn);
                        }
                    }
                });
                (response_opt, temp_new_value)
            })
            .inner
        })
        .inner;

    let (_widget_resp_opt, final_new_value) = inner_response;

    // --- 4. Add Hover Text for Invalid State ---
    if cell_validation_state == ValidationState::Invalid {
        let hover_text = format!("Invalid Value! '{}' is not allowed here.", current_display_text);
        ui.interact(frame_rect, frame_id.with("hover_invalid"), Sense::hover())
            .on_hover_text(hover_text);
    }
    final_new_value
}