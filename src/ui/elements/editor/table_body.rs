// src/ui/elements/editor/table_body.rs
use crate::sheets::{
    definitions::{ColumnValidator, SheetMetadata},
    events::{
        OpenStructureViewEvent, RequestCopyCell, RequestPasteCell, RequestToggleAiRowGeneration,
        UpdateCellEvent,
    },
    resources::{ClipboardBuffer, SheetRegistry, SheetRenderCache},
};
// MODIFIED: Import SheetInteractionState
use crate::ui::common::edit_cell_widget;
use crate::ui::elements::editor::state::{
    AiModeState, EditorWindowState, FilteredRowsCacheEntry, SheetInteractionState,
};
use crate::ui::validation::normalize_for_link_cmp;
use bevy::log::{debug, error, warn};
use bevy::prelude::*;
use bevy_egui::egui;
use egui_extras::{TableBody, TableRow};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[allow(dead_code)]
fn calculate_filters_hash(filters: &Vec<Option<String>>) -> u64 {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    filters.hash(&mut s);
    s.finish()
}

#[allow(dead_code)]
fn get_filtered_row_indices_internal(grid: &[Vec<String>], metadata: &SheetMetadata) -> Vec<usize> {
    let filters: Vec<Option<String>> = metadata.columns.iter().map(|c| c.filter.clone()).collect();
    if filters.iter().all(Option::is_none) {
        return (0..grid.len()).collect();
    }

    (0..grid.len())
        .filter(|&row_idx| {
            if let Some(row) = grid.get(row_idx) {
                filters.iter().enumerate().all(|(col_idx, filter_opt)| {
                    match filter_opt {
                        Some(filter_text) if !filter_text.is_empty() => {
                            // OR semantics across '|' separated terms (case-insensitive)
                            let terms: Vec<&str> = filter_text
                                .split('|')
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                                .collect();
                            if terms.is_empty() {
                                return true;
                            }
                            row.get(col_idx).map_or(false, |cell_text| {
                                let cell_normalized = normalize_for_link_cmp(cell_text);
                                terms.iter().any(|t| {
                                    let term_normalized = normalize_for_link_cmp(t);
                                    cell_normalized.contains(&term_normalized)
                                })
                            })
                        }
                        _ => true,
                    }
                })
            } else {
                false
            }
        })
        .collect()
}

