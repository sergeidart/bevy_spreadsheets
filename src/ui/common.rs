// src/ui/common.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Response, Sense};
use std::collections::HashSet;
use std::sync::Arc;
// use std::str::FromStr; // Not strictly needed if parsing logic moves

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator, SheetMetadata},
    events::{OpenStructureViewEvent, RequestToggleAiRowGeneration, RequestCopyCell, RequestPasteCell},
    resources::{SheetRegistry, SheetRenderCache, ClipboardBuffer},
};
use crate::ui::elements::editor::state::{EditorWindowState, SheetInteractionState};
use crate::ui::validation::{normalize_for_link_cmp, ValidationState}; // Keep for enum access
use crate::ui::widgets::handle_linked_column_edit;
use crate::ui::widgets::linked_column_cache::{self, CacheResult};
// Option widgets removed

/// Generate a concise preview string for a structure cell, matching the grid view rendering.
/// Returns a tuple of (preview_text, parse_failed_flag).
pub fn generate_structure_preview(raw: &str) -> (String, bool) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (String::new(), false);
    }

    let mut out = String::new();
    let mut multi_rows = false;
    let mut parse_failed = false;

    fn stringify_json_value(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.to_owned(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        }
    }

    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(val) => match val {
            serde_json::Value::Array(arr) => {
                if arr.iter().all(|v| v.is_string()) {
                    let vals: Vec<String> = arr
                        .iter()
                        .map(stringify_json_value)
                        .filter(|s| !s.trim().is_empty())
                        .collect();
                    out = vals.join(", ");
                } else if arr.iter().all(|v| v.is_array()) {
                    multi_rows = arr.len() > 1;
                    if let Some(first) = arr.first().and_then(|v| v.as_array()) {
                        let vals: Vec<String> = first
                            .iter()
                            .map(stringify_json_value)
                            .filter(|s| !s.trim().is_empty())
                            .collect();
                        out = vals.join(", ");
                    }
                } else if arr.iter().all(|v| v.is_object()) {
                    multi_rows = arr.len() > 1;
                    if let Some(first) = arr.first().and_then(|v| v.as_object()) {
                        let mut entries: Vec<(String, String)> = first
                            .iter()
                            .map(|(k, v)| (k.clone(), stringify_json_value(v)))
                            .filter(|(k, v)| {
                                !v.trim().is_empty()
                                    && !k.eq_ignore_ascii_case("__parentdescriptor")
                            })
                            .collect();
                        entries.sort_by(|a, b| a.0.cmp(&b.0));
                        out = entries
                            .into_iter()
                            .map(|(_, v)| v)
                            .collect::<Vec<_>>()
                            .join(", ");
                    }
                } else {
                    let vals: Vec<String> = arr
                        .iter()
                        .map(stringify_json_value)
                        .filter(|s| !s.trim().is_empty())
                        .collect();
                    multi_rows = arr.len() > 1;
                    out = vals.join(", ");
                }
            }
            serde_json::Value::Object(map) => {
                let mut entries: Vec<(String, String)> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), stringify_json_value(v)))
                    .filter(|(k, v)| {
                        !v.trim().is_empty() && !k.eq_ignore_ascii_case("__parentdescriptor")
                    })
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                out = entries
                    .into_iter()
                    .map(|(_, v)| v)
                    .collect::<Vec<_>>()
                    .join(", ");
            }
            other => {
                out = stringify_json_value(&other);
            }
        },
        Err(_) => parse_failed = true,
    }

    if out.chars().count() > 64 {
        let truncated: String = out.chars().take(64).collect();
        out = truncated + "…";
    }
    if multi_rows {
        out.push_str("...");
    }
    (out, parse_failed)
}

/// Generate a preview string from structure rows (Vec<Vec<String>>)
/// Similar to generate_structure_preview but takes rows directly instead of JSON
pub fn generate_structure_preview_from_rows(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    
    let first_row = &rows[0];
    let values: Vec<String> = first_row
        .iter()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .collect();
    
    let mut out = values.join(", ");
    
    if out.chars().count() > 64 {
        let truncated: String = out.chars().take(64).collect();
        out = truncated + "…";
    }
    if rows.len() > 1 {
        out.push_str("...");
    }
    out
}

