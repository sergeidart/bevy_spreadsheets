// src/ui/elements/editor/table_header.rs
use bevy::log::info;
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Id, Order, PointerButton, Sense, Stroke};
use egui_extras::TableRow;

use super::state::{EditorWindowState, SheetInteractionState};
use crate::sheets::definitions::{ColumnValidator, SheetMetadata};
use crate::sheets::events::{
    RequestBatchUpdateColumnAiInclude, RequestReorderColumn, RequestUpdateAiSendSchema,
    RequestUpdateAiStructureSend, RequestUpdateColumnAiInclude,
};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ui_handlers;
pub fn sheet_table_header(
    header_row: &mut TableRow,
    ctx: &egui::Context,
    metadata: &SheetMetadata,
    sheet_name: &str,
    category: &Option<String>,
    _registry: &SheetRegistry,
    state: &mut EditorWindowState,
    mut reorder_writer: EventWriter<RequestReorderColumn>,
    mut column_include_writer: EventWriter<RequestUpdateColumnAiInclude>,
    mut batch_include_writer: EventWriter<RequestBatchUpdateColumnAiInclude>,
    mut send_schema_writer: EventWriter<RequestUpdateAiSendSchema>,
    mut structure_send_writer: EventWriter<RequestUpdateAiStructureSend>,
) {
    let headers = metadata.get_headers();
    let filters = metadata.get_filters();
    let num_cols = headers.len();

    // Compute visible columns (respecting structure context)
    let visible_columns = state.get_visible_column_indices(category, sheet_name, num_cols);

    // Column drag enabled
    let is_column_mode = true;
    let dnd_id_source = Id::new("column_dnd_context")
        .with(&state.selected_category)
        .with(sheet_name);

    let mut _header_row_y_range: Option<std::ops::RangeInclusive<f32>> = None;
    let primary_released_this_frame = ctx.input(|i| i.pointer.primary_released());
    
    // Track column rects for drop detection
    let mut column_rects: Vec<(usize, egui::Rect)> = Vec::new();

    for c_idx in visible_columns.iter().copied() {
        // Skip columns that are marked deleted in metadata
        if let Some(col_def) = metadata.columns.get(c_idx) {
            if col_def.deleted {
                continue;
            }
        }
        let header_text = &headers[c_idx];
        header_row.col(|ui| {
            if _header_row_y_range.is_none() {
                _header_row_y_range = Some(ui.max_rect().y_range().into());
            }

            let item_id = dnd_id_source.with(c_idx);
            let can_drag = is_column_mode;

            let (_id, mut response) = ui.allocate_at_least(ui.available_size_before_wrap(), Sense::click_and_drag());
            
            // Store column rect for later drop detection
            column_rects.push((c_idx, response.rect));

            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(response.rect), |header_content_ui| {
                header_content_ui.horizontal(|ui_h| {
                    // Treat Parent_key column specially: always included, cannot be deleted/unchecked
                    let is_parent_key = header_text.eq_ignore_ascii_case("parent_key") && c_idx == 1;
                    match state.current_interaction_mode {
                        SheetInteractionState::DeleteModeActive => {
                            if !is_parent_key {
                                let mut is_selected_for_delete = state.selected_columns_for_deletion.contains(&c_idx);
                                if ui_h.checkbox(&mut is_selected_for_delete, "").changed() {
                                    if is_selected_for_delete { state.selected_columns_for_deletion.insert(c_idx); }
                                    else { state.selected_columns_for_deletion.remove(&c_idx); }
                                }
                            } else {
                                // Reserve space for alignment but don't draw an interactive checkbox
                                let mut dummy = false;
                                ui_h.add_enabled(false, egui::Checkbox::without_text(&mut dummy));
                            }
                            ui_h.add_space(2.0);
                        }
                        SheetInteractionState::AiModeActive => {
                            if let Some(col_def) = metadata.columns.get(c_idx) {
                                if !matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                                    if is_parent_key {
                                        // Parent_key is always included and cannot be toggled
                                        let mut dummy = true;
                                        let _ =
                                            ui_h.add_enabled(false, egui::Checkbox::new(&mut dummy, ""));
                                        ui_h.add_space(2.0);
                                    } else {
                                        let in_structure_context =
                                            !state.virtual_structure_stack.is_empty();
                                        let mut is_included =
                                            !matches!(col_def.ai_include_in_send, Some(false));
                                        let checkbox_resp = ui_h.checkbox(&mut is_included, "");

                                        if checkbox_resp.changed() {
                                            if in_structure_context {
                                                ui_handlers::handle_ai_include_change_structure(
                                                    state,
                                                    metadata,
                                                    &headers,
                                                    category,
                                                    sheet_name,
                                                    c_idx,
                                                    is_included,
                                                    &mut send_schema_writer,
                                                );
                                            } else {
                                                ui_handlers::handle_ai_include_change_root(
                                                    state,
                                                    metadata,
                                                    &headers,
                                                    category,
                                                    sheet_name,
                                                    c_idx,
                                                    is_included,
                                                    &mut column_include_writer,
                                                );
                                            }
                                            ui_h.ctx().request_repaint();
                                        }

                                        checkbox_resp.context_menu(|menu_ui| {
                                            if in_structure_context {
                                                if menu_ui.button("Select all").clicked() {
                                                    ui_handlers::handle_select_all_structure(
                                                        state,
                                                        metadata,
                                                        &headers,
                                                        category,
                                                        sheet_name,
                                                        &visible_columns,
                                                        &mut send_schema_writer,
                                                    );
                                                    ui_h.ctx().request_repaint();
                                                    menu_ui.close_menu();
                                                }
                                                if menu_ui.button("De-select all").clicked() {
                                                    ui_handlers::handle_deselect_all_structure(
                                                        state,
                                                        metadata,
                                                        &headers,
                                                        category,
                                                        sheet_name,
                                                        &visible_columns,
                                                        &mut send_schema_writer,
                                                    );
                                                    ui_h.ctx().request_repaint();
                                                    menu_ui.close_menu();
                                                }
                                            } else {
                                                if menu_ui.button("Select all").clicked() {
                                                    ui_handlers::handle_select_all_root(
                                                        state,
                                                        metadata,
                                                        &headers,
                                                        category,
                                                        sheet_name,
                                                        &visible_columns,
                                                        &mut batch_include_writer,
                                                    );
                                                    ui_h.ctx().request_repaint();
                                                    menu_ui.close_menu();
                                                }
                                                if menu_ui.button("De-select all").clicked() {
                                                    ui_handlers::handle_deselect_all_root(
                                                        state,
                                                        metadata,
                                                        &headers,
                                                        category,
                                                        sheet_name,
                                                        &visible_columns,
                                                        &mut batch_include_writer,
                                                    );
                                                    ui_h.ctx().request_repaint();
                                                    menu_ui.close_menu();
                                                }
                                            }
                                        });

                                        ui_h.add_space(2.0);
                                    }
                                } else {
                                    let mut is_included = matches!(col_def.ai_include_in_send, Some(true));
                                    let checkbox_resp = ui_h.checkbox(&mut is_included, "");
                                    if checkbox_resp.changed() {
                                        ui_handlers::handle_structure_checkbox_change(
                                            state,
                                            category,
                                            sheet_name,
                                            c_idx,
                                            is_included,
                                            &mut structure_send_writer,
                                        );
                                        ui_h.ctx().request_repaint();
                                    }
                                    ui_h.add_space(2.0);
                                }
                            }
                        }
                        _ => {}
                    }

                    let display_text = if filters.get(c_idx).cloned().flatten().is_some() {
                        format!("{} (Filtered)", header_text)
                    } else {
                        header_text.clone()
                    };

                    let can_open_options = state.current_interaction_mode == SheetInteractionState::Idle;

                    // Determine header background color based on selection state
                    let header_bg_color = if state.selected_columns_for_deletion.contains(&c_idx) {
                        Color32::from_rgba_unmultiplied(120, 20, 20, 150)
                    } else {
                        Color32::TRANSPARENT
                    };

                    // Style: button-like header with darker background, full column width, centered text
                    let dark_bg_default = egui::Color32::from_rgb(50, 50, 52); // Slightly brighter for better visibility
                    let fill_color = if header_bg_color == egui::Color32::TRANSPARENT { dark_bg_default } else { header_bg_color };
                    let header_button = egui::Button::new(&display_text)
                        .fill(fill_color)
                        .min_size(ui_h.available_size());
                    let header_button_response = ui_h.add_enabled(can_open_options, header_button);
                    // Update last header right edge based on the button rect in content space
                    let button_right = header_button_response.rect.right();
                    if button_right.is_finite() && button_right > state.last_header_right_edge_x {
                        state.last_header_right_edge_x = button_right;
                    }

                    if header_button_response.clicked() && can_open_options {
                        state.show_column_options_popup = true;
                        state.options_column_target_sheet = sheet_name.to_string();
                        state.options_column_target_index = c_idx;
                        state.column_options_popup_needs_init = true;
                        state.options_column_target_category = metadata.category.clone();
                    }
                    if can_open_options {
                        header_button_response.on_hover_text(format!(
                            "Click for options for column '{}'",
                            header_text
                        ));
                    } else if !is_column_mode {
                        let mode_name = match state.current_interaction_mode {
                            SheetInteractionState::AiModeActive => "AI Mode",
                            SheetInteractionState::DeleteModeActive => "Delete Mode",
                            _ => "another mode",
                        };
                        header_button_response.on_hover_text(format!(
                            "Column options disabled while in {}",
                            mode_name
                        ));
                    }
                });
            });

            if can_drag {
                let interact_response = response.interact(Sense::drag());
                if interact_response.drag_started_by(PointerButton::Primary) {
                    if state.column_drag_state.source_index.is_none() {
                        state.column_drag_state.source_index = Some(c_idx);
                        ctx.set_dragged_id(item_id);
                        info!("Drag started for column idx: {}, id: {:?}", c_idx, item_id);
                    }
                }

                if ctx.is_being_dragged(item_id) {
                    response = response.on_hover_text(format!("Dragging column: {}", header_text));
                    egui::Area::new(item_id.with("drag_preview"))
                        .order(Order::Tooltip)
                        .current_pos(ctx.input(|i| i.pointer.hover_pos().unwrap_or(response.rect.center())))
                        .movable(false)
                        .show(ctx, |ui_preview| {
                            let frame = egui::Frame::popup(ui_preview.style());
                            frame.show(ui_preview, |fui| {
                                fui.label(format!("Moving: {}", header_text));
                            });
                        });
                }

                // --- Drop Target Visual Cue ---
                if let Some(source_column_idx) = state.column_drag_state.source_index {
                    if source_column_idx != c_idx && response.hovered() {
                        if let Some(globally_dragged_egui_id) = ctx.dragged_id() {
                            if globally_dragged_egui_id == dnd_id_source.with(source_column_idx) {
                                let painter = ui.painter();
                                let rect = response.rect;
                                let stroke = Stroke::new(2.0, Color32::GREEN);
                                if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
                                    if pointer_pos.x < rect.center().x {
                                        painter.vline(
                                            rect.left() + stroke.width / 2.0,
                                            rect.y_range(),
                                            stroke,
                                        );
                                    } else {
                                        painter.vline(
                                            rect.right() - stroke.width / 2.0,
                                            rect.y_range(),
                                            stroke,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                // Drop handling moved outside the loop for better detection
            }
        });
    }
    
    // --- Handle Drop Outside Column Loop ---
    // This ensures drop works even if pointer is between columns or timing is off
    if primary_released_this_frame {
        if let Some(source_idx) = state.column_drag_state.source_index {
            // Get pointer position
            if let Some(pointer_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                // Find which column the pointer is over or closest to
                let mut target_col_idx: Option<usize> = None;
                let mut insert_before = false;
                
                for (col_idx, rect) in column_rects.iter() {
                    if rect.contains(pointer_pos) {
                        // Pointer is directly over this column
                        target_col_idx = Some(*col_idx);
                        insert_before = pointer_pos.x < rect.center().x;
                        break;
                    }
                }
                
                // If we found a target column, calculate the destination index
                if let Some(target_idx) = target_col_idx {
                    let mut dest = if insert_before { target_idx } else { target_idx + 1 };
                    let max_len = metadata.columns.len();
                    if dest > max_len { 
                        dest = max_len; 
                    }
                    let new_index = if dest > source_idx { dest - 1 } else { dest };
                    
                    if source_idx != new_index {
                        info!("Dropping column {} at position {}", source_idx, new_index);
                        reorder_writer.write(RequestReorderColumn {
                            category: category.clone(),
                            sheet_name: sheet_name.to_string(),
                            old_index: source_idx,
                            new_index,
                        });
                    }
                } else {
                    info!("Drop cancelled - pointer not over any column");
                }
            }
            // Always clear the drag state when mouse is released
            state.column_drag_state.source_index = None;
        }
    }
}
