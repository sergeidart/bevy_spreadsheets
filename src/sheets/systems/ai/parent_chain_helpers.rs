// src/sheets/systems/ai/parent_chain_helpers.rs
// Helper functions for parent chain filtering and row matching

use bevy::prelude::*;

use crate::sheets::resources::SheetRegistry;
use crate::sheets::definitions::SheetMetadata;
use crate::ui::elements::editor::state::EditorWindowState;

/// Extract parent_key column and grand_*_parent columns from metadata
/// Returns (parent_key_col, grand_cols) where grand_cols is a vector of (column_index, N)
pub fn extract_parent_columns(metadata: &SheetMetadata) -> (Option<usize>, Vec<(usize, usize)>) {
    let parent_key_col = metadata
        .columns
        .iter()
        .position(|c| c.header.eq_ignore_ascii_case("parent_key"))
        .or_else(|| if metadata.is_structure_table() { Some(1) } else { None });

    let grand_cols: Vec<(usize, usize)> = metadata
        .columns
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            if c.header.starts_with("grand_") && c.header.ends_with("_parent") {
                if let Some(n_str) = c
                    .header
                    .strip_prefix("grand_")
                    .and_then(|s| s.strip_suffix("_parent"))
                {
                    if let Ok(n) = n_str.parse::<usize>() {
                        return Some((i, n));
                    }
                }
            }
            None
        })
        .collect();

    (parent_key_col, grand_cols)
}

/// Check if a row matches all expected parent indices in the chain
/// Returns true if all parent relationships match
pub fn row_matches_parent_chain(
    row: &[String],
    expected_parent_indices: &[usize],
    parent_key_col: Option<usize>,
    grand_cols: &[(usize, usize)],
) -> bool {
    if expected_parent_indices.is_empty() {
        return true;
    }

    // Check parent_key (immediate parent - last in chain)
    if let Some(pk_idx) = parent_key_col {
        if let Some(&expected_parent_idx) = expected_parent_indices.last() {
            let actual_value = row.get(pk_idx).cloned().unwrap_or_default();
            let expected_value = expected_parent_idx.to_string();
            if actual_value != expected_value {
                return false;
            }
        }
    }

    // Check grand_N_parent columns
    if expected_parent_indices.len() > 1 {
        let clen = expected_parent_indices.len();
        for (gcol, n) in grand_cols {
            if clen > *n {
                let expected_idx = expected_parent_indices[clen - 1 - *n];
                let expected_str = expected_idx.to_string();
                if row.get(*gcol).map(|s| s.as_str()) != Some(expected_str.as_str()) {
                    return false;
                }
            }
        }
    }

    true
}