pub(crate) fn get_filtered_row_indices_cached(
    state: &mut EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
    grid: &[Vec<String>],
    metadata: &SheetMetadata,
) -> Arc<Vec<usize>> {
    let cache_key = (category.clone(), sheet_name.to_string());

    // Check if we're in a structure navigation context for this sheet
    let structure_filter = state.structure_navigation_stack.last().filter(|nav_ctx| {
        &nav_ctx.structure_sheet_name == sheet_name && category == &nav_ctx.parent_category
    });

    if !state.force_filter_recalculation {
        if let Some(entry) = state.filtered_row_indices_cache.get(&cache_key) {
            // If we're in structure navigation, we need to recalculate with the hidden filter
            if structure_filter.is_none() {
                return Arc::clone(&entry.rows);
            }
        }
    }

    let active_filters = metadata.get_filters();
    let filters_hash = calculate_filters_hash(&active_filters);
    let total_rows = grid.len();

    if let Some(entry) = state.filtered_row_indices_cache.get(&cache_key) {
        if entry.filters_hash == filters_hash
            && entry.total_rows == total_rows
            && structure_filter.is_none()
        {
            state.force_filter_recalculation = false;
            return Arc::clone(&entry.rows);
        }
    }

    debug!(
        "Recalculating filtered indices for '{:?}/{}' (hash: {}, force_recalc: {}, structure_filter: {})",
        category, sheet_name, filters_hash, state.force_filter_recalculation, structure_filter.is_some()
    );

    let mut indices = get_filtered_row_indices_internal(grid, metadata);

    // Apply hidden structure filter if present
    // For multi-level structures, we need to match ALL parent keys (grand_N_parent, parent_key)
    if let Some(nav_ctx) = structure_filter {
        indices = indices
            .into_iter()
            .filter(|&row_idx| {
                let row = match grid.get(row_idx) {
                    Some(r) => r,
                    None => return false,
                };

                // Get parent_key column (usually column 1, but could vary with grands)
                // Find it by checking metadata
                let parent_key_col_idx = metadata
                    .columns
                    .iter()
                    .position(|c| c.header.eq_ignore_ascii_case("parent_key"))
                    .unwrap_or(1); // Default fallback

                // Check parent_key matches
                let parent_key_matches = row
                    .get(parent_key_col_idx)
                    .map(|pk| {
                        let matches = pk == &nav_ctx.parent_row_key;
                        if !matches {
                            bevy::log::debug!(
                                "Filtering row_idx={}: parent_key mismatch - row has '{}', expected '{}'",
                                row_idx, pk, nav_ctx.parent_row_key
                            );
                        } else {
                            bevy::log::debug!(
                                "Filtering row_idx={}: parent_key MATCH - '{}'",
                                row_idx, pk
                            );
                        }
                        matches
                    })
                    .unwrap_or(false);

                if !parent_key_matches {
                    return false;
                }

                // Check all ancestor keys match (grand_N_parent columns)
                // ancestor_keys is ordered: [deepest ancestor, ..., immediate parent's parent_key]
                // For depth=3: ancestor_keys = [grand_2_value, grand_1_value, parent_key_value]
                // We iterate columns sequentially and match against ancestor_keys in order
                if !nav_ctx.ancestor_keys.is_empty() {
                    let mut ancestor_idx = 0;
                    
                    // Iterate through columns and match grand_N_parent columns against ancestor_keys sequentially
                    for (col_idx, col) in metadata.columns.iter().enumerate() {
                        if col.header.starts_with("grand_") && col.header.ends_with("_parent") {
                            // Match this grand column with the next ancestor_key
                            if ancestor_idx < nav_ctx.ancestor_keys.len() {
                                let expected_key = &nav_ctx.ancestor_keys[ancestor_idx];
                                if let Some(row_grand_key) = row.get(col_idx) {
                                    if row_grand_key != expected_key {
                                        bevy::log::debug!(
                                            "Filtering row_idx={}: {} mismatch - expected '{}' (ancestor_keys[{}]), got '{}'",
                                            row_idx, col.header, expected_key, ancestor_idx, row_grand_key
                                        );
                                        return false; // Mismatch
                                    } else {
                                        bevy::log::debug!(
                                            "Filtering row_idx={}: {} match - '{}' == ancestor_keys[{}]",
                                            row_idx, col.header, row_grand_key, ancestor_idx
                                        );
                                    }
                                } else {
                                    bevy::log::debug!(
                                        "Filtering row_idx={}: {} column missing in row data",
                                        row_idx, col.header
                                    );
                                    return false; // Column missing
                                }
                                ancestor_idx += 1;
                            }
                        }
                    }
                }

                true
            })
            .collect();
    }

    // Rule: if a row was just added (row 0), do not filter it out until UI processes the add
    // Include row 0 temporarily while request_scroll_to_new_row is set
    if state.request_scroll_to_new_row && !grid.is_empty() && !indices.contains(&0) {
        indices.insert(0, 0);
        indices.sort_unstable();
    }

    let indices = Arc::new(indices);
    state.filtered_row_indices_cache.insert(
        cache_key,
        FilteredRowsCacheEntry {
            rows: Arc::clone(&indices),
            filters_hash,
            total_rows,
        },
    );
    state.force_filter_recalculation = false;
    indices
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub fn sheet_table_body(
    mut body: TableBody,
    row_height: f32,
    category: &Option<String>,
    sheet_name: &str,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    mut cell_update_writer: EventWriter<UpdateCellEvent>,
    state: &mut EditorWindowState,
    open_structure_events: &mut EventWriter<OpenStructureViewEvent>,
    toggle_ai_events: &mut EventWriter<RequestToggleAiRowGeneration>,
    copy_events: &mut EventWriter<RequestCopyCell>,
    paste_events: &mut EventWriter<RequestPasteCell>,
    clipboard_buffer: &ClipboardBuffer,
) -> bool {
    let sheet_data_ref = match registry.get_sheet(category, sheet_name) {
        Some(data) => data,
        None => {
            warn!(
                "Sheet '{:?}/{}' not found in registry for table_body.",
                category, sheet_name
            );
            body.row(row_height, |mut row| {
                row.col(|ui| {
                    ui.label("Sheet missing");
                });
            });
            return true;
        }
    };

    let metadata_ref = match &sheet_data_ref.metadata {
        Some(meta) => meta,
        None => {
            warn!(
                "Sheet '{:?}/{}' found but metadata missing in table_body",
                category, sheet_name
            );
            body.row(row_height, |mut row| {
                row.col(|ui| {
                    ui.label("Metadata missing");
                });
            });
            return true;
        }
    };

    let grid_data = &sheet_data_ref.grid;
    // Now include structure columns - they will be rendered as buttons
    let num_cols = metadata_ref.columns.len();
    let validators: Vec<Option<ColumnValidator>> = metadata_ref
        .columns
        .iter()
        .map(|c| c.validator.clone())
        .collect();

    // Get visible column indices (respects hidden flag on columns)
    let visible_columns = state.get_visible_column_indices(category, sheet_name, metadata_ref);

    let filtered_indices =
        get_filtered_row_indices_cached(state, category, sheet_name, grid_data, metadata_ref);

    if num_cols == 0 && !grid_data.is_empty() {
        body.row(row_height, |mut row| {
            row.col(|ui| {
                ui.label("(No columns)");
            });
        });
        return false;
    } else if filtered_indices.is_empty() && !grid_data.is_empty() {
        body.row(row_height, |mut row| {
            row.col(|ui| {
                ui.label("(No rows match filter)");
            });
        });
        return false;
    } else if grid_data.is_empty() {
        body.row(row_height, |mut row| {
            row.col(|ui| {
                ui.label("(Sheet is empty)");
            });
        });
        return false;
    }

    let num_filtered_rows = filtered_indices.len();

    state.ensure_ai_included_columns_cache(registry, category, sheet_name);

    body.rows(
        row_height,
    num_filtered_rows,
        |mut ui_row: TableRow| {
            let filtered_row_index_in_list = ui_row.index();
            let original_row_index =
                match filtered_indices.get(filtered_row_index_in_list) {
                    Some(&idx) => idx,
                    None => {
                        error!("Filtered index out of bounds! List index: {}, List len: {}", filtered_row_index_in_list, filtered_indices.len());
                        ui_row.col(|ui| { ui.colored_label(egui::Color32::RED, "Err"); });
                        return;
                    }
                };

            let current_row_actual_data_ref_opt = grid_data.get(original_row_index);

            if let Some(current_row_actual_data_ref) = current_row_actual_data_ref_opt {
                 if current_row_actual_data_ref.len() != num_cols {
                    ui_row.col(|ui| {
                        ui.colored_label(egui::Color32::RED, format!("Row Len Err ({} vs {})", current_row_actual_data_ref.len(), num_cols));
                    });
                    warn!("Row length mismatch in sheet '{:?}/{}', row {}: Expected {}, found {}", category, sheet_name, original_row_index, num_cols, current_row_actual_data_ref.len());
                    return;
                }

                for (col_display_idx, c_idx) in visible_columns.iter().copied().enumerate() {
                    ui_row.col(|ui| {
                        // MODIFIED: Use current_interaction_mode to determine if checkboxes are shown
                        let show_checkbox =
                            (state.current_interaction_mode == SheetInteractionState::AiModeActive && state.ai_mode == AiModeState::Preparing) ||
                            matches!(state.current_interaction_mode, SheetInteractionState::DeleteModeActive);

                        if col_display_idx == 0 && show_checkbox {
                            let is_selected = state.ai_selected_rows.contains(&original_row_index);
                            let mut checkbox_state = is_selected;
                            let response = ui.add(egui::Checkbox::without_text(&mut checkbox_state));
                            if response.changed() {
                                // Update the HashSet to match the NEW checkbox state
                                if checkbox_state {
                                    state.ai_selected_rows.insert(original_row_index);
                                } else {
                                    state.ai_selected_rows.remove(&original_row_index);
                                }
                            }
                            // Right-click context menu for bulk row selection on visible rows
                            response.context_menu(|menu_ui| {
                                if menu_ui.button("Select all visible rows").clicked() {
                                    for &ri in filtered_indices.iter() { state.ai_selected_rows.insert(ri); }
                                    menu_ui.close_menu();
                                    ui.ctx().request_repaint();
                                }
                                if menu_ui.button("Remove selection from all visible rows").clicked() {
                                    for &ri in filtered_indices.iter() { state.ai_selected_rows.remove(&ri); }
                                    menu_ui.close_menu();
                                    ui.ctx().request_repaint();
                                }
                            });
                        }

                        let validator_opt_for_cell = validators.get(c_idx).cloned().flatten();
                        let cell_id = egui::Id::new("cell")
                            .with(category.as_deref().unwrap_or("root"))
                            .with(sheet_name)
                            .with(original_row_index)
                            .with(c_idx);

                        if let Some(new_value) = edit_cell_widget(
                            ui,
                            cell_id,
                            &validator_opt_for_cell,
                            category,
                            sheet_name,
                            original_row_index,
                            c_idx,
                            registry,
                            render_cache,
                            state,
                            open_structure_events,
                            toggle_ai_events,
                            copy_events,
                            paste_events,
                            clipboard_buffer,
                        ) {
                            cell_update_writer.write(UpdateCellEvent {
                                category: category.clone(),
                                sheet_name: sheet_name.to_string(),
                                row_index: original_row_index,
                                col_index: c_idx,
                                new_value: new_value,
                            });
                        }
                    });
                }
            } else {
                ui_row.col(|ui| { ui.colored_label(egui::Color32::RED, "Row Idx Err"); });
                error!("Original row index {} out of bounds for grid_data (len {}) in sheet '{:?}/{}' during table_body rendering.", original_row_index, grid_data.len(), category, sheet_name);
            }
        },
    );
    false
}
