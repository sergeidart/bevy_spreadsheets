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
        lineage_helpers::walk_parent_lineage,
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
    // Check if this is a technical column that should be displayed as green read-only
    let is_technical_column = if state.should_hide_structure_technical_columns(category, sheet_name) {
        registry
            .get_sheet(category, sheet_name)
            .and_then(|sd| sd.metadata.as_ref())
            .and_then(|meta| meta.columns.get(col_index))
            .map(|col_def| {
                col_def.header.eq_ignore_ascii_case("row_index")
                    || col_def.header.eq_ignore_ascii_case("parent_key")
                    || col_def.header.eq_ignore_ascii_case("temp_new_row_index")
                    || col_def.header.eq_ignore_ascii_case("_obsolete_temp_new_row_index")
            })
            .unwrap_or(false)
    } else {
        false
    };
    if is_technical_column {
        // Special handling for parent_key: show lineage with › separator
        let is_parent_key = registry
            .get_sheet(category, sheet_name)
            .and_then(|sd| sd.metadata.as_ref())
            .and_then(|meta| meta.columns.get(col_index))
            .map(|col_def| col_def.header.eq_ignore_ascii_case("parent_key"))
            .unwrap_or(false);
        
        let display = if is_parent_key && !current_display_text.is_empty() {
            // When in structure navigation context, use ancestor_keys from navigation state
            // Otherwise, build lineage by walking parent chain
            if let Some(nav_ctx) = state.structure_navigation_stack.last() {
                // We're in a structure navigation - use the navigation context lineage
                // ancestor_keys contains the full lineage including immediate parent
                if !nav_ctx.ancestor_keys.is_empty() {
                    nav_ctx.ancestor_keys.join(" › ")
                } else {
                    // Edge case: no ancestors (shouldn't happen in normal navigation)
                    current_display_text.to_string()
                }
            } else {
                // Not in structure navigation - walk the parent chain
                if let Ok(parent_row_idx) = current_display_text.parse::<usize>() {
                    // Try to get parent table info from metadata
                    if let Some(parent_link) = registry
                        .get_sheet(category, sheet_name)
                        .and_then(|sd| sd.metadata.as_ref())
                        .and_then(|meta| meta.structure_parent.as_ref())
                    {
                        let parent_category = parent_link.parent_category.clone();
                        let parent_sheet = parent_link.parent_sheet.clone();
                        
                        // Check cache first
                        let cache_key = (parent_category.clone(), parent_sheet.clone(), parent_row_idx);
                        let lineage = if let Some(cached) = state.parent_lineage_cache.get(&cache_key) {
                            cached.clone()
                        } else {
                            // Build lineage and cache it
                            let lineage = walk_parent_lineage(
                                registry,
                                &parent_category,
                                &parent_sheet,
                                parent_row_idx,
                            );
                            state.parent_lineage_cache.insert(cache_key, lineage.clone());
                            lineage
                        };
                        
                        // Format lineage with › separator
                        if !lineage.is_empty() {
                            lineage.iter()
                                .map(|(_, display_val, _)| display_val.as_str())
                                .collect::<Vec<_>>()
                                .join(" › ")
                        } else {
                            current_display_text.to_string()
                        }
                    } else {
                        current_display_text.to_string()
                    }
                } else {
                    current_display_text.to_string()
                }
            }
        } else if current_display_text.is_empty() {
            "(empty)".to_string()
        } else {
            current_display_text.to_string()
        };
        
    let desired_size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
        let (_id, rect) = ui.allocate_space(desired_size);
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |inner| {
            inner.allocate_ui_with_layout(
                rect.size(),
                egui::Layout::left_to_right(egui::Align::Center),
                |row_ui| {
                    row_ui.vertical_centered(|vc| {
                        let label = vc.label(
                            egui::RichText::new(&display).color(egui::Color32::from_rgb(0, 180, 0)),
                        );
                        
                        // Add tooltip for parent_key showing table names
                        if is_parent_key && !current_display_text.is_empty() {
                            if let Ok(parent_row_idx) = current_display_text.parse::<usize>() {
                                if let Some(parent_link) = registry
                                    .get_sheet(category, sheet_name)
                                    .and_then(|sd| sd.metadata.as_ref())
                                    .and_then(|meta| meta.structure_parent.as_ref())
                                {
                                    let cache_key = (parent_link.parent_category.clone(), parent_link.parent_sheet.clone(), parent_row_idx);
                                    if let Some(lineage) = state.parent_lineage_cache.get(&cache_key) {
                                        if !lineage.is_empty() {
                                            let tooltip_text = lineage.iter()
                                                .map(|(table, display, idx)| format!("{} ({}[{}])", display, table, idx))
                                                .collect::<Vec<_>>()
                                                .join(" › ");
                                            label.on_hover_text(tooltip_text);
                                        }
                                    }
                                }
                            }
                        }
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
                                    // Get parent's row_index to use for filtering children
                                    // After migration, children store parent's row_index in their parent_key column
                                    let parent_row_index = registry
                                        .get_sheet(category, sheet_name)
                                        .and_then(|sd| sd.grid.get(row_index))
                                        .and_then(|row| row.get(0)) // row_index is always at column 0
                                        .map(|s| s.clone())
                                        .unwrap_or_else(|| row_index.to_string());

                                    // For display purposes, also get the human-readable key
                                    let parent_display_key = registry
                                        .get_sheet(category, sheet_name)
                                        .and_then(|sd| {
                                            let row = sd.grid.get(row_index)?;
                                            let key_idx_dyn = sd.metadata.as_ref().and_then(|meta| {
                                                meta.columns.iter().position(|c| {
                                                    let h = c.header.to_ascii_lowercase();
                                                    h != "row_index"
                                                        && h != "parent_key"
                                                        && h != "temp_new_row_index"
                                                        && h != "_obsolete_temp_new_row_index"
                                                })
                                            }).or(Some(0));
                                            key_idx_dyn.and_then(|idx| row.get(idx)).cloned()
                                        })
                                        .unwrap_or_else(|| row_index.to_string());
                                    let ui_header = col_def
                                        .display_header
                                        .as_ref()
                                        .cloned()
                                        .unwrap_or_else(|| col_def.header.clone());
                                    let button_text = if current_display_text.trim().is_empty() {
                                        ui_header.clone()
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
                                            // Filter by parent_row_index (numeric comparison)
                                            // Children's parent_key column (index 1) stores the parent's row_index
                                            let c = struct_sheet
                                                .grid
                                                .iter()
                                                .filter(|r| r.get(1).map(|v| v == &parent_row_index).unwrap_or(false))
                                                .count();
                                            state.ui_structure_row_count_cache.insert(cache_key.clone(), c);
                                            count_opt = Some(c);
                                        }
                                    }
                                    let rows_count = count_opt.unwrap_or(0);
                                    resp = resp.on_hover_text(format!(
                                        "Structure: {}\nParent: {} (row_index: {})\nRows: {}\nPreview: {}\n\nClick to open",
                                        structure_sheet_name, parent_display_key, parent_row_index, rows_count, current_display_text
                                    ));
                                    if resp.clicked() {
                                        if registry.get_sheet(category, &structure_sheet_name).is_some() {
                                            // Build ancestor_keys (display values) and ancestor_row_indices (numeric) from navigation stack
                                            let mut ancestor_keys = if let Some(current_nav) = state.structure_navigation_stack.last() {
                                                current_nav.ancestor_keys.clone()
                                            } else {
                                                Vec::new()
                                            };
                                            
                                            let mut ancestor_row_indices = if let Some(current_nav) = state.structure_navigation_stack.last() {
                                                current_nav.ancestor_row_indices.clone()
                                            } else {
                                                Vec::new()
                                            };

                                            // Get parent row's row_index value to use as parent_key
                                            let parent_row_index_value = registry
                                                .get_sheet(category, sheet_name)
                                                .and_then(|sd| sd.grid.get(row_index))
                                                .and_then(|row| row.get(0)) // row_index is always at column 0
                                                .map(|s| s.clone())
                                                .unwrap_or_else(|| row_index.to_string());

                                            // Get the display value for the current row (for UI breadcrumb)
                                            let display_value = registry
                                                .get_sheet(category, sheet_name)
                                                .and_then(|sheet_data| {
                                                    sheet_data.metadata.as_ref().and_then(|metadata| {
                                                        sheet_data.grid.get(row_index).map(|row| {
                                                            crate::ui::elements::editor::structure_navigation::get_first_content_column_value(metadata, row)
                                                        })
                                                    })
                                                })
                                                .unwrap_or_else(|| current_display_text.to_string());

                                            // Add to lineage arrays
                                            ancestor_keys.push(display_value.clone());
                                            ancestor_row_indices.push(parent_row_index_value.clone());

                                            bevy::log::info!(
                                                "Opening structure: {} -> {} | parent_row_index='{}' (display: '{}') | ancestor_keys={:?} | ancestor_row_indices={:?}",
                                                sheet_name, structure_sheet_name, parent_row_index_value, display_value, ancestor_keys, ancestor_row_indices
                                            );
                                            let nav_context = crate::ui::elements::editor::state::StructureNavigationContext {
                                                structure_sheet_name: structure_sheet_name.clone(),
                                                parent_category: category.clone(),
                                                parent_sheet_name: sheet_name.to_string(),
                                                parent_row_key: parent_row_index_value,
                                                ancestor_keys,
                                                ancestor_row_indices,
                                                parent_column_name: ui_header,
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
