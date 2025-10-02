// src/ui/elements/editor/table_header.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Id, Order, PointerButton, Sense, Stroke};
use egui_extras::TableRow;

use super::state::{EditorWindowState, SheetInteractionState};
use crate::sheets::definitions::{ColumnValidator, SheetMetadata};
use crate::sheets::events::{
    RequestReorderColumn, RequestUpdateAiSendSchema, RequestUpdateAiStructureSend,
};
use crate::sheets::resources::SheetRegistry;

pub fn sheet_table_header(
    header_row: &mut TableRow,
    ctx: &egui::Context,
    metadata: &SheetMetadata,
    sheet_name: &str,
    category: &Option<String>,
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
    mut reorder_writer: EventWriter<RequestReorderColumn>,
    mut send_schema_writer: EventWriter<RequestUpdateAiSendSchema>,
    mut structure_send_writer: EventWriter<RequestUpdateAiStructureSend>,
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
                    match state.current_interaction_mode {
                        SheetInteractionState::DeleteModeActive => {
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
                        SheetInteractionState::AiModeActive => {
                            if let Some(col_def) = metadata.columns.get(c_idx) {
                                if !matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                                    let mut is_included = !matches!(col_def.ai_include_in_send, Some(false));
                                    if ui_h.checkbox(&mut is_included, "").changed() {
                                        if let Some((root_category, root_sheet, structure_path)) =
                                            resolve_root_context_for_current_view(
                                                registry,
                                                category,
                                                sheet_name,
                                            )
                                        {
                                            if root_sheet.is_empty() {
                                                warn!(
                                                    "Skipping AI send schema update: missing root sheet for {:?}/{}",
                                                    root_category, sheet_name
                                                );
                                            } else {
                                                let included_indices: Vec<usize> = metadata
                                                .columns
                                                .iter()
                                                .enumerate()
                                                .filter_map(|(idx, col)| {
                                                    if matches!(
                                                        col.validator,
                                                        Some(ColumnValidator::Structure)
                                                    ) {
                                                        return None;
                                                    }
                                                    let include = if idx == c_idx {
                                                        is_included
                                                    } else {
                                                        !matches!(
                                                            col.ai_include_in_send,
                                                            Some(false)
                                                        )
                                                    };
                                                    if include {
                                                        Some(idx)
                                                    } else {
                                                        None
                                                    }
                                                })
                                                .collect();
                                                let structure_path_opt = if structure_path.is_empty() {
                                                    None
                                                } else {
                                                    Some(structure_path)
                                                };
                                                send_schema_writer.write(RequestUpdateAiSendSchema {
                                                    category: root_category.clone(),
                                                    sheet_name: root_sheet.clone(),
                                                    structure_path: structure_path_opt,
                                                    included_columns: included_indices.clone(),
                                                });
                                                state.ai_cached_included_columns_category = category.clone();
                                                state.ai_cached_included_columns_sheet = Some(sheet_name.to_string());
                                                state.ai_cached_included_columns_path = state
                                                    .virtual_structure_stack
                                                    .iter()
                                                    .map(|ctx| ctx.parent.parent_col)
                                                    .collect();
                                                let mut flags =
                                                    vec![false; metadata.columns.len()];
                                                for &idx in &included_indices {
                                                    if let Some(slot) = flags.get_mut(idx) {
                                                        *slot = true;
                                                    }
                                                }
                                                state.ai_cached_included_columns = flags;
                                                state.ai_cached_included_columns_dirty = false;
                                                state.ai_cached_included_columns_valid = true;
                                            }
                                            ui_h.ctx().request_repaint();
                                        }
                                    }
                                    ui_h.add_space(2.0);
                                } else {
                                    let mut is_included =
                                        matches!(col_def.ai_include_in_send, Some(true));
                                    if ui_h.checkbox(&mut is_included, "").changed() {
                                        if let Some((root_category, root_sheet, mut structure_path)) =
                                            resolve_root_context_for_current_view(
                                                registry,
                                                category,
                                                sheet_name,
                                            )
                                        {
                                            if root_sheet.is_empty() {
                                                warn!(
                                                    "Skipping AI structure send update: missing root sheet for {:?}/{}",
                                                    root_category, sheet_name
                                                );
                                            } else {
                                                structure_path.push(c_idx);
                                                structure_send_writer
                                                    .write(RequestUpdateAiStructureSend {
                                                        category: root_category.clone(),
                                                        sheet_name: root_sheet.clone(),
                                                        structure_path: structure_path.clone(),
                                                        include: is_included,
                                                    });
                                                state.mark_ai_included_columns_dirty();
                                                ui_h.ctx().request_repaint();
                                            }
                                        }
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

                    let can_open_options =
                        state.current_interaction_mode == SheetInteractionState::Idle;

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
                // Show visual cue if we are dragging something and this column is not the source
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

                // --- Handle Drop ---
                if primary_released_this_frame {
                    // Global check for release
                    if let Some(source_idx) = state.column_drag_state.source_index {
                        // Our app knows a drag was in progress; check if the release happened over this column
                        if response.hovered() {
                            info!(
                                "Drop Attempt: Primary released on hovered column idx: {}, item_id: {:?}",
                                c_idx,
                                item_id
                            );
                            let mut target_drop_idx = c_idx;
                            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
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
                                info!(
                                    "Column drop confirmed: Source idx {}, Target insert idx {}",
                                    source_idx,
                                    final_insert_idx
                                );
                                reorder_writer.write(RequestReorderColumn {
                                    category: state.selected_category.clone(),
                                    sheet_name: sheet_name.to_string(),
                                    old_index: source_idx,
                                    new_index: final_insert_idx,
                                });
                            } else {
                                info!(
                                    "Drop resulted in no change (source_idx: {}, final_insert_idx: {}).",
                                    source_idx,
                                    final_insert_idx
                                );
                            }

                            state.column_drag_state.source_index = None;
                            ctx.set_dragged_id(Id::NULL);
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

fn resolve_root_context_for_current_view(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
) -> Option<(Option<String>, String, Vec<usize>)> {
    let mut root_category = category.clone();
    let mut root_sheet = sheet_name.to_string();
    let mut path_rev: Vec<usize> = Vec::new();
    let mut safety = 0;

    loop {
        safety += 1;
        if safety > 32 {
            warn!(
                "Structure parent chain exceeded safety limit for {:?}/{}",
                category, sheet_name
            );
            break;
        }
        let meta_opt = registry
            .get_sheet(&root_category, &root_sheet)
            .and_then(|s| s.metadata.as_ref());
        if let Some(meta) = meta_opt {
            if let Some(parent) = &meta.structure_parent {
                path_rev.push(parent.parent_column_index);
                root_category = parent.parent_category.clone();
                root_sheet = parent.parent_sheet.clone();
                continue;
            }
        }
        break;
    }

    path_rev.reverse();
    Some((root_category, root_sheet, path_rev))
}
