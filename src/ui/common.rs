// src/ui/common.rs

use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Response, Sense};
use std::collections::HashSet;

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    events::{
        OpenStructureViewEvent, RequestCopyCell, RequestPasteCell, RequestToggleAiRowGeneration,
    },
    resources::{ClipboardBuffer, SheetRegistry, SheetRenderCache},
    systems::logic::{
        determine_cell_background_color, determine_effective_validation_state,
        is_column_ai_included, is_structure_column_ai_included, prefetch_linked_column_values,
    },
};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::validation::ValidationState;
use crate::ui::widgets::handle_linked_column_edit;

#[allow(clippy::too_many_arguments, unused_variables, unused_assignments)]
pub fn edit_cell_widget(
    ui: &mut egui::Ui,
    id: egui::Id,
    validator_opt: &Option<ColumnValidator>,
    category: &Option<String>,
    sheet_name: &str,
    row_index: usize,
    col_index: usize,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    state: &mut EditorWindowState,
    structure_open_events: &mut EventWriter<OpenStructureViewEvent>,
    _toggle_ai_events: &mut EventWriter<RequestToggleAiRowGeneration>,
    copy_events: &mut EventWriter<RequestCopyCell>,
    paste_events: &mut EventWriter<RequestPasteCell>,
    clipboard_buffer: &ClipboardBuffer,
) -> Option<String> {
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

    let basic_type = registry
        .get_sheet(category, sheet_name)
        .and_then(|sd| sd.metadata.as_ref())
        .and_then(|meta| meta.columns.get(col_index))
        .map_or(ColumnDataType::String, |col_def| col_def.data_type);

    let prefetch = prefetch_linked_column_values(validator_opt, registry, state);
    let prefetch_allowed_values = prefetch.raw_values;
    let prefetch_allowed_values_norm = prefetch.normalized_values;

    // Parent_key column in structure tables is read-only and centered
    if state.should_hide_structure_technical_columns(category, sheet_name) && col_index == 1 {
        let display = if current_display_text.is_empty() {
            "(empty)"
        } else {
            current_display_text
        };
    let desired_size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
        let (_id, rect) = ui.allocate_space(desired_size);
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |inner| {
            inner.allocate_ui_with_layout(
                rect.size(),
                egui::Layout::left_to_right(egui::Align::Center),
                |row_ui| {
                    row_ui.vertical_centered(|vc| {
                        vc.label(
                            egui::RichText::new(display).color(egui::Color32::from_rgb(0, 180, 0)),
                        );
                    });
                },
            );
        });
        return None;
    }

    let desired_size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
    let (frame_id, frame_rect) = ui.allocate_space(desired_size);

    let effective_validation_state = determine_effective_validation_state(
        current_display_text,
        &prefetch_allowed_values_norm,
        cell_validation_state,
    );

    let col_ai_included = is_column_ai_included(state, category, sheet_name, col_index);

    let is_structure_column = matches!(
        validator_opt,
        Some(ColumnValidator::Structure)
    );

    let is_linked_column = matches!(
        validator_opt,
        Some(ColumnValidator::Linked { .. })
    );

    let struct_ai_included =
        is_structure_column_ai_included(state, category, sheet_name, col_index, is_structure_column);

    let bg_color = determine_cell_background_color(
        is_column_selected_for_deletion,
        is_row_selected,
        current_interaction_mode,
        is_structure_column,
        struct_ai_included,
        col_ai_included,
        effective_validation_state,
        is_linked_column,
        basic_type,
    );

    let frame = egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(2, 1))
        .fill(bg_color);

    let inner_response = ui
        .allocate_new_ui(egui::UiBuilder::new().max_rect(frame_rect), |frame_ui| {
            frame.show(frame_ui, |widget_ui| {
                    let mut response_opt: Option<Response> = None;
                    let mut temp_new_value: Option<String> = None;

                    macro_rules! handle_numeric {
                        ($ui:ident, $T:ty, $id_suffix:expr, $default:expr, $speed:expr) => {{
                            let mut value_for_widget: $T =
                                current_display_text.parse().unwrap_or($default);
                            let size = egui::vec2($ui.available_width(), $ui.style().spacing.interact_size.y);
                            let resp = $ui.scope(|ui_num| {
                                let dark = Color32::from_rgb(45, 45, 45);
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
                                if menu_ui.button("🗑 Clear").clicked() {
                                    temp_new_value = Some(String::new());
                                    menu_ui.close_menu();
                                }
                            });
                            response_opt = Some(resp);
                        }};
                    }

                    match validator_opt {
                            Some(ColumnValidator::Structure) => {
                                let column_def = registry
                                    .get_sheet(category, sheet_name)
                                    .and_then(|sd| sd.metadata.as_ref())
                                    .and_then(|meta| meta.columns.get(col_index));
                                
                                if let Some(col_def) = column_def {
                                    let structure_sheet_name = format!("{}_{}", sheet_name, col_def.header);
                                    // Get parent key: use structure_key_parent_column_index if available, 
                                    // otherwise use first real data column (index 1 for regular, 2 for structure)
                                    let parent_key = registry
                                        .get_sheet(category, sheet_name)
                                        .and_then(|sd| {
                                            let row = sd.grid.get(row_index)?;
                                            let key_col_idx = col_def.structure_key_parent_column_index
                                                .or_else(|| {
                                                    // Fallback: first data column
                                                    if let Some(meta) = &sd.metadata {
                                                        if meta.is_structure_table() {
                                                            Some(2) // Skip row_index (0) and parent_key (1)
                                                        } else {
                                                            Some(1) // Skip row_index (0)
                                                        }
                                                    } else {
                                                        Some(0) // Fallback if no metadata
                                                    }
                                                });
                                            key_col_idx.and_then(|idx| row.get(idx)).map(|s| s.clone())
                                        })
                                        .unwrap_or_else(|| row_index.to_string());
                                    let button_text = if current_display_text.trim().is_empty() {
                                        col_def.header.clone()
                                    } else {
                                        current_display_text.to_string()
                                    };
                                    let button = egui::Button::new(button_text);
                                    let mut resp = widget_ui.add_sized(
                                        widget_ui.available_size(),
                                        button
                                    );
                                    let cache_key = (
                                        category.clone(),
                                        structure_sheet_name.clone(),
                                        row_index,
                                        col_index,
                                        1usize,
                                    );
                                    let mut count_opt = state.ui_structure_row_count_cache.get(&cache_key).copied();
                                    if count_opt.is_none() {
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
                                    let resp = widget_ui.label("?");
                                    response_opt = Some(resp);
                                }
                            }
                            Some(ColumnValidator::Linked {
                                target_sheet_name,
                                target_column_index,
                            }) => {
                                let empty_backing_local;
                                let allowed_values: &HashSet<String> =
                                    if let Some(values) = prefetch_allowed_values.as_ref() {
                                        values.as_ref()
                                    } else {
                                        empty_backing_local = HashSet::new();
                                        &empty_backing_local
                                    };
                                let (new_val, resp) = widget_ui
                                    .allocate_ui_with_layout(
                                        egui::vec2(
                                            widget_ui.available_width(),
                                            widget_ui.style().spacing.interact_size.y,
                                        ),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |row_ui| {
                                            row_ui.horizontal(|ui_h| {
                                                let nav_button_size = ui_h.style().spacing.interact_size.y;
                                                let spacing = ui_h.spacing().item_spacing.x;
                                                let available_for_edit =
                                                    (ui_h.available_width() - nav_button_size - spacing)
                                                        .max(8.0);

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
                                    if menu_ui.button("🗑 Clear").clicked() {
                                        temp_new_value = Some(String::new());
                                        menu_ui.close_menu();
                                    }
                                });
                                response_opt = Some(resp);
                            }
                            Some(ColumnValidator::Basic(_)) | None => {
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
                                            if menu_ui.button("🗑 Clear").clicked() {
                                                temp_new_value = Some(String::new());
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
                                            if menu_ui.button("🗑 Clear").clicked() {
                                                temp_new_value = Some(String::new());
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
                        }
                    (response_opt, temp_new_value)
                })
                .inner
        })
        .inner;

    let (_widget_resp_opt, final_new_value) = inner_response;

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
