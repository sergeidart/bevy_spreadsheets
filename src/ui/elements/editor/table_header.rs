// src/ui/elements/editor/table_header.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Id, Order, PointerButton, Sense, Stroke};
use egui_extras::TableRow;

use super::state::{EditorWindowState, SheetInteractionState};
use crate::sheets::definitions::SheetMetadata;
use crate::sheets::events::RequestReorderColumn;

pub fn sheet_table_header(
    header_row: &mut TableRow,
    ctx: &egui::Context,
    metadata: &SheetMetadata,
    sheet_name: &str,
    state: &mut EditorWindowState,
    mut reorder_writer: EventWriter<RequestReorderColumn>,
) {
    let headers = metadata.get_headers();
    let filters = metadata.get_filters();
    let num_cols = headers.len();

    // Column drag is now always enabled regardless of mode
    let is_column_mode = true;
    let dnd_id_source = Id::new("column_dnd_context")
        .with(&state.selected_category)
        .with(sheet_name);

    let mut _header_row_y_range: Option<std::ops::RangeInclusive<f32>> = None;
    let mut drop_handled_this_frame = false;

    // Check for global primary button release once at the start of header processing for this frame
    let primary_released_this_frame = ctx.input(|i| i.pointer.primary_released());

    for (c_idx, header_text) in headers.iter().enumerate() {
        header_row.col(|ui| {
            if _header_row_y_range.is_none() {
                 _header_row_y_range = Some(ui.max_rect().y_range().into());
            }

            let item_id = dnd_id_source.with(c_idx); 
            let can_drag = is_column_mode;

            let (_id, mut response) = ui.allocate_at_least(ui.available_size_before_wrap(), Sense::click_and_drag());
            
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(response.rect), |header_content_ui| {
                header_content_ui.horizontal(|ui_h| {
                    if matches!(state.current_interaction_mode, SheetInteractionState::DeleteModeActive) {
                        let mut is_selected_for_delete = state.selected_columns_for_deletion.contains(&c_idx);
                        if ui_h.checkbox(&mut is_selected_for_delete, "").changed() {
                            if is_selected_for_delete {
                                state.selected_columns_for_deletion.insert(c_idx);
                            } else {
                                state.selected_columns_for_deletion.remove(&c_idx);
                            }
                        }
                        ui_h.add_space(2.0);
                    }

                    let display_text = if filters.get(c_idx).cloned().flatten().is_some() {
                        format!("{} (Filtered)", header_text)
                    } else {
                        header_text.clone()
                    };

                    let can_open_options = state.current_interaction_mode == SheetInteractionState::Idle;
                    
                    // Determine header background color based on selection state
                    let header_bg_color = if state.selected_columns_for_deletion.contains(&c_idx) {
                        Color32::from_rgba_unmultiplied(120, 20, 20, 150) // Red background for deletion
                    } else {
                        Color32::TRANSPARENT
                    };
                    
                    let header_button = egui::Button::new(&display_text).fill(header_bg_color);
                    let header_button_response = ui_h.add_enabled(can_open_options, header_button);

                    if header_button_response.clicked() && can_open_options {
                        state.show_column_options_popup = true;
                        state.options_column_target_sheet = sheet_name.to_string();
                        state.options_column_target_index = c_idx;
                        state.column_options_popup_needs_init = true;
                        state.options_column_target_category = metadata.category.clone();
                    }
                    if can_open_options {
                         header_button_response.on_hover_text(format!("Click for options for column '{}'", header_text));
                    } else if !is_column_mode {
                        let mode_name = match state.current_interaction_mode {
                            SheetInteractionState::AiModeActive => "AI Mode",
                            SheetInteractionState::DeleteModeActive => "Delete Mode",
                            _ => "another mode",
                        };
                         header_button_response.on_hover_text(format!("Column options disabled while in {}", mode_name));
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
                // Show visual cue if we are dragging something and this column is not the source
                if let Some(source_column_idx) = state.column_drag_state.source_index {
                    if source_column_idx != c_idx && response.hovered() {
                        // Further check if egui is actually dragging our item
                         if let Some(globally_dragged_egui_id) = ctx.dragged_id() {
                            if globally_dragged_egui_id == dnd_id_source.with(source_column_idx) {
                                let painter = ui.painter(); 
                                let rect = response.rect; 
                                let stroke = Stroke::new(2.0, Color32::GREEN);
                                if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
                                    if pointer_pos.x < rect.center().x { 
                                        painter.vline(rect.left() + stroke.width / 2.0, rect.y_range(), stroke);
                                    } else { 
                                        painter.vline(rect.right() - stroke.width / 2.0, rect.y_range(), stroke);
                                    }
                                }
                            }
                        }
                    }
                }

                // --- Handle Drop ---
                if primary_released_this_frame { // Global check for release
                    if let Some(source_idx) = state.column_drag_state.source_index { // Our app knows a drag was in progress
                        // Check if the release happened over *this specific column's response area*
                        if response.hovered() {
                            info!("Drop Attempt: Primary released on hovered column idx: {}, item_id: {:?}", c_idx, item_id);
                            // At this point, egui's internal dragged_id might already be None.
                            // We rely on our `source_idx` and `response.hovered()`.

                            let mut target_drop_idx = c_idx; 
                            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) { // Should still be valid for hover check
                                if pos.x > response.rect.center().x {
                                    target_drop_idx += 1; 
                                }
                            }
                            
                            let final_insert_idx = if source_idx < target_drop_idx {
                                target_drop_idx.saturating_sub(1)
                            } else {
                                target_drop_idx
                            };
                            let final_insert_idx = final_insert_idx.min(num_cols); 

                            if source_idx != final_insert_idx && final_insert_idx <= num_cols {
                                info!("Column drop confirmed: Source idx {}, Target insert idx {}", source_idx, final_insert_idx);
                                reorder_writer.write(RequestReorderColumn {
                                    category: state.selected_category.clone(),
                                    sheet_name: sheet_name.to_string(),
                                    old_index: source_idx,
                                    new_index: final_insert_idx,
                                });
                            } else {
                                info!("Drop resulted in no change (source_idx: {}, final_insert_idx: {}).", source_idx, final_insert_idx);
                            }
                            
                            state.column_drag_state.source_index = None;
                            ctx.set_dragged_id(Id::NULL); // Clear egui's state too
                            drop_handled_this_frame = true; 
                        }
                    }
                }
            } 
        });
    }

    // Global drag release check (if mouse released and not handled by a specific column drop)
    if primary_released_this_frame && !drop_handled_this_frame {
        if state.column_drag_state.source_index.is_some() {
            info!(
                "Drag cancelled or released outside target (Global Check). Source idx: {:?}",
                state.column_drag_state.source_index
            );
            state.column_drag_state.source_index = None;
            // It's good practice to also clear egui's dragged_id if our app thought a drag was active.
            // Egui might have already cleared it, but this ensures consistency.
            ctx.set_dragged_id(Id::NULL);
        }
    }

    if num_cols == 0 {
        header_row.col(|ui| {
            ui.strong("(No Columns)");
        });
    }
}
