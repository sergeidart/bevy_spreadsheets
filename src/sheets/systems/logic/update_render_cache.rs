// src/sheets/systems/logic/update_render_cache.rs
//! Main render cache update system
//!
//! This system listens for various events and updates the SheetRenderCache accordingly.
//! For large tables (>500 rows), it uses lazy resolution to provide instant responsiveness.

use bevy::prelude::*;
use std::collections::HashSet;

use crate::{
    sheets::{
        definitions::{ColumnDataType, ColumnValidator},
        events::{
            RequestDeleteSheet,
            RequestRenameSheet,
            RequestSheetRevalidation,
            SheetDataModifiedInRegistryEvent,
        },
        resources::{SheetRegistry, SheetRenderCache},
        systems::logic::generate_structure_preview,
    },
    ui::{
        elements::editor::state::EditorWindowState,
        validation::{validate_basic_cell, validate_linked_cell, ValidationState},
    },
};

// Import from submodules
mod ancestor_resolution;
mod parent_lookup_cache;

use ancestor_resolution::{resolve_ancestor_key_display_text, resolve_ancestor_key_with_cache};
use parent_lookup_cache::build_parent_lookup_cache;

/// System that listens for various events and updates the SheetRenderCache.
/// This effectively replaces the old handle_sheet_revalidation_request system.
#[allow(clippy::too_many_arguments)]
pub fn handle_sheet_render_cache_update(
    // Event Readers:
    mut ev_revalidate: EventReader<RequestSheetRevalidation>,
    mut ev_data_modified: EventReader<SheetDataModifiedInRegistryEvent>,
    mut ev_sheet_deleted: EventReader<RequestDeleteSheet>, // To clear cache
    mut ev_sheet_renamed: EventReader<RequestRenameSheet>, // To rename cache entry
    mut ev_cache_renamed: EventReader<crate::sheets::events::RequestRenameCacheEntry>, // Internal cache rename
    // Consider adding readers for AddSheetRowRequest, RequestDeleteRows,
    // RequestUpdateColumnName, RequestUpdateColumnValidator if they
    // don't already fire SheetDataModifiedInRegistryEvent or RequestSheetRevalidation appropriately.
    // For now, we assume those events lead to one of the above.

    // Resources:
    registry: Res<SheetRegistry>,
    mut render_cache: ResMut<SheetRenderCache>,
    // Local state for editor, used by validate_linked_cell for its own cache.
    mut editor_state: Local<EditorWindowState>,
    // Access to global UI state (for debug toggles like show_hidden_sheets)
    current_editor_state: Option<Res<EditorWindowState>>,
) {
    let mut sheets_to_rebuild: HashSet<(Option<String>, String)> = HashSet::new();

    // 1. Collect sheets explicitly requested for revalidation
    for event in ev_revalidate.read() {
        sheets_to_rebuild.insert((event.category.clone(), event.sheet_name.clone()));
    }

    // 2. Collect sheets where data was modified
    for event in ev_data_modified.read() {
        sheets_to_rebuild.insert((event.category.clone(), event.sheet_name.clone()));
    }

    // Handle cache clearing for deleted sheets
    for event in ev_sheet_deleted.read() {
        render_cache.clear_sheet_render_data(&event.category, &event.sheet_name);
        // Also remove from sheets_to_rebuild if it was marked, as it no longer exists
        sheets_to_rebuild.remove(&(event.category.clone(), event.sheet_name.clone()));
        debug!(
            "Cleared render cache for deleted sheet: '{:?}/{}'",
            event.category, event.sheet_name
        );
    }

    // Handle cache renaming for renamed sheets
    for event in ev_sheet_renamed.read() {
        render_cache.rename_sheet_render_data(&event.category, &event.old_name, &event.new_name);
        // If the old name was in sheets_to_rebuild, remove it and add the new one.
        if sheets_to_rebuild.remove(&(event.category.clone(), event.old_name.clone())) {
            sheets_to_rebuild.insert((event.category.clone(), event.new_name.clone()));
        }
        debug!(
            "Renamed render cache entry: '{:?}/{}' -> '{:?}/{}'",
            event.category, event.old_name, event.category, event.new_name
        );
    }

    // Handle internal cache rename requests (do not trigger full rename logic)
    for event in ev_cache_renamed.read() {
        render_cache.rename_sheet_render_data(&event.category, &event.old_name, &event.new_name);
        if sheets_to_rebuild.remove(&(event.category.clone(), event.old_name.clone())) {
            sheets_to_rebuild.insert((event.category.clone(), event.new_name.clone()));
        }
        debug!(
            "Renamed render cache entry (internal): '{:?}/{}' -> '{:?}/{}'",
            event.category, event.old_name, event.category, event.new_name
        );
    }

    if sheets_to_rebuild.is_empty() {
        return;
    }

    debug!(
        "Rebuilding render cache for sheets: {:?}",
        sheets_to_rebuild
    );

    for (category, sheet_name) in sheets_to_rebuild {
        if let Some(sheet_data) = registry.get_sheet(&category, &sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_rows = sheet_data.grid.len();
                let num_cols = metadata.columns.len();

                // Get a mutable reference to the sheet's cache, ensuring dimensions
                let current_sheet_render_cache = render_cache.ensure_and_get_sheet_cache_mut(
                    &category,
                    &sheet_name,
                    num_rows,
                    num_cols,
                );

                // Phase 1: Fast initial population (no ancestor resolution)
                // For large tables, this gives instant responsiveness
                let has_ancestor_columns = metadata.columns.iter().any(|col| {
                    col.header.eq_ignore_ascii_case("parent_key")
                });

                let use_lazy_resolution = num_rows > 500 && has_ancestor_columns;
                let show_raw_ancestor_keys = current_editor_state
                    .as_ref()
                    .map(|s| s.show_hidden_sheets)
                    .unwrap_or(false);

                if use_lazy_resolution {
                    debug!("Using lazy resolution for large table '{:?}/{}' ({} rows)",
                           category, sheet_name, num_rows);
                }

                for r_idx in 0..num_rows {
                    for c_idx in 0..num_cols {
                        let cell_value_str = sheet_data
                            .grid
                            .get(r_idx)
                            .and_then(|row| row.get(c_idx))
                            .map(|s| s.as_str())
                            .unwrap_or(""); // Default to empty string if out of bounds

                        let col_def_opt = metadata.columns.get(c_idx);
                        // let mut validation_state_for_cell = ValidationState::default();
                        // let mut display_text_for_cell = cell_value_str.to_string(); // Default

                        if let Some(col_def) = col_def_opt {
                            // Perform validation
                            // Remove unused val_state assignment
                            let (_val_state, _allowed_values_opt) = match &col_def.validator {
                                Some(ColumnValidator::Basic(data_type)) => {
                                    let (state, _parse_error) =
                                        validate_basic_cell(cell_value_str, *data_type);
                                    (state, None)
                                }
                                Some(ColumnValidator::Linked {
                                    target_sheet_name,
                                    target_column_index,
                                }) => {
                                    validate_linked_cell(
                                        cell_value_str,
                                        target_sheet_name,
                                        *target_column_index,
                                        &registry,
                                        &mut editor_state, // Pass Local state mutably for linked_column_cache access
                                    )
                                }
                                Some(ColumnValidator::Structure) => {
                                    // Treat structure cells as always valid (content is JSON string) for now
                                    (ValidationState::Valid, None)
                                }
                                None => {
                                    // Treat as basic string if no validator
                                    let (state, _parse_error) =
                                        validate_basic_cell(cell_value_str, ColumnDataType::String);
                                    (state, None)
                                }
                            };
                            // Directly use validation in the render_cell assignment below
                        } else {
                            error!("Render Cache: Metadata column index mismatch for sheet '{:?}/{}' at column {}", category, sheet_name, c_idx);
                            // Directly use ValidationState::Invalid below
                        }

                        // Update the specific cell in the render cache
                        if let Some(render_cell) = current_sheet_render_cache
                            .get_mut(r_idx)
                            .and_then(|row| row.get_mut(c_idx))
                        {
                            // Custom preview for structure cells: friendly label using parent key and column name
                            let preview_text = if let Some(col_def) = col_def_opt {
                                if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                                    // Try to form "<Key> <ColumnName>" if possible
                                    let mut label: Option<String> = None;
                                    if let Some(parent_sheet) = registry.get_sheet(&category, &sheet_name) {
                                        if let Some(parent_row) = parent_sheet.grid.get(r_idx) {
                                            // Prefer configured key index; otherwise compute first non-technical column dynamically
                                            let key_col_idx = col_def.structure_key_parent_column_index
                                                .filter(|&idx| parent_sheet
                                                    .metadata
                                                    .as_ref()
                                                    .and_then(|m| m.columns.get(idx))
                                                    .map(|c| {
                                                        let h = c.header.to_ascii_lowercase();
                                                        h != "row_index" && h != "parent_key" && h != "temp_new_row_index" && h != "_obsolete_temp_new_row_index"
                                                    })
                                                    .unwrap_or(false)
                                                )
                                                .or_else(|| {
                                                    parent_sheet.metadata.as_ref().and_then(|meta| {
                                                        meta.columns.iter().position(|c| {
                                                            let h = c.header.to_ascii_lowercase();
                                                            h != "row_index" && h != "parent_key" && h != "temp_new_row_index" && h != "_obsolete_temp_new_row_index"
                                                        })
                                                    })
                                                });

                                            if let Some(key_idx) = key_col_idx {
                                                let parent_key = parent_row.get(key_idx).cloned().unwrap_or_default();
                                                let col_name = col_def.header.trim();
                                                if !parent_key.trim().is_empty() {
                                                    label = Some(format!("{} {}", parent_key, col_name));
                                                } else {
                                                    label = Some(col_name.to_string());
                                                }
                                            }
                                        }
                                    }

                                    // Fallback to parsed JSON preview
                                    label.unwrap_or_else(|| {
                                        generate_structure_preview(cell_value_str).0
                                    })
                                } else {
                                    // Check if this is a parent_key or grand_*_parent column (ancestor key)
                                    // These now store row_index values, need to resolve to display text
                                    if show_raw_ancestor_keys {
                                        // Debug mode: show raw numeric/text keys to verify migration
                                        cell_value_str.to_string()
                                    } else if use_lazy_resolution {
                                        // Lazy path: show raw now; progressive pass resolves later
                                        cell_value_str.to_string()
                                    } else {
                                        // Small tables: resolve immediately
                                        resolve_ancestor_key_display_text(
                                            cell_value_str,
                                            col_def,
                                            &category,
                                            &sheet_name,
                                            &registry,
                                        ).unwrap_or_else(|| cell_value_str.to_string())
                                    }
                                }
                            } else {
                                cell_value_str.to_string()
                            };
                            render_cell.display_text = preview_text;
                            render_cell.validation_state = if let Some(col_def) = col_def_opt {
                                // Use val_state from above
                                let (val_state, _) = match &col_def.validator {
                                    Some(ColumnValidator::Basic(data_type)) => {
                                        let (state, _) =
                                            validate_basic_cell(cell_value_str, *data_type);
                                        (state, None)
                                    }
                                    Some(ColumnValidator::Linked {
                                        target_sheet_name,
                                        target_column_index,
                                    }) => validate_linked_cell(
                                        cell_value_str,
                                        target_sheet_name,
                                        *target_column_index,
                                        &registry,
                                        &mut editor_state,
                                    ),
                                    Some(ColumnValidator::Structure) => {
                                        (ValidationState::Valid, None)
                                    }
                                    None => {
                                        let (state, _) = validate_basic_cell(
                                            cell_value_str,
                                            ColumnDataType::String,
                                        );
                                        (state, None)
                                    }
                                };
                                val_state
                            } else {
                                ValidationState::Invalid
                            };
                        } else {
                            // This should not happen if ensure_and_get_sheet_cache_mut worked correctly
                            error!("Render Cache: Index out of bounds when updating cell [{},{}] for sheet '{:?}/{}'. Dimensions: {}x{}", r_idx, c_idx, category, sheet_name, num_rows, num_cols);
                        }
                    } // end col loop
                } // end row loop

                // Phase 2: Progressive ancestor resolution for large tables
                // This runs AFTER initial fast display, progressively resolving row_index to text
                if use_lazy_resolution && !show_raw_ancestor_keys {
                    debug!("Starting progressive resolution for '{:?}/{}' ({} rows, filtered prioritization)",
                           category, sheet_name, num_rows);

                    // Build parent lookup cache for O(1) resolution
                    let parent_lookup_cache = build_parent_lookup_cache(&category, &sheet_name, &registry);

                    // Compute filtered indices to prioritize currently relevant rows
                    let filtered_indices: Vec<usize> = {
                        // Basic filter by metadata filters (contains OR semantics) + structure nav filter
                        let filters: Vec<Option<String>> = metadata.get_filters();
                        let mut out: Vec<usize> = Vec::new();
                        // Structure navigation hidden filter
                        let nav_ctx_opt = current_editor_state.as_ref().and_then(|st| st.structure_navigation_stack.last());
                        // Precompute column positions
                        let parent_key_col = metadata
                            .columns
                            .iter()
                            .position(|c| c.header.eq_ignore_ascii_case("parent_key"));

                        'rowloop: for r_idx in 0..num_rows {
                            // Apply metadata filters
                            if filters.iter().enumerate().any(|(ci, f)| {
                                if let Some(ftext) = f {
                                    if ftext.trim().is_empty() { return false; }
                                    let terms: Vec<&str> = ftext.split('|').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                                    if terms.is_empty() { return false; }
                                    let cell = sheet_data.grid.get(r_idx).and_then(|row| row.get(ci)).cloned().unwrap_or_default();
                                    let cell_norm = cell.to_lowercase();
                                    !terms.iter().any(|t| cell_norm.contains(&t.to_lowercase()))
                                } else { false }
                            }) {
                                continue 'rowloop;
                            }
                            // Apply structure navigation hidden filter if applicable
                            if let Some(nav_ctx) = nav_ctx_opt {
                                if nav_ctx.structure_sheet_name == sheet_name && nav_ctx.parent_category == category {
                                    // parent_key match
                                    if let Some(pk_col) = parent_key_col {
                                        let cell_pk = sheet_data.grid.get(r_idx).and_then(|row| row.get(pk_col)).cloned().unwrap_or_default();
                                        if cell_pk != nav_ctx.parent_row_key { continue 'rowloop; }
                                    }
                                }
                            }
                            out.push(r_idx);
                        }
                        out
                    };

                    let batch_size = 500;
                    let mut resolved_count = 0;
                    for chunk in filtered_indices.chunks(batch_size) {
                        for &r_idx in chunk {
                            for c_idx in 0..num_cols {
                                if let Some(col_def) = metadata.columns.get(c_idx) {
                                    let is_ancestor = col_def.header.eq_ignore_ascii_case("parent_key");
                                    if is_ancestor {
                                        let cell_value_str = sheet_data
                                            .grid
                                            .get(r_idx)
                                            .and_then(|row| row.get(c_idx))
                                            .map(|s| s.as_str())
                                            .unwrap_or("");
                                        if let Some(resolved_text) = resolve_ancestor_key_with_cache(
                                            cell_value_str,
                                            col_def,
                                            &sheet_name,
                                            &parent_lookup_cache,
                                        ) {
                                            if let Some(render_cell) = current_sheet_render_cache
                                                .get_mut(r_idx)
                                                .and_then(|row| row.get_mut(c_idx))
                                            {
                                                render_cell.display_text = resolved_text;
                                                resolved_count += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        debug!("  Resolved filtered chunk ({} total resolved)", resolved_count);
                    }
                    debug!("Progressive resolution complete: {} ancestor cells resolved (filtered)", resolved_count);
                }

                debug!("Render cache updated for '{:?}/{}'.", category, sheet_name);
            } else {
                warn!(
                    "Cannot update render cache for sheet '{:?}/{}': Metadata missing.",
                    category, sheet_name
                );
                render_cache.clear_sheet_render_data(&category, &sheet_name);
            }
        } else {
            // Sheet not found in registry, ensure its render cache is also cleared
            trace!(
                "Sheet '{:?}/{}' not found in registry. Clearing its render cache.",
                category,
                sheet_name
            );
            render_cache.clear_sheet_render_data(&category, &sheet_name);
        }
    }
}
