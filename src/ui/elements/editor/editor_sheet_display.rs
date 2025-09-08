// src/ui/elements/editor/editor_sheet_display.rs
use bevy::prelude::*;
use bevy_egui::egui; // EguiContexts might not be needed here if ctx is passed
use egui_extras::{Column, TableBody, TableBuilder};
use crate::sheets::{
    resources::{SheetRegistry, SheetRenderCache},
    events::{RequestReorderColumn, UpdateCellEvent, OpenStructureViewEvent, AddSheetRowRequest, RequestAddColumn},
};
use crate::ui::elements::editor::state::{EditorWindowState, SheetInteractionState, AiModeState};
use crate::ui::elements::editor::table_header::sheet_table_header;
// removed: legacy direct body helper (we render body inline to support the new control column)
// No longer need to import SheetEventWriters as a whole struct here

#[allow(clippy::too_many_arguments)]
pub(super) fn show_sheet_table(
    ui: &mut egui::Ui,
    ctx: &egui::Context, 
    row_height: f32,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    // MODIFIED: Accept individual EventWriters directly by value
    reorder_column_writer: EventWriter<RequestReorderColumn>,
    mut cell_update_writer: EventWriter<UpdateCellEvent>,
    mut open_structure_writer: EventWriter<OpenStructureViewEvent>,
    mut add_row_writer: EventWriter<AddSheetRowRequest>,
    mut add_column_writer: EventWriter<RequestAddColumn>,
) {
    // If user picked a different real sheet while a virtual structure stack exists, exit structure view
    if !state.virtual_structure_stack.is_empty() {
        if let Some(current_sel) = &state.selected_sheet_name {
            // Root parent sheet is the parent sheet of the first (oldest) virtual context
            let root_parent_sheet = state.virtual_structure_stack.first().map(|v| v.parent.parent_sheet.clone());
            let root_parent_category_opt = state.virtual_structure_stack.first().and_then(|v| v.parent.parent_category.clone());
            // If user changed either sheet name or category away from the root parent, clear stack
            let category_changed = root_parent_category_opt != state.selected_category;
            if root_parent_sheet.as_ref() != Some(current_sel) || category_changed {
                state.virtual_structure_stack.clear();
            }
        } else {
            // No sheet selected anymore -> clear
            state.virtual_structure_stack.clear();
        }
    }
    // If a virtual structure sheet is active, temporarily override selected sheet for rendering
    let backup_sheet = state.selected_sheet_name.clone();
    if let Some(vctx) = state.virtual_structure_stack.last() {
        if let Some(vsheet) = registry.get_sheet(&state.selected_category, &vctx.virtual_sheet_name) {
            if vsheet.metadata.is_some() {
                state.selected_sheet_name = Some(vctx.virtual_sheet_name.clone());
            }
        }
    }
    if let Some(selected_name) = &state.selected_sheet_name.clone() { 
        let current_category_clone = state.selected_category.clone(); 

        let sheet_data_ref_opt = registry.get_sheet(&current_category_clone, selected_name);

        if sheet_data_ref_opt.is_none() {
            warn!("Selected sheet '{:?}/{}' not found in registry for rendering.", current_category_clone, selected_name);
            ui.vertical_centered(|ui| { ui.label(format!("Sheet '{:?}/{}' no longer exists...", current_category_clone, selected_name)); });
            if state.selected_sheet_name.as_deref() == Some(selected_name.as_str()) {
                state.selected_sheet_name = None;
                state.reset_interaction_modes_and_selections();
                state.force_filter_recalculation = true;
            }
            return;
        }

        if let Some(sheet_data_ref) = sheet_data_ref_opt {
             if let Some(metadata) = &sheet_data_ref.metadata {
                // Notice for standard sheets only (structure view handled earlier)
                let num_cols = metadata.columns.len();
                if metadata.get_filters().len() != num_cols && num_cols > 0 {
                     error!("Metadata inconsistency detected (cols vs filters) for sheet '{:?}/{}'. Revalidation might be needed.", current_category_clone, selected_name);
                     ui.colored_label(egui::Color32::RED, "Metadata inconsistency detected...");
                     return;
                }

                // Determine if this is a virtual structure sheet and gather ancestor key columns
                // Each entry stores (header_text, value_text). We'll display only the value in both header & body as requested.
                let mut ancestor_key_columns: Vec<(String, String)> = Vec::new();
                if let Some(last_ctx) = state.virtual_structure_stack.last() {
                    if last_ctx.virtual_sheet_name == *selected_name {
                        // Iterate through stack in order (oldest -> newest)
                        for vctx in &state.virtual_structure_stack {
                            if let Some(parent_sheet) = registry.get_sheet(&state.selected_category, &vctx.parent.parent_sheet) {
                                if let (Some(parent_meta), Some(parent_row)) = (&parent_sheet.metadata, parent_sheet.grid.get(vctx.parent.parent_row)) {
                                    if let Some(struct_col_def) = parent_meta.columns.get(vctx.parent.parent_col) {
                                        // Prefer parent-selected key; fallback to first non-structure
                                        let key_col_idx = struct_col_def.structure_key_parent_column_index
                                            .or_else(|| parent_meta.columns.iter().position(|c| !matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure))))
                                            .unwrap_or(0);
                                        if let Some(key_col_def) = parent_meta.columns.get(key_col_idx) {
                                            let value = parent_row.get(key_col_idx).cloned().unwrap_or_default();
                                            ancestor_key_columns.push((key_col_def.header.clone(), value));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Capture the start position to anchor floating buttons later
                let table_start_pos = ui.next_widget_position();

                let scroll_resp = egui::ScrollArea::both()
                    .id_salt("main_sheet_table_scroll_area")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                       let mut table_builder = TableBuilder::new(ui)
                           .striped(true)
                           .resizable(true)
                           .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                           .min_scrolled_height(0.0);

                       let prefix_count = ancestor_key_columns.len();
                       let total_cols = num_cols + prefix_count;

                       if total_cols == 0 {
                            if state.scroll_to_row_index.is_some() { state.scroll_to_row_index = None; }
                            // Always add a fixed left control column
                            table_builder = table_builder
                                .column(Column::initial(26.0).at_least(26.0).resizable(false))
                                .column(Column::remainder().resizable(false));
                       } else {
                            // Add fixed left control column for checkboxes/buttons
                            table_builder = table_builder.column(Column::initial(26.0).at_least(26.0).resizable(false));

                            // Build prefix (read-only key) columns next (if any)
                            for _ in 0..prefix_count { table_builder = table_builder.column(Column::initial(110.0).at_least(60.0).resizable(false).clip(true)); }
                            const DEFAULT_COL_WIDTH: f32 = 120.0; // width field deprecated
                            for _i in 0..num_cols {
                                let col = Column::initial(DEFAULT_COL_WIDTH).at_least(40.0).resizable(true).clip(true);
                                table_builder = table_builder.column(col);
                            }
                       }
                       if let Some(row_idx) = state.scroll_to_row_index {
                            if total_cols > 0 {
                                table_builder = table_builder.scroll_to_row(row_idx, Some(egui::Align::TOP));
                            }
                            state.scroll_to_row_index = None;
                       }

                       table_builder
                           .header(row_height, |mut header_row| {
                               // Left control header cell: keep minimal content and do NOT draw a separator to avoid visual clash with Add Row
                               header_row.col(|_ui| {
                                   // Intentionally empty: no line under the left control column header
                               });
                               // Render ancestor key headers (green, read-only)
                               for (key_header, value) in &ancestor_key_columns {
                                   header_row.col(|ui| {
                                       let r = ui.colored_label(egui::Color32::from_rgb(0, 170, 0), key_header);
                                       if !value.is_empty() { r.on_hover_text(format!("Key value: {}", value)); } else { r.on_hover_text(format!("Key column: {}", key_header)); }
                                       // bottom separator under each header cell
                                       let rect = ui.max_rect();
                                       let y = rect.bottom();
                                       ui.painter().hline(rect.x_range(), y, ui.visuals().widgets.noninteractive.bg_stroke);
                                   });
                               }
                               // Render regular headers using existing helper
                               sheet_table_header(&mut header_row, ctx, metadata, selected_name, state, reorder_column_writer);
                            })
                           .body(|body: TableBody| {
                               if ancestor_key_columns.is_empty() {
                                   // Render standard body with the control column prepended
                                   let sheet_ref = registry.get_sheet(&current_category_clone, selected_name).unwrap();
                                   let meta_ref = sheet_ref.metadata.as_ref().unwrap();
                                   let grid = &sheet_ref.grid;
                                   use crate::sheets::definitions::ColumnValidator;
                                   let validators: Vec<Option<ColumnValidator>> = meta_ref.columns.iter().map(|c| c.validator.clone()).collect();
                                   // Filtering
                                   let filters: Vec<Option<String>> = meta_ref.columns.iter().map(|c| c.filter.clone()).collect();
                                   let filtered_indices: Vec<usize> = if filters.iter().all(|f| f.is_none()) {
                                       (0..grid.len()).collect()
                                   } else {
                                       (0..grid.len()).filter(|row_idx| {
                                           if let Some(row) = grid.get(*row_idx) {
                                               // AND across columns, OR within a column (terms separated by '|')
                                               filters.iter().enumerate().all(|(col_idx, filter_opt)| {
                                                   match filter_opt {
                                                       Some(txt) if !txt.is_empty() => {
                                                           row.get(col_idx).map_or(false, |cell| {
                                                               let cell_lower = cell.to_lowercase();
                                                               let terms: Vec<&str> = txt
                                                                   .split('|')
                                                                   .map(|s| s.trim())
                                                                   .filter(|s| !s.is_empty())
                                                                   .collect();
                                                               if terms.is_empty() { return true; }
                                                               terms.iter().any(|t| {
                                                                   let term_lower = t.to_lowercase();
                                                                   cell_lower.contains(&term_lower)
                                                               })
                                                           })
                                                       }
                                                       _ => true,
                                                   }
                                               })
                                           } else { false }
                                       }).collect()
                                   };

                                   body.rows(row_height, filtered_indices.len(), |mut row| {
                                       let idx_in_list = row.index();
                                       let original_row_index = *filtered_indices.get(idx_in_list).unwrap_or(&0);

                                       // Left control cell
                                       row.col(|ui| {
                                           let ai_preparing = state.current_interaction_mode == SheetInteractionState::AiModeActive && state.ai_mode == AiModeState::Preparing;
                                           if state.current_interaction_mode == SheetInteractionState::DeleteModeActive || ai_preparing {
                                               let mut is_selected = state.ai_selected_rows.contains(&original_row_index);
                                               let response = ui.add(egui::Checkbox::without_text(&mut is_selected));
                                               if response.changed() {
                                                   if is_selected { state.ai_selected_rows.insert(original_row_index); } else { state.ai_selected_rows.remove(&original_row_index); }
                                               }
                                           } else {
                                               ui.allocate_exact_size(egui::vec2(18.0, row_height), egui::Sense::hover());
                                           }
                                       });

                                       if let Some(row_data) = grid.get(original_row_index) {
                                           if row_data.len() != num_cols { row.col(|ui| { ui.colored_label(egui::Color32::RED, "Row Len Err"); }); return; }
                                           for c_idx in 0..num_cols { row.col(|ui| {
                                               let validator_opt = validators.get(c_idx).cloned().flatten();
                                               let cell_id = egui::Id::new("cell")
                                                   .with(current_category_clone.as_deref().unwrap_or("root"))
                                                   .with(selected_name)
                                                   .with(original_row_index)
                                                   .with(c_idx);
                                               if let Some(new_value) = crate::ui::common::edit_cell_widget(
                                                   ui,
                                                   cell_id,
                                                   &validator_opt,
                                                   &current_category_clone,
                                                   selected_name,
                                                   original_row_index,
                                                   c_idx,
                                                   registry,
                                                   render_cache,
                                                   state,
                                                   &mut open_structure_writer,
                                               ) {
                                                   cell_update_writer.write(crate::sheets::events::UpdateCellEvent { category: current_category_clone.clone(), sheet_name: selected_name.to_string(), row_index: original_row_index, col_index: c_idx, new_value });
                                               }
                                           }); }
                                       } else { row.col(|ui| { ui.colored_label(egui::Color32::RED, "Row Idx Err"); }); }
                                   });
                               } else {
                                   // Wrap body to inject control + prefix columns per row
                                   let inner_body = body;
                                   let original_category = current_category_clone.clone();
                                   use crate::sheets::definitions::ColumnValidator;
                                   let sheet_ref = registry.get_sheet(&original_category, selected_name).unwrap();
                                   let meta_ref = sheet_ref.metadata.as_ref().unwrap();
                                   let grid = &sheet_ref.grid;
                                   let validators: Vec<Option<ColumnValidator>> = meta_ref.columns.iter().map(|c| c.validator.clone()).collect();
                                   let filters: Vec<Option<String>> = meta_ref.columns.iter().map(|c| c.filter.clone()).collect();
                                   let filtered_indices: Vec<usize> = if filters.iter().all(|f| f.is_none()) {
                                       (0..grid.len()).collect()
                                   } else {
                                       (0..grid.len()).filter(|row_idx| {
                                           if let Some(row) = grid.get(*row_idx) {
                                               // AND across columns, OR within a column (terms separated by '|')
                                               filters.iter().enumerate().all(|(col_idx, filter_opt)| {
                                                   match filter_opt {
                                                       Some(txt) if !txt.is_empty() => {
                                                           row.get(col_idx).map_or(false, |cell| {
                                                               let cell_lower = cell.to_lowercase();
                                                               let terms: Vec<&str> = txt
                                                                   .split('|')
                                                                   .map(|s| s.trim())
                                                                   .filter(|s| !s.is_empty())
                                                                   .collect();
                                                               if terms.is_empty() { return true; }
                                                               terms.iter().any(|t| {
                                                                   let term_lower = t.to_lowercase();
                                                                   cell_lower.contains(&term_lower)
                                                               })
                                                           })
                                                       }
                                                       _ => true,
                                                   }
                                               })
                                           } else { false }
                                       }).collect()
                                   };
                                   let num_cols_local = meta_ref.columns.len();
                                   inner_body.rows(row_height, filtered_indices.len(), |mut row| {
                                       let idx_in_list = row.index();
                                       let original_row_index = *filtered_indices.get(idx_in_list).unwrap_or(&0);

                                       // Left control cell
                                       row.col(|ui| {
                                           let ai_preparing = state.current_interaction_mode == SheetInteractionState::AiModeActive && state.ai_mode == AiModeState::Preparing;
                                           if state.current_interaction_mode == SheetInteractionState::DeleteModeActive || ai_preparing {
                                               let mut is_selected = state.ai_selected_rows.contains(&original_row_index);
                                               let response = ui.add(egui::Checkbox::without_text(&mut is_selected));
                                               if response.changed() {
                                                   if is_selected { state.ai_selected_rows.insert(original_row_index); } else { state.ai_selected_rows.remove(&original_row_index); }
                                               }
                                           } else {
                                               ui.allocate_exact_size(egui::vec2(18.0, row_height), egui::Sense::hover());
                                           }
                                       });

                                       // Prefix key columns
                                       for (_, value) in &ancestor_key_columns {
                                           row.col(|ui| { ui.colored_label(egui::Color32::from_rgb(0, 150, 0), value); });
                                       }
                                       if let Some(row_data) = grid.get(original_row_index) {
                                           if row_data.len() != num_cols_local { row.col(|ui| { ui.colored_label(egui::Color32::RED, "Row Len Err"); }); return; }
                                           for c_idx in 0..num_cols_local { row.col(|ui| {
                                               let validator_opt = validators.get(c_idx).cloned().flatten();
                                               let cell_id = egui::Id::new("cell")
                                                   .with(original_category.as_deref().unwrap_or("root"))
                                                   .with(selected_name)
                                                   .with(original_row_index)
                                                   .with(c_idx);
                                               if let Some(new_value) = crate::ui::common::edit_cell_widget(
                                                   ui,
                                                   cell_id,
                                                   &validator_opt,
                                                   &original_category,
                                                   selected_name,
                                                   original_row_index,
                                                   c_idx,
                                                   registry,
                                                   render_cache,
                                                   state,
                                                   &mut open_structure_writer,
                                               ) {
                                                   cell_update_writer.write(crate::sheets::events::UpdateCellEvent { category: original_category.clone(), sheet_name: selected_name.to_string(), row_index: original_row_index, col_index: c_idx, new_value });
                                               }
                                           }); }
                                       } else { row.col(|ui| { ui.colored_label(egui::Color32::RED, "Row Idx Err"); }); }
                                   });
                               }
                           });

                    });

                // Floating overlay controls (outside ScrollArea so they don't clip over content)
                let header_h = row_height; // must match header height above
                let any_toolbox_open = state.current_interaction_mode != SheetInteractionState::Idle || state.show_toybox_menu;
                let add_controls_visible = !any_toolbox_open;
                if add_controls_visible {
                    let scroll_rect = scroll_resp.inner_rect;
                    // Add Row: place on the delimiter (bottom of header), near left edge of the table
                    let pos_left = egui::pos2(table_start_pos.x + 6.0, table_start_pos.y + header_h - 9.0);
                    egui::Area::new("floating_add_row_btn".into()).order(egui::Order::Foreground).fixed_pos(pos_left).show(ctx, |ui_f| {
                        let btn = egui::Button::new("+").min_size(egui::vec2(18.0, 18.0));
                        if ui_f.add(btn).on_hover_text("Simple Add Row").clicked() {
                            if let Some(sheet_name) = &state.selected_sheet_name {
                                add_row_writer.write(AddSheetRowRequest { category: state.selected_category.clone(), sheet_name: sheet_name.clone(), initial_values: None });
                            }
                        }
                    });

                    // Add Column placement:
                    // - If no horizontal scrolling: place right after the last header delimiter
                    // - If horizontal scrolling exists: place at the rightmost edge of the viewport
                    let has_h_scroll = scroll_resp.inner_rect.width() < scroll_resp.content_size.x;
                    // Estimate rightmost delimiter position: when no horizontal scroll, use table_start_pos.x + scroll content width
                    let right_x = if !has_h_scroll {
                        // content_size.x is approximate full table width; keep 6px gap
                        table_start_pos.x + scroll_resp.content_size.x + 6.0
                    } else {
                        // viewport right with padding
                        scroll_rect.right() + 12.0
                    };
                    let pos_right = egui::pos2(right_x, table_start_pos.y + 2.0);
                    egui::Area::new("floating_add_col_btn".into()).order(egui::Order::Foreground).fixed_pos(pos_right).show(ctx, |ui_f| {
                        let btn = egui::Button::new("+").min_size(egui::vec2(18.0, 18.0));
                        if ui_f.add(btn).on_hover_text("Add Column").clicked() {
                            if let Some(sheet_name) = &state.selected_sheet_name {
                                add_column_writer.write(RequestAddColumn { category: state.selected_category.clone(), sheet_name: sheet_name.clone() });
                            }
                        }
                    });
                }
            } else {
                 warn!("Metadata object missing for sheet '{:?}/{}' even though sheet data exists.", current_category_clone, selected_name);
                 ui.colored_label(egui::Color32::YELLOW, format!("Metadata missing for sheet '{:?}/{}'.", current_category_clone, selected_name));
            }
        }
    } else {
         if state.selected_category.is_some() {
            ui.vertical_centered(|ui| { ui.label("Select a sheet from the category, or upload a new one."); });
         }
         else {
            ui.vertical_centered(|ui| { ui.label("Select a category and sheet, or upload JSON."); });
         }
    }
    // Restore selection if we temporarily switched to virtual
    if let Some(vctx) = state.virtual_structure_stack.last() {
        if let Some(orig_sheet) = &backup_sheet { if *orig_sheet != vctx.virtual_sheet_name { state.selected_sheet_name = backup_sheet; } }
    } else {
        // No virtual sheet active, ensure selection remains original
        state.selected_sheet_name = backup_sheet;
    }
}