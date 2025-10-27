// src/sheets/systems/ai/duplicate_map_helpers.rs
// Helper functions for building duplicate detection maps with parent chain awareness

use std::collections::HashMap;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::definitions::ColumnValidator;
use crate::ui::elements::editor::state::EditorWindowState;
use crate::sheets::systems::ai::row_helpers::normalize_cell_value;
use crate::sheets::systems::ai::parent_chain_helpers::{
    extract_parent_columns, row_matches_parent_chain, convert_parent_names_to_row_indices,
};
use crate::sheets::systems::ai::column_helpers::extract_linked_column_info;

/// Resolve a linked cell's stored value (usually a row_index) to its display text in the target sheet,
/// and return the normalized comparable string.
pub fn resolve_linked_display_value(
    registry: &SheetRegistry,
    category: &Option<String>,
    target_sheet_name: &str,
    target_column_index: usize,
    stored_value: &str,
) -> Option<String> {
    // Parse stored value as row_index
    let row_index_value = stored_value.parse::<i64>().ok()?;
    let sheet = registry.get_sheet(category, target_sheet_name)?;
    // Find the row with matching row_index in column 0
    let row_opt = sheet.grid.iter().find(|r| {
        r.get(0)
            .and_then(|s| s.parse::<i64>().ok())
            .map(|idx| idx == row_index_value)
            .unwrap_or(false)
    })?;
    // Get display cell
    let display_val = row_opt.get(target_column_index)?.clone();
    Some(normalize_cell_value(&display_val))
}

/// Build a duplicate detection map filtered by parent context using a single key column
/// This ensures duplicate checking only considers rows with matching parent chain
pub fn build_duplicate_map_for_parents(
    parent_prefix_values: &[String],
    key_actual_col_opt: Option<usize>,
    cat_ctx: &Option<String>,
    sheet_ctx: &Option<String>,
    registry: &SheetRegistry,
    state: &EditorWindowState,
    _included: &[usize],
) -> HashMap<String, usize> {
    let mut map: HashMap<String, usize> = HashMap::new();

    let Some(first_col_actual) = key_actual_col_opt else {
        return map;
    };

    let Some(sheet_name) = sheet_ctx else {
        return map;
    };

    let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_name) else {
        return map;
    };

    // If no parent prefix, check against all rows (base table)
    if parent_prefix_values.is_empty() {
        // Determine if key column is a Linked column to resolve comparable text
        let linked_info = sheet_ref
            .metadata
            .as_ref()
            .and_then(|meta| meta.columns.get(first_col_actual))
            .and_then(|c| match &c.validator {
                Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                    Some((target_sheet_name.clone(), *target_column_index))
                }
                _ => None,
            });

        for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
            if let Some(val) = row.get(first_col_actual) {
                // Insert raw normalized value
                let norm_raw = normalize_cell_value(val);
                if !norm_raw.is_empty() {
                    map.entry(norm_raw).or_insert(row_idx);
                }
                // If linked, also resolve to display text and insert that key too
                if let Some((ref target_sheet_name, target_col_idx)) = linked_info {
                    if let Some(display_norm) = resolve_linked_display_value(
                        registry,
                        cat_ctx,
                        target_sheet_name,
                        target_col_idx,
                        val,
                    ) {
                        if !display_norm.is_empty() {
                            map.entry(display_norm).or_insert(row_idx);
                        }
                    }
                }
            }
        }
        return map;
    }

    // Convert parent names to row_index values by looking them up in parent tables
    let parent_row_indices = convert_parent_names_to_row_indices(parent_prefix_values, state, registry);

    // Get parent column indices
    let meta_opt = sheet_ref.metadata.as_ref();
    let (parent_key_col, grand_cols) = if let Some(meta) = meta_opt {
        extract_parent_columns(meta)
    } else {
        return map;
    };

    // Determine if key column is a Linked column to resolve comparable text
    let linked_info = sheet_ref
        .metadata
        .as_ref()
        .and_then(|meta| meta.columns.get(first_col_actual))
        .and_then(|c| match &c.validator {
            Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                Some((target_sheet_name.clone(), *target_column_index))
            }
            _ => None,
        });

    // Filter rows to only those matching ALL parent row_index values
    for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
        if !row_matches_parent_chain(row, &parent_row_indices, parent_key_col, &grand_cols) {
            continue;
        }

        if let Some(val) = row.get(first_col_actual) {
            // Raw value key
            let norm_raw = normalize_cell_value(val);
            if !norm_raw.is_empty() {
                map.entry(norm_raw).or_insert(row_idx);
            }
            // Linked display key (if applicable)
            if let Some((ref target_sheet_name, target_col_idx)) = linked_info {
                if let Some(display_norm) = resolve_linked_display_value(
                    registry,
                    cat_ctx,
                    target_sheet_name,
                    target_col_idx,
                    val,
                ) {
                    if !display_norm.is_empty() {
                        map.entry(display_norm).or_insert(row_idx);
                    }
                }
            }
        }
    }

    map
}

/// Build a composite duplicate map using ALL included columns for the filtered parent context.
/// Key is the concatenation of normalized values for each included column (resolving Linked columns to display text).
pub fn build_composite_duplicate_map_for_parents(
    parent_prefix_values: &[String],
    included: &[usize],
    cat_ctx: &Option<String>,
    sheet_ctx: &Option<String>,
    registry: &SheetRegistry,
    state: &EditorWindowState,
) -> HashMap<String, usize> {
    let mut map: HashMap<String, usize> = HashMap::new();

    let Some(sheet_name) = sheet_ctx else {
        return map;
    };
    let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_name) else {
        return map;
    };
    let Some(meta) = &sheet_ref.metadata else {
        return map;
    };

    // Compute parent row indices to filter by full chain
    let parent_row_indices = convert_parent_names_to_row_indices(parent_prefix_values, state, registry);

    // Get parent_key and grand columns
    let (parent_key_col, grand_cols) = extract_parent_columns(meta);

    // Pre-resolve linked info per included column
    let linked_targets = extract_linked_column_info(meta, included);

    // Iterate rows and filter by full parent chain; then build composite key
    for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
        if !row_matches_parent_chain(row, &parent_row_indices, parent_key_col, &grand_cols) {
            continue;
        }

        // Build composite key from included columns only
        // Parent filtering is done via row_matches_parent_chain above, not by adding parent indices to the key
        let mut parts: Vec<String> = Vec::with_capacity(included.len());

        // Track if all data columns are empty
        let mut all_empty = true;
        
        for &col in included {
            if let Some(raw) = row.get(col) {
                if let Some((ref target_sheet, target_col)) = linked_targets.get(&col) {
                    if let Some(norm) = resolve_linked_display_value(
                        registry,
                        cat_ctx,
                        target_sheet,
                        *target_col,
                        raw,
                    ) {
                        if !norm.trim().is_empty() {
                            all_empty = false;
                        }
                        parts.push(norm);
                        continue;
                    }
                }
                let norm = normalize_cell_value(raw);
                if !norm.trim().is_empty() {
                    all_empty = false;
                }
                parts.push(norm);
            } else {
                parts.push(String::new());
            }
        }

        // Skip rows where all data columns are empty
        if all_empty {
            continue;
        }

        let key = parts.join("||");
        map.entry(key).or_insert(row_idx);
    }

    map
}
