// src/ui/elements/editor/structure_navigation.rs
use crate::sheets::definitions::{
    SheetMetadata,
};
use crate::sheets::{
    resources::{SheetRegistry},
};
use bevy::prelude::*;

/// Collects ancestor keys from a parent row for structure navigation.
/// 
/// **Post-Refactor (2025-10-28):**
/// Now uses lineage walking instead of reading grand_N_parent columns.
/// Walks up the parent chain using parent_key references to build full lineage.
/// 
/// Returns (ancestor_keys, display_value) where:
/// - ancestor_keys: Display values from all ancestors (root to immediate parent)
/// - display_value: The display value from the current parent row
///
/// # Arguments
/// * `target_structure_name` - The name of the child structure table we're navigating to
#[allow(dead_code)]
pub fn collect_structure_ancestors(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    target_structure_name: &str,
    row_index: usize,
) -> (Vec<String>, String) {
    use crate::sheets::systems::logic::lineage_helpers;
    
    // Get the current row's display value
    let mut display_value = String::new();
    let mut parent_row_parent_key: Option<usize> = None;
    
    if let Some(parent_sheet) = registry.get_sheet(category, sheet_name) {
        if let Some(parent_meta) = &parent_sheet.metadata {
            if let Some(row) = parent_sheet.grid.get(row_index) {
                // Get display value from first content column
                display_value = get_first_content_column_value(parent_meta, row);
                
                // Get this row's parent_key (to walk its ancestors, not including itself)
                if let Some(pk_col) = parent_meta.columns.iter().position(|c| c.header.eq_ignore_ascii_case("parent_key")) {
                    if let Some(pk_str) = row.get(pk_col) {
                        if !pk_str.is_empty() {
                            parent_row_parent_key = pk_str.parse::<usize>().ok();
                        }
                    }
                }
            }
        }
    }
    
    // Walk up the lineage from the parent's parent (to get ancestors, not including current parent)
    let mut ancestor_keys: Vec<String> = if let Some(parent_pk) = parent_row_parent_key {
        // This row has a parent, walk from there
        if let Some(parent_link) = registry.get_sheet(category, sheet_name)
            .and_then(|sd| sd.metadata.as_ref())
            .and_then(|meta| meta.structure_parent.as_ref())
        {
            let lineage = lineage_helpers::walk_parent_lineage(
                registry,
                &parent_link.parent_category,
                &parent_link.parent_sheet,
                parent_pk
            );
            
            lineage.iter()
                .map(|(_, display_val, _)| display_val.clone())
                .collect()
        } else {
            Vec::new()
        }
    } else {
        // This row is at root level (no parent), so no ancestors
        Vec::new()
    };
    
    // Add the current parent's display value to the ancestor keys
    // This way, ancestor_keys contains the full lineage including the immediate parent
    if !display_value.is_empty() {
        ancestor_keys.push(display_value.clone());
    }
    
    bevy::log::info!(
        "Structure ancestors collected (lineage walk): {} -> {} | ancestors={:?}, display='{}'",
        sheet_name,
        target_structure_name,
        ancestor_keys,
        display_value
    );
    
    (ancestor_keys, display_value)
}

/// Get display value from first content column (skipping technical columns)
/// 
/// This is a wrapper around SheetMetadata::get_first_data_column_value for backwards compatibility
pub fn get_first_content_column_value(metadata: &SheetMetadata, row: &[String]) -> String {
    metadata.get_first_data_column_value(row)
}

/// Get the index of the first data (non-technical) column in metadata.
/// Filters out: row_index, parent_key, id, created_at, updated_at
#[allow(dead_code)]
pub fn get_first_data_column_index(metadata: &SheetMetadata) -> Option<usize> {
    metadata.columns.iter().position(|col| {
        let lower = col.header.to_lowercase();
        lower != "row_index"
            && lower != "parent_key"
            && lower != "id"
            && lower != "created_at"
            && lower != "updated_at"
    })
}

// Deprecated: Virtual structure view events no longer used
// Structure navigation now uses real DB-backed child tables via structure_navigation_stack