/// Renders interactive UI for editing a single cell based on its validator.
/// Handles displaying the appropriate widget and visual validation state.
/// Returns Some(new_value) if the value was changed by the user interaction *this frame*, None otherwise.
#[allow(clippy::too_many_arguments, unused_variables, unused_assignments)] // Accept many args and allow some unused depending on validator branch
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
    registry: &SheetRegistry,        // For metadata lookup for widget type
    render_cache: &SheetRenderCache, // Use the new render cache
    // Still need EditorWindowState mutably for linked column cache access (for dropdowns)
    state: &mut EditorWindowState,
    // NEW: event writer for structure navigation
    structure_open_events: &mut EventWriter<OpenStructureViewEvent>,
    _toggle_ai_events: &mut EventWriter<RequestToggleAiRowGeneration>,
    // NEW: event writers for copy/paste
    copy_events: &mut EventWriter<RequestCopyCell>,
    paste_events: &mut EventWriter<RequestPasteCell>,
    clipboard_buffer: &ClipboardBuffer,
) -> Option<String> {
    // Return type remains Option<String> for committed changes

    // --- 0. Read selection states first before any mutable borrows ---
    let is_column_selected_for_deletion = state.selected_columns_for_deletion.contains(&col_index);
    let is_row_selected = state.ai_selected_rows.contains(&row_index);
    let current_interaction_mode = state.current_interaction_mode;

    // --- 1. Read Pre-calculated RenderableCellData ---
    let render_cell_data_opt =
        render_cache.get_cell_data(category, sheet_name, row_index, col_index);

    let (current_display_text, cell_validation_state) = match render_cell_data_opt {
        Some(data) => (data.display_text.as_str(), data.validation_state),
        None => {
            warn!(
                "Render cache miss for cell [{}/{}, {}/{}]. Defaulting.",
                category.as_deref().unwrap_or("root"),
                sheet_name,
                row_index,
                col_index
            );
            ("", ValidationState::default())
        }
    };

    // Determine basic type for widget selection from metadata (still needed)
    let basic_type = registry
        .get_sheet(category, sheet_name)
        .and_then(|sd| sd.metadata.as_ref())
        .and_then(|meta| meta.columns.get(col_index))
        .map_or(ColumnDataType::String, |col_def| col_def.data_type);

    // Prefetch allowed values for linked columns once (and reuse below)
    let mut prefetch_allowed_values: Option<Arc<HashSet<String>>> = None;
    let mut prefetch_allowed_values_norm: Option<Arc<HashSet<String>>> = None;
    if let Some(ColumnValidator::Linked {
        target_sheet_name,
        target_column_index,
    }) = validator_opt
    {
        if let CacheResult::Success {
            raw: values,
            normalized,
        } = linked_column_cache::get_or_populate_linked_options(
            target_sheet_name,
            *target_column_index,
            registry,
            state,
        ) {
            prefetch_allowed_values = Some(values);
            prefetch_allowed_values_norm = Some(normalized);
        }
    }

    // --- 2. Parent_key read-only in structure tables ---
    if state.should_hide_structure_technical_columns(category, sheet_name) && col_index == 1 {
        let display = if current_display_text.is_empty() { "(empty)" } else { current_display_text };
        // Center horizontally and vertically within the cell for Parent_key
        let desired_size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
        let (_id, rect) = ui.allocate_space(desired_size);
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |inner| {
            inner.allocate_ui_with_layout(
                rect.size(),
                egui::Layout::left_to_right(egui::Align::Center),
                |row_ui| {
                    row_ui.vertical_centered(|vc| {
                        vc.label(
                            egui::RichText::new(display)
                                .color(egui::Color32::from_rgb(0, 180, 0)),
                        );
                    });
                },
            );
        });
        return None;
    }

    // --- 3. Allocate Space & Draw Frame ---
    let desired_size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
    let (frame_id, frame_rect) = ui.allocate_space(desired_size);

    // Determine an effective validation state: prefer the up-to-date cached linked set when present
    let effective_validation_state =
        if let Some(values_norm) = prefetch_allowed_values_norm.as_ref() {
            if current_display_text.is_empty() {
                ValidationState::Empty
            } else {
                let needle = normalize_for_link_cmp(current_display_text);
                let exists = values_norm.contains(&needle);
                if exists {
                    ValidationState::Valid
                } else {
                    ValidationState::Invalid
                }
            }
        } else {
            cell_validation_state
        };

    // Determine whether the cached AI-included column set applies to this cell
    let is_column_ai_included = if state.ai_cached_included_columns_valid
        && state.ai_cached_included_columns_sheet.as_deref() == Some(sheet_name)
        && state.ai_cached_included_columns_category.as_ref() == category.as_ref()
    {
        state
            .ai_cached_included_columns
            .get(col_index)
            .copied()
            .unwrap_or(false)
    } else {
        false
    };

    // Determine whether this is a structure column
    let is_structure_column = validator_opt
        .as_ref()
        .map(|v| matches!(v, crate::sheets::definitions::ColumnValidator::Structure))
        .unwrap_or(false);

    // Determine whether this is a linked column
    let is_linked_column = validator_opt
        .as_ref()
        .map(|v| matches!(v, crate::sheets::definitions::ColumnValidator::Linked { .. }))
        .unwrap_or(false);

    // Determine whether selected structure columns are included in AI sends
    let is_structure_ai_included = if is_structure_column
        && state.ai_cached_included_columns_valid
        && state.ai_cached_included_columns_sheet.as_deref() == Some(sheet_name)
        && state.ai_cached_included_columns_category.as_ref() == category.as_ref()
    {
        state
            .ai_cached_included_structure_columns
            .get(col_index)
            .copied()
            .unwrap_or(false)
    } else {
        false
    };

    // Determine selection-based coloring first, then fall back to validation colors
    let bg_color = {
        // Check for deletion selection (columns or rows)
        if is_column_selected_for_deletion {
            Color32::from_rgba_unmultiplied(120, 20, 20, 200) // Red background for column deletion
        } else if is_row_selected
            && current_interaction_mode == SheetInteractionState::DeleteModeActive
        {
            Color32::from_rgba_unmultiplied(120, 20, 20, 200) // Red background for row deletion
        }
        // Check for AI selection (rows only) - excluding structure columns
        else if is_row_selected && current_interaction_mode == SheetInteractionState::AiModeActive
        {
            if (is_structure_column && !is_structure_ai_included)
                || (!is_structure_column && !is_column_ai_included)
            {
                // Fall back to validation colors for structure columns
                match effective_validation_state {
                    ValidationState::Empty => Color32::TRANSPARENT,
                    ValidationState::Valid => Color32::TRANSPARENT,
                    ValidationState::Invalid => Color32::from_rgba_unmultiplied(80, 20, 20, 180),
                }
            } else {
                Color32::from_rgba_unmultiplied(20, 60, 120, 200) // Blue background for AI selection
            }
        }
        // Fall back to validation-based colors
        else {
            // Use a unified dark background slightly darker than the column header (which is rgb(40,40,40))
            let dark_cell_fill = Color32::from_rgb(35, 35, 35);
            match effective_validation_state {
                ValidationState::Empty => {
                    // For numeric and string cells, show the dark background even when empty to keep visual consistency
                    if matches!(basic_type, ColumnDataType::I64 | ColumnDataType::F64 | ColumnDataType::String) {
                        dark_cell_fill
                    } else {
                        Color32::TRANSPARENT
                    }
                }
                ValidationState::Valid => {
                    if is_linked_column {
                        // Correct (matching) linked values use the same dark background
                        dark_cell_fill
                    } else if matches!(basic_type, ColumnDataType::Bool | ColumnDataType::I64 | ColumnDataType::F64 | ColumnDataType::String) {
                        // Bool and numeric cells use the same dark background
                        dark_cell_fill
                    } else {
                        Color32::TRANSPARENT
                    }
                }
                ValidationState::Invalid => Color32::from_rgba_unmultiplied(80, 20, 20, 180),
            }
        }
    };

    let frame = egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(2, 1))
        .fill(bg_color);

    // --- 3. Draw the Frame and Widget Logic Inside ---
    let inner_response = ui
        .allocate_new_ui(egui::UiBuilder::new().max_rect(frame_rect), |frame_ui| {
            frame.show(frame_ui, |widget_ui| {
                    let mut response_opt: Option<Response> = None;
                    let mut temp_new_value: Option<String> = None;

                    macro_rules! handle_numeric {
                        ($ui:ident, $T:ty, $id_suffix:expr, $default:expr, $speed:expr) => {{
                            let mut value_for_widget: $T =
                                current_display_text.parse().unwrap_or($default);
                            // Make numeric control fill the entire column width (height = row height)
                            let size = egui::vec2($ui.available_width(), $ui.style().spacing.interact_size.y);
                            // Force the DragValue's internal background to the same dark color as the cell frame
                            let resp = $ui.scope(|ui_num| {
                                let dark = Color32::from_rgb(35, 35, 35);
                                let visuals = &mut ui_num.style_mut().visuals;
                                visuals.widgets.inactive.weak_bg_fill = dark;
                                visuals.widgets.inactive.bg_fill = dark;
                                visuals.widgets.hovered.weak_bg_fill = dark;
                                visuals.widgets.hovered.bg_fill = dark;
                                visuals.widgets.active.weak_bg_fill = dark;
                                visuals.widgets.active.bg_fill = dark;
                                ui_num.add_sized(size, egui::DragValue::new(&mut value_for_widget).speed($speed))
                            }).inner;
                            if resp.changed() {
                                temp_new_value = Some(value_for_widget.to_string());
                            }
                            resp.context_menu(|menu_ui| {
                                if menu_ui.button("📋 Copy").clicked() {
                                    copy_events.write(RequestCopyCell {
                                        category: category.clone(),
                                        sheet_name: sheet_name.to_string(),
                                        row_index,
                                        col_index,
                                    });
                                    menu_ui.close_menu();
                                }
                                let has_clipboard_data = clipboard_buffer.cell_value.is_some();
                                if menu_ui.add_enabled(has_clipboard_data, egui::Button::new("📄 Paste")).clicked() {
                                    paste_events.write(RequestPasteCell {
                                        category: category.clone(),
                                        sheet_name: sheet_name.to_string(),
                                        row_index,
                                        col_index,
                                    });
                                    menu_ui.close_menu();
                                }
                            });
                            response_opt = Some(resp);
                        }};
                    }
                    // Option<T> support removed

                    // Render content aligned to the top-left without vertical centering wrappers
                    match validator_opt {
                            Some(ColumnValidator::Structure) => {
                                // Structure columns render as buttons that navigate to the structure sheet
                                // Get the structure table name
                                let column_def = registry
                                    .get_sheet(category, sheet_name)
                                    .and_then(|sd| sd.metadata.as_ref())
                                    .and_then(|meta| meta.columns.get(col_index));
                                
                                if let Some(col_def) = column_def {
                                    let structure_sheet_name = format!("{}_{}", sheet_name, col_def.header);
                                    
                                    // Get the parent row's key (first column value) for filtering
                                    let parent_key = registry
                                        .get_sheet(category, sheet_name)
                                        .and_then(|sd| sd.grid.get(row_index))
                                        .and_then(|row| row.first())
                                        .map(|s| s.clone())
                                        .unwrap_or_else(|| row_index.to_string());
                                    
                                    // Use render cache preview text for this cell as primary button label.
                                    // When empty, fall back to a friendly column label.
                                    let button_text = if current_display_text.trim().is_empty() {
                                        col_def.header.clone()
                                    } else {
                                        current_display_text.to_string()
                                    };
                                    // Use default button styling (no custom fill)
                                    let button = egui::Button::new(button_text);
                                    let mut resp = widget_ui.add_sized(
                                        widget_ui.available_size(),
                                        button
                                    );
                                    
                                    // Add tooltip with details including cached row count
                                    // Build cache key matching state.ui_structure_row_count_cache signature
                                    // parent_row_index_in_root = row_index, structure_col_index = col_index, path_len = 1 (root level)
                                    let cache_key = (
                                        category.clone(),
                                        structure_sheet_name.clone(),
                                        row_index,
                                        col_index,
                                        1usize,
                                    );
                                    let mut count_opt = state.ui_structure_row_count_cache.get(&cache_key).copied();
                                    if count_opt.is_none() {
                                        // Compute quickly from grid by counting rows with matching parent_key in col 1
                                        if let Some(struct_sheet) = registry.get_sheet(category, &structure_sheet_name) {
                                            let c = struct_sheet
                                                .grid
                                                .iter()
                                                .filter(|r| r.get(1).map(|v| v == &parent_key).unwrap_or(false))
                                                .count();
                                            state.ui_structure_row_count_cache.insert(cache_key.clone(), c);
                                            count_opt = Some(c);
                                        }
                                    }
                                    let rows_count = count_opt.unwrap_or(0);
                                    resp = resp.on_hover_text(format!(
                                        "Structure: {}\nParent: {}\nRows: {}\nPreview: {}\n\nClick to open",
                                        structure_sheet_name, parent_key, rows_count, current_display_text
                                    ));
                                    
                                    if resp.clicked() {
                                        // If the real structure sheet exists, navigate to it. Otherwise, open virtual view.
                                        if registry.get_sheet(category, &structure_sheet_name).is_some() {
                                            let nav_context = crate::ui::elements::editor::state::StructureNavigationContext {
                                                structure_sheet_name: structure_sheet_name.clone(),
                                                parent_category: category.clone(),
                                                parent_sheet_name: sheet_name.to_string(),
                                                parent_row_key: parent_key.clone(),
                                                parent_column_name: col_def.header.clone(),
                                            };
                                            state.structure_navigation_stack.push(nav_context);
                                            state.selected_category = category.clone();
                                            state.selected_sheet_name = Some(structure_sheet_name);
                                        } else {
                                            // Fallback to virtual view for editing/adding when no structure sheet yet
                                            structure_open_events.write(OpenStructureViewEvent {
                                                parent_category: category.clone(),
                                                parent_sheet: sheet_name.to_string(),
                                                row_index,
                                                col_index,
                                            });
                                        }
                                    }
                                    
                                    response_opt = Some(resp);
                                } else {
                                    // Fallback if metadata not found
                                    let resp = widget_ui.label("?");
                                    response_opt = Some(resp);
                                }
                            }
                            Some(ColumnValidator::Linked {
                                target_sheet_name,
                                target_column_index,
                            }) => {
                                // Use prefetch if available; provide stable empty backing otherwise
                                let empty_backing_local; // defined on the stack and lives through this block
                                let allowed_values: &HashSet<String> =
                                    if let Some(values) = prefetch_allowed_values.as_ref() {
                                        values.as_ref()
                                    } else {
                                        empty_backing_local = HashSet::new();
                                        &empty_backing_local
                                    };
                                // Layout: [TextEdit (grows)] [→ Nav Button (fixed small square)]
                                // Ensure vertical centering for both controls
                                let (new_val, resp) = widget_ui
                                    .allocate_ui_with_layout(
                                        egui::vec2(
                                            widget_ui.available_width(),
                                            widget_ui.style().spacing.interact_size.y,
                                        ),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |row_ui| {
                                            row_ui.horizontal(|ui_h| {
                                                // Calculate available width for text edit (reserve space for nav button)
                                                let nav_button_size = ui_h.style().spacing.interact_size.y; // square button matching row height
                                                let spacing = ui_h.spacing().item_spacing.x;
                                                let available_for_edit =
                                                    (ui_h.available_width() - nav_button_size - spacing)
                                                        .max(8.0);

                                                // Draw text edit in a sized container (horizontally centered inside row height)
                                                let (new_val, resp) = ui_h
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(available_for_edit, nav_button_size),
                                                        egui::Layout::left_to_right(egui::Align::Center),
                                                        |edit_ui| {
                                                            edit_ui.vertical_centered(|vc| {
                                                                handle_linked_column_edit(
                                                                    vc,
                                                                    id,
                                                                    current_display_text,
                                                                    target_sheet_name,
                                                                    *target_column_index,
                                                                    registry,
                                                                    allowed_values,
                                                                )
                                                            })
                                                            .inner
                                                        },
                                                    )
                                                    .inner;

                                                // Draw small navigation button; use ASCII '>' to avoid font issues
                                                let nav_btn = ui_h
                                                    .add_sized([
                                                        nav_button_size,
                                                        nav_button_size,
                                                    ], egui::Button::new(">"))
                                                    .on_hover_text(format!(
                                                        "Navigate to sheet: {}",
                                                        target_sheet_name
                                                    ));

                                                if nav_btn.clicked() {
                                                    // Navigate to the target sheet
                                                    state.selected_category = category.clone();
                                                    state.selected_sheet_name =
                                                        Some(target_sheet_name.clone());
                                                    state.reset_interaction_modes_and_selections();
                                                    state.force_filter_recalculation = true;
                                                }

                                                (new_val, resp)
                                            })
                                            .inner
                                        },
                                    )
                                    .inner;
                                
                                temp_new_value = new_val;
                                // Add context menu to the linked column response
                                resp.context_menu(|menu_ui| {
                                    if menu_ui.button("📋 Copy").clicked() {
                                        copy_events.write(RequestCopyCell {
                                            category: category.clone(),
                                            sheet_name: sheet_name.to_string(),
                                            row_index,
                                            col_index,
                                        });
                                        menu_ui.close_menu();
                                    }
                                    let has_clipboard_data = clipboard_buffer.cell_value.is_some();
                                    if menu_ui.add_enabled(has_clipboard_data, egui::Button::new("📄 Paste")).clicked() {
                                        paste_events.write(RequestPasteCell {
                                            category: category.clone(),
                                            sheet_name: sheet_name.to_string(),
                                            row_index,
                                            col_index,
                                        });
                                        menu_ui.close_menu();
                                    }
                                });
                                response_opt = Some(resp);
                            }
                            Some(ColumnValidator::Basic(_)) | None => {
                                // Basic or No Validator
                                match basic_type {
                                    ColumnDataType::String => {
                                        let mut temp_string = current_display_text.to_string();
                                        let resp = widget_ui.add_sized(
                                            widget_ui.available_size(),
                                            egui::TextEdit::singleline(&mut temp_string)
                                                .frame(false),
                                        );
                                        if resp.changed() {
                                            temp_new_value = Some(temp_string);
                                        }
                                        resp.context_menu(|menu_ui| {
                                            if menu_ui.button("📋 Copy").clicked() {
                                                copy_events.write(RequestCopyCell {
                                                    category: category.clone(),
                                                    sheet_name: sheet_name.to_string(),
                                                    row_index,
                                                    col_index,
                                                });
                                                menu_ui.close_menu();
                                            }
                                            let has_clipboard_data = clipboard_buffer.cell_value.is_some();
                                            if menu_ui.add_enabled(has_clipboard_data, egui::Button::new("📄 Paste")).clicked() {
                                                paste_events.write(RequestPasteCell {
                                                    category: category.clone(),
                                                    sheet_name: sheet_name.to_string(),
                                                    row_index,
                                                    col_index,
                                                });
                                                menu_ui.close_menu();
                                            }
                                        });
                                        response_opt = Some(resp);
                                    }
                                    ColumnDataType::Bool => {
                                        let mut value_for_widget = matches!(
                                            current_display_text.to_lowercase().as_str(),
                                            "true" | "1"
                                        );
                                        // Center the checkbox horizontally and vertically within the cell
                                        let mut resp_opt: Option<Response> = None;
                                        widget_ui.allocate_ui_with_layout(
                                            egui::vec2(widget_ui.available_width(), widget_ui.style().spacing.interact_size.y),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |row_ui| {
                                                row_ui.vertical_centered(|vc| {
                                                    let r = vc.add(egui::Checkbox::new(&mut value_for_widget, ""));
                                                    resp_opt = Some(r);
                                                });
                                            },
                                        );
                                        let resp = resp_opt.expect("bool widget response should be set");
                                        if resp.changed() {
                                            temp_new_value = Some(value_for_widget.to_string());
                                        }
                                        resp.context_menu(|menu_ui| {
                                            if menu_ui.button("📋 Copy").clicked() {
                                                copy_events.write(RequestCopyCell {
                                                    category: category.clone(),
                                                    sheet_name: sheet_name.to_string(),
                                                    row_index,
                                                    col_index,
                                                });
                                                menu_ui.close_menu();
                                            }
                                            let has_clipboard_data = clipboard_buffer.cell_value.is_some();
                                            if menu_ui.add_enabled(has_clipboard_data, egui::Button::new("📄 Paste")).clicked() {
                                                paste_events.write(RequestPasteCell {
                                                    category: category.clone(),
                                                    sheet_name: sheet_name.to_string(),
                                                    row_index,
                                                    col_index,
                                                });
                                                menu_ui.close_menu();
                                            }
                                        });
                                        response_opt = Some(resp);
                                    }
                                    ColumnDataType::I64 => {
                                        handle_numeric!(widget_ui, i64, "i64", 0, 1.0)
                                    }
                                    ColumnDataType::F64 => {
                                        handle_numeric!(widget_ui, f64, "f64", 0.0, 0.1)
                                    }
                                }
                            }
                            // REMOVED: Old virtual structure implementation - replaced with real sheet navigation buttons above
                            /*
                            Some(ColumnValidator::Structure) => {
                                // Parse raw structure cell (not the render cache display text) to provide consistent preview and actions.
                                let raw_cell_json = registry
                                    .get_sheet(category, sheet_name)
                                    .and_then(|sd| sd.grid.get(row_index))
                                    .and_then(|r| r.get(col_index))
                                    .map(|s| s.as_str())
                                    .unwrap_or(current_display_text);
                                let (mut summary, parse_failed) =
                                    generate_structure_preview(raw_cell_json);
                                if parse_failed && !raw_cell_json.trim().is_empty() {
                                    summary = "(parse err)".to_string();
                                }
                                if summary.is_empty() {
                                    summary = "(empty)".to_string();
                                }
                                let structure_context =
                                    compute_structure_root_and_path(state, sheet_name, col_index);
                                let response_btn = centered_widget_ui.button(summary);
                                let clicked = response_btn.clicked();
                                response_btn.context_menu(|menu_ui| {
                                    let mut add_rows_clicked = false;
                                    let mut copy_clicked = false;
                                    let mut paste_clicked = false;
                                    let mut toggle_change: Option<(
                                        Option<String>,
                                        String,
                                        Vec<usize>,
                                        bool,
                                        Option<bool>,
                                    )> = None;

                                    // Copy button
                                    if menu_ui.button("📋 Copy").clicked() {
                                        copy_clicked = true;
                                    }
                                    
                                    // Paste button
                                    let has_clipboard_data = clipboard_buffer.cell_value.is_some();
                                    if menu_ui.add_enabled(has_clipboard_data, egui::Button::new("📄 Paste")).clicked() {
                                        paste_clicked = true;
                                    }

                                    menu_ui.horizontal(|row_ui| {
                                        let add_rows_resp = row_ui.add(
                                            egui::Label::new("Add Rows")
                                                .sense(egui::Sense::click()),
                                        );
                                        if add_rows_resp.clicked() {
                                            add_rows_clicked = true;
                                        }

                                        if structure_context.is_some() {
                                            row_ui.separator();
                                        }

                                        if let Some((root_category, root_sheet, structure_path)) =
                                            structure_context.as_ref()
                                        {
                                            if let Some(root_meta) = registry
                                                .get_sheet(root_category, root_sheet)
                                                .and_then(|sd| sd.metadata.as_ref())
                                            {
                                                let sheet_default =
                                                    root_meta.ai_enable_row_generation;
                                                let current_override =
                                                    resolve_structure_override_for_menu(
                                                        root_meta,
                                                        structure_path,
                                                    );
                                                let mut desired =
                                                    current_override.unwrap_or(sheet_default);
                                                let toggle_resp = row_ui
                                                    .add(egui::Checkbox::without_text(
                                                        &mut desired,
                                                    ))
                                                    .on_hover_text(
                                                        "Allow AI row generation for this structure",
                                                    );
                                                if toggle_resp.changed() {
                                                    let new_override = if desired == sheet_default {
                                                        None
                                                    } else {
                                                        Some(desired)
                                                    };
                                                    toggle_change = Some((
                                                        root_category.clone(),
                                                        root_sheet.clone(),
                                                        structure_path.clone(),
                                                        desired,
                                                        new_override,
                                                    ));
                                                }
                                            } else {
                                                let mut dummy = false;
                                                row_ui.add_enabled(
                                                    false,
                                                    egui::Checkbox::without_text(&mut dummy),
                                                );
                                            }
                                        }
                                    });

                                    if copy_clicked {
                                        copy_events.write(RequestCopyCell {
                                            category: category.clone(),
                                            sheet_name: sheet_name.to_string(),
                                            row_index,
                                            col_index,
                                        });
                                        menu_ui.close_menu();
                                    }

                                    if paste_clicked {
                                        paste_events.write(RequestPasteCell {
                                            category: category.clone(),
                                            sheet_name: sheet_name.to_string(),
                                            row_index,
                                            col_index,
                                        });
                                        menu_ui.close_menu();
                                    }

                                    if add_rows_clicked {
                                        if let Some((root_category, root_sheet, _structure_path)) =
                                            structure_context.as_ref()
                                        {
                                            structure_open_events.write(OpenStructureViewEvent {
                                                parent_category: root_category.clone(),
                                                parent_sheet: root_sheet.clone(),
                                                row_index,
                                                col_index,
                                            });
                                        } else {
                                            structure_open_events.write(OpenStructureViewEvent {
                                                parent_category: category.clone(),
                                                parent_sheet: sheet_name.to_string(),
                                                row_index,
                                                col_index,
                                            });
                                        }
                                        menu_ui.close_menu();
                                    }

                                    if let Some((root_category, root_sheet, structure_path, desired, new_override)) =
                                        toggle_change
                                    {
                                        toggle_ai_events.write(RequestToggleAiRowGeneration {
                                            category: root_category,
                                            sheet_name: root_sheet,
                                            enabled: desired,
                                            structure_path: Some(structure_path),
                                            structure_override: new_override,
                                        });
                                        menu_ui.close_menu();
                                    }
                                });
                                if clicked {
                                    structure_open_events.write(OpenStructureViewEvent {
                                        parent_category: category.clone(),
                                        parent_sheet: sheet_name.to_string(),
                                        row_index,
                                        col_index,
                                    });
                                }
                                response_opt = Some(response_btn);
                            }
                            */
                        }
                    (response_opt, temp_new_value)
                })
                .inner
        })
        .inner;

    let (_widget_resp_opt, final_new_value) = inner_response;

    // --- 4. Add Hover Text for Invalid State ---
    if effective_validation_state == ValidationState::Invalid {
        let hover_text = format!(
            "Invalid Value! '{}' is not allowed here.",
            current_display_text
        );
        ui.interact(frame_rect, frame_id.with("hover_invalid"), Sense::hover())
            .on_hover_text(hover_text);
    }
    final_new_value
}

