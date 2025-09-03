// src/ui/elements/editor/editor_sheet_display.rs
use bevy::prelude::*;
use bevy_egui::egui; // EguiContexts might not be needed here if ctx is passed
use egui_extras::{Column, TableBody, TableBuilder};
use crate::sheets::{
    resources::{SheetRegistry, SheetRenderCache},
    events::{RequestReorderColumn, UpdateCellEvent, OpenStructureViewEvent},
};
use crate::ui::elements::editor::state::{EditorWindowState, AiModeState, SheetInteractionState};
use crate::ui::elements::editor::table_header::sheet_table_header;
use crate::ui::elements::editor::table_body::sheet_table_body;
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
                                        if let Some(key_col_idx) = struct_col_def.structure_key_parent_column_index {
                                            if let Some(key_col_def) = parent_meta.columns.get(key_col_idx) {
                                                let value = parent_row.get(key_col_idx).cloned().unwrap_or_default();
                                                // Store actual key column header as first element; value as second.
                                                ancestor_key_columns.push((key_col_def.header.clone(), value));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                egui::ScrollArea::both()
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
                            table_builder = table_builder.column(Column::remainder().resizable(false));
                       } else {
                            // Build prefix (read-only key) columns first
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
                           .header(20.0, |mut header_row| {
                               // Render ancestor key headers (green, read-only)
                               for (key_header, value) in &ancestor_key_columns {
                                   header_row.col(|ui| {
                                       let r = ui.colored_label(egui::Color32::from_rgb(0, 170, 0), key_header);
                                       if !value.is_empty() { r.on_hover_text(format!("Key value: {}", value)); } else { r.on_hover_text(format!("Key column: {}", key_header)); }
                                   });
                               }
                               // Render regular headers using existing helper
                               sheet_table_header(&mut header_row, ctx, metadata, selected_name, state, reorder_column_writer);
                            })
                           .body(|body: TableBody| {
                               if ancestor_key_columns.is_empty() {
                                   sheet_table_body(body, row_height, &current_category_clone, selected_name, registry, render_cache, cell_update_writer, state, &mut open_structure_writer);
                               } else {
                                   // Wrap body to inject prefix columns per row
                                   let inner_body = body;
                                   let original_category = current_category_clone.clone();
                                   // Reuse logic from sheet_table_body by manually iterating rows (can't easily intercept inside)
                                   // We'll replicate minimal needed rendering for prefix columns then delegate per-cell editing.
                                   use crate::sheets::definitions::ColumnValidator;
                                   let sheet_ref = registry.get_sheet(&original_category, selected_name).unwrap();
                                   let meta_ref = sheet_ref.metadata.as_ref().unwrap();
                                   let grid = &sheet_ref.grid;
                                   let validators: Vec<Option<ColumnValidator>> = meta_ref.columns.iter().map(|c| c.validator.clone()).collect();
                                   // Filtering logic reuse: leverage existing filtered cache by calling helper indirectly via sheet_table_body? Instead recompute quickly.
                                   let filters: Vec<Option<String>> = meta_ref.columns.iter().map(|c| c.filter.clone()).collect();
                                   let mut filtered_indices: Vec<usize> = if filters.iter().all(|f| f.is_none()) { (0..grid.len()).collect() } else { (0..grid.len()).filter(|row_idx| {
                                       if let Some(row) = grid.get(*row_idx) { filters.iter().enumerate().all(|(col_idx, filter_opt)| {
                                           match filter_opt { Some(txt) if !txt.is_empty() => row.get(col_idx).map_or(false, |cell| cell.to_lowercase().contains(&txt.to_lowercase())), _ => true }
                                       }) } else { false }
                                   }).collect() };
                                   if filtered_indices.is_empty() && !grid.is_empty() { filtered_indices = Vec::new(); }
                                   let num_cols_local = meta_ref.columns.len();
                                   inner_body.rows(row_height, filtered_indices.len(), |mut row| {
                                       let idx_in_list = row.index();
                                       let original_row_index = *filtered_indices.get(idx_in_list).unwrap_or(&0);
                                       // Determine if we show selection checkbox (AI preparing or delete mode)
                                       let show_checkbox = (state.current_interaction_mode == SheetInteractionState::AiModeActive && state.ai_mode == AiModeState::Preparing) || (state.current_interaction_mode == SheetInteractionState::DeleteModeActive);
                                       // Prefix key columns (same value for all rows). Place checkbox in the very first prefix column if needed.
                                       for (p_idx, (_, value)) in ancestor_key_columns.iter().enumerate() {
                                           row.col(|ui| {
                                               if p_idx == 0 && show_checkbox {
                                                   let mut is_selected = state.ai_selected_rows.contains(&original_row_index);
                                                   let response = ui.add(egui::Checkbox::without_text(&mut is_selected));
                                                   if response.changed() {
                                                       if is_selected { state.ai_selected_rows.insert(original_row_index); } else { state.ai_selected_rows.remove(&original_row_index); }
                                                   }
                                                   ui.add_space(2.0); ui.separator(); ui.add_space(2.0);
                                               }
                                               ui.colored_label(egui::Color32::from_rgb(0, 150, 0), value);
                                           });
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