/// Convert human-readable parent names to row_index values
/// Walks up the parent chain to find the row_index for each parent name
/// 
/// Priority order:
/// 1. Virtual structure stack (for JSON structure fields)
/// 2. Structure navigation stack (for real structure tables)
/// 3. Metadata structure_parent (for direct parent table lookup)
pub fn convert_parent_names_to_row_indices(
    parent_names: &[String],
    state: &EditorWindowState,
    registry: &SheetRegistry,
) -> Vec<usize> {
    let mut indices = Vec::new();

    info!(
        "convert_parent_names_to_row_indices: parent_names={:?}",
        parent_names
    );

    if parent_names.is_empty() {
        info!("convert_parent_names_to_row_indices: No parent names provided, returning empty");
        return indices;
    }

    // PRIORITY 1: Check if we're in a virtual structure view (JSON structures)
    if !state.virtual_structure_stack.is_empty() {
        // Extract parent row indices from the stack
        for vctx in &state.virtual_structure_stack {
            indices.push(vctx.parent.parent_row);
        }
        info!(
            "convert_parent_names_to_row_indices: Using virtual_structure_stack, got indices={:?}",
            indices
        );
        return indices;
    }

    // PRIORITY 2: Check if we're in a real structure navigation (real child tables)
    if !state.structure_navigation_stack.is_empty() {
        // For real structure sheets, we need to parse parent_row_key to get the row index
        for nav_ctx in &state.structure_navigation_stack {
            if let Ok(parent_idx) = nav_ctx.parent_row_key.parse::<usize>() {
                indices.push(parent_idx);
            } else {
                warn!(
                    "convert_parent_names_to_row_indices: Failed to parse parent_row_key '{}' as usize",
                    nav_ctx.parent_row_key
                );
            }
        }
        
        // Also add ancestor keys
        for nav_ctx in &state.structure_navigation_stack {
            for ancestor_key in &nav_ctx.ancestor_keys {
                if let Ok(ancestor_idx) = ancestor_key.parse::<usize>() {
                    if !indices.contains(&ancestor_idx) {
                        indices.push(ancestor_idx);
                    }
                }
            }
        }
        
        info!(
            "convert_parent_names_to_row_indices: Using structure_navigation_stack, got indices={:?}",
            indices
        );
        return indices;
    }

    // PRIORITY 3: Try to resolve from metadata structure_parent
    let (cat_ctx, sheet_ctx) = state.current_sheet_context();
    let Some(sheet_name) = sheet_ctx else {
        warn!("convert_parent_names_to_row_indices: No sheet context available");
        return indices;
    };

    let Some(sheet_ref) = registry.get_sheet(&cat_ctx, &sheet_name) else {
        warn!("convert_parent_names_to_row_indices: Sheet '{}' not found", sheet_name);
        return indices;
    };

    let Some(meta) = &sheet_ref.metadata else {
        warn!("convert_parent_names_to_row_indices: Sheet '{}' has no metadata", sheet_name);
        return indices;
    };

    // Get parent table reference from metadata
    let parent_link = meta.structure_parent.as_ref();
    let Some(parent_info) = parent_link else {
        info!("convert_parent_names_to_row_indices: Sheet '{}' has no parent structure metadata, returning empty", sheet_name);
        return indices;
    };

    // For each parent name, look it up in the parent table
    let mut current_parent_sheet = parent_info.parent_sheet.clone();
    let mut current_parent_category = parent_info.parent_category.clone();

    for parent_name in parent_names {
        let Some(parent_sheet_ref) = registry.get_sheet(&current_parent_category, &current_parent_sheet) else {
            break;
        };

        let Some(parent_meta) = &parent_sheet_ref.metadata else {
            break;
        };

        // Find the first data column index (skip technical columns)
        let first_data_col = find_first_data_column(parent_meta);

        let Some(data_col_idx) = first_data_col else {
            break;
        };

        // Find the row where the first data column matches the parent name
        let mut found_row_idx = None;
        for (row_idx, row) in parent_sheet_ref.grid.iter().enumerate() {
            if let Some(cell_value) = row.get(data_col_idx) {
                if cell_value == parent_name {
                    found_row_idx = Some(row_idx);
                    break;
                }
            }
        }

        let Some(row_idx) = found_row_idx else {
            // Parent name not found, can't continue chain
            warn!(
                "Parent name '{}' not found in table '{}', stopping chain lookup",
                parent_name, current_parent_sheet
            );
            break;
        };

        info!(
            "Found parent '{}' at row_index={} in table '{}'",
            parent_name, row_idx, current_parent_sheet
        );
        indices.push(row_idx);

        // Move up to next parent in chain if there is one
        if let Some(grandparent_link) = &parent_meta.structure_parent {
            current_parent_sheet = grandparent_link.parent_sheet.clone();
            current_parent_category = grandparent_link.parent_category.clone();
        } else {
            // No more parents in chain
            break;
        }
    }

    info!("Final parent_row_indices={:?}", indices);
    indices
}

/// Find the first non-technical data column in metadata
fn find_first_data_column(metadata: &SheetMetadata) -> Option<usize> {
    metadata.columns.iter().position(|col| {
        let lower = col.header.to_lowercase();
        lower != "row_index"
            && lower != "parent_key"
            && !lower.starts_with("grand_")
            && lower != "id"
            && lower != "created_at"
            && lower != "updated_at"
    })
}