#[allow(dead_code)]
fn compute_structure_root_and_path(
    state: &EditorWindowState,
    current_sheet_name: &str,
    col_index: usize,
) -> Option<(Option<String>, String, Vec<usize>)> {
    let mut path: Vec<usize> = state
        .virtual_structure_stack
        .iter()
        .map(|ctx| ctx.parent.parent_col)
        .collect();
    path.push(col_index);
    if path.is_empty() {
        return None;
    }
    let (root_category, root_sheet) = if let Some(first_ctx) = state.virtual_structure_stack.first()
    {
        (
            first_ctx.parent.parent_category.clone(),
            first_ctx.parent.parent_sheet.clone(),
        )
    } else {
        (
            state.selected_category.clone(),
            state
                .selected_sheet_name
                .clone()
                .unwrap_or_else(|| current_sheet_name.to_string()),
        )
    };
    Some((root_category, root_sheet, path))
}

#[allow(dead_code)]
fn resolve_structure_override_for_menu(meta: &SheetMetadata, path: &[usize]) -> Option<bool> {
    if path.is_empty() {
        return None;
    }
    let column = meta.columns.get(path[0])?;
    if path.len() == 1 {
        return column.ai_enable_row_generation;
    }
    let mut field = column.structure_schema.as_ref()?.get(path[1])?;
    if path.len() == 2 {
        return field.ai_enable_row_generation;
    }
    for idx in path.iter().skip(2) {
        field = field.structure_schema.as_ref()?.get(*idx)?;
    }
    field.ai_enable_row_generation
}

