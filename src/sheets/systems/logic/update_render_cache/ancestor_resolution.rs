// src/sheets/systems/logic/update_render_cache/ancestor_resolution.rs
//! Ancestor key resolution for render cache
//!
//! Provides functions to resolve numeric row_index values stored in ancestor columns
//! (parent_key, grand_*_parent) to human-readable display text by looking up the
//! corresponding row in parent tables.

use bevy::prelude::*;
use crate::sheets::resources::SheetRegistry;

/// Resolves ancestor key columns (parent_key, grand_*_parent) from row_index to display text
///
/// After the migration, these columns store numeric row_index values instead of text.
/// This function looks up the parent row by row_index and returns the display value
/// from the first data column.
///
/// Returns Some(display_text) if resolution succeeds, None otherwise.
pub fn resolve_ancestor_key_display_text(
    cell_value: &str,
    col_def: &crate::sheets::definitions::ColumnDefinition,
    current_category: &Option<String>,
    current_sheet_name: &str,
    registry: &SheetRegistry,
) -> Option<String> {
    // Only process parent_key column
    let is_parent_key = col_def.header.eq_ignore_ascii_case("parent_key");

    if !is_parent_key {
        return None; // Not an ancestor key column
    }

    // Skip empty values
    if cell_value.trim().is_empty() {
        return None;
    }

    // Try to parse as row_index (numeric)
    let row_index_value = match cell_value.parse::<i64>() {
        Ok(idx) => idx,
        Err(_) => {
            // Not numeric - might be pre-migration text value, display as-is
            trace!("Cell value '{}' in column '{}' is not numeric, skipping resolution", cell_value, col_def.header);
            return None;
        }
    };

    trace!("Resolving ancestor column '{}' with row_index={} in table '{}'",
           col_def.header, row_index_value, current_sheet_name);

    // Determine how many levels up to navigate
    // parent_key: 1 level up (immediate parent)
    // grand_1_parent: 2 levels up (parent's parent)
    // grand_2_parent: 3 levels up (parent's parent's parent)
    let levels_up = if is_parent_key {
        1
    } else {
        // Parse the number from grand_N_parent
        let level_str = col_def.header
            .strip_prefix("grand_")
            .and_then(|s| s.strip_suffix("_parent"))?;
        let grand_level: usize = level_str.parse().ok()?;
        grand_level + 1 // grand_1 means 2 levels up
    };

    trace!("  → Need to navigate {} levels up", levels_up);

    // Navigate up the hierarchy
    // Structure tables are named: ParentTable_ColumnName_SubColumn...
    // Navigate up by removing the last N segments
    let mut target_table_name = current_sheet_name;
    for level in 0..levels_up {
        target_table_name = match target_table_name.rsplit_once('_') {
            Some((parent, _)) => parent,
            None => {
                trace!("  → Failed to navigate up {} levels (stopped at level {})", levels_up, level);
                return None; // Can't navigate up that many levels
            }
        };
        trace!("    Level {}: {}", level + 1, target_table_name);
    }

    trace!("  → Target table: '{}'", target_table_name);

    // Get target ancestor sheet from registry
    let parent_sheet = registry.get_sheet(current_category, target_table_name)?;

    // Find the row with matching row_index
    // row_index is stored in column 0 for all tables
    let parent_row = parent_sheet.grid.iter().find(|row| {
        row.get(0)
            .and_then(|idx_str| idx_str.parse::<i64>().ok())
            .map(|idx| idx == row_index_value)
            .unwrap_or(false)
    })?;

    // Get the display value from the first data column of the parent row
    // For structure tables: skip row_index (0), parent_key (1), grand_*_parent (2+), find first real data column
    // For regular tables: skip row_index (0), find first real data column
    let parent_metadata = parent_sheet.metadata.as_ref()?;

    // Find first non-technical column
    let first_data_col_idx = parent_metadata.columns.iter().position(|col| {
        let lower = col.header.to_lowercase();
        lower != "row_index"
            && lower != "parent_key"
            && lower != "id"
            && lower != "created_at"
            && lower != "updated_at"
    })?;

    // Get display text from that column
    let display_text = parent_row.get(first_data_col_idx)?.clone();

    if display_text.is_empty() {
        trace!("  → Resolved but empty, returning None");
        None
    } else {
        trace!("  → Successfully resolved: {} → '{}'", row_index_value, display_text);
        Some(display_text)
    }
}

/// Resolve ancestor key using pre-built lookup cache (O(1) instead of O(n))
///
/// This is an optimized version used during batch resolution that uses a pre-built
/// HashMap for O(1) lookups instead of linear searching through parent tables.
pub fn resolve_ancestor_key_with_cache(
    cell_value: &str,
    col_def: &crate::sheets::definitions::ColumnDefinition,
    current_sheet_name: &str,
    parent_cache: &std::collections::HashMap<(String, i64), String>,
) -> Option<String> {
    // Only process parent_key column
    let is_parent_key = col_def.header.eq_ignore_ascii_case("parent_key");

    if !is_parent_key {
        return None;
    }

    // Skip empty values
    if cell_value.trim().is_empty() {
        return None;
    }

    // Parse row_index
    let row_index_value = cell_value.parse::<i64>().ok()?;

    // Determine target table (same logic as before)
    let levels_up = if is_parent_key {
        1
    } else {
        let level_str = col_def.header
            .strip_prefix("grand_")
            .and_then(|s| s.strip_suffix("_parent"))?;
        let grand_level: usize = level_str.parse().ok()?;
        grand_level + 1
    };

    // Navigate up hierarchy
    let mut target_table_name = current_sheet_name;
    for _ in 0..levels_up {
        target_table_name = target_table_name.rsplit_once('_')?.0;
    }

    // O(1) lookup in cache!
    parent_cache.get(&(target_table_name.to_string(), row_index_value)).cloned()
}
