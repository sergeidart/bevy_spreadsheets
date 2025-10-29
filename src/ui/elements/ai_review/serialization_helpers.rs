// src/ui/elements/ai_review/serialization_helpers.rs
//! Shared helpers for serializing structure rows to JSON format
//! Used by AI review handlers and other UI components

use std::collections::HashMap;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::logic::lineage_helpers::resolve_parent_key_from_lineage;
use crate::ui::elements::editor::state::{EditorWindowState, StructureReviewEntry};

/// Serialize structure rows to JSON string (array of objects format)
/// 
/// This is the standard format used throughout the application for storing
/// structure data in cells: [{"field1": "val1", "field2": "val2"}, ...]
/// 
/// # Arguments
/// * `rows` - The rows to serialize (Vec<Vec<String>>)
/// * `headers` - Column headers corresponding to each field in the rows
/// 
/// # Returns
/// JSON string in array-of-objects format, or "[]" if serialization fails
pub fn serialize_structure_rows_to_json(rows: &[Vec<String>], headers: &[String]) -> String {
    if rows.is_empty() {
        return "[]".to_string();
    }

    let array_of_objects: Vec<serde_json::Map<String, serde_json::Value>> = rows
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (i, value) in row.iter().enumerate() {
                let field_name = headers
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("field_{}", i));
                obj.insert(field_name, serde_json::Value::String(value.clone()));
            }
            obj
        })
        .collect();

    let json_value = serde_json::json!(array_of_objects);
    serde_json::to_string(&json_value).unwrap_or_else(|_| "[]".to_string())
}

/// Choose the appropriate rows from a StructureReviewEntry based on decision status
/// 
/// Returns merged_rows if decided (user has made final choices), otherwise ai_rows
pub fn get_rows_to_serialize(entry: &StructureReviewEntry) -> &Vec<Vec<String>> {
    if entry.decided {
        &entry.merged_rows
    } else {
        &entry.ai_rows
    }
}

/// Resolve parent_key value for new row insertion in structure tables
/// 
/// Handles both real structure navigation (table-based) and virtual structure
/// navigation (JSON-based fields). Returns (column_index, value) tuple if resolved.
/// 
/// # Priority Order
/// 1. Real structure navigation (structure_navigation_stack) - higher priority
/// 2. Virtual structure stack (virtual_structure_stack) - for JSON structures
/// 
/// # Arguments
/// * `state` - Editor window state containing navigation stacks
/// * `registry` - Sheet registry for looking up parent rows
/// * `selected_category` - Current category
/// * `active_sheet_name` - Current sheet name
/// * `key_overrides` - Map of ancestor override flags (for virtual structures)
/// * `ancestor_key_values` - Vec of ancestor key display values (for virtual structures)
/// 
/// # Returns
/// Option<(column_index, parent_key_value)> if parent_key should be set
pub fn resolve_parent_key_for_new_row(
    state: &EditorWindowState,
    registry: &SheetRegistry,
    selected_category: &Option<String>,
    active_sheet_name: &str,
    key_overrides: &HashMap<usize, bool>,
    ancestor_key_values: &Vec<String>,
) -> Option<(usize, String)> {
    // Get metadata for the current (child) sheet
    let child_meta = registry
        .get_sheet(selected_category, active_sheet_name)
        .and_then(|s| s.metadata.as_ref())?;

    // Find parent_key column index
    let parent_key_col = child_meta
        .columns
        .iter()
        .position(|c| c.header.eq_ignore_ascii_case("parent_key"))?;

    // PRIORITY 1: Real structure navigation (Games → Games_Score)
    if !state.structure_navigation_stack.is_empty() {
        if let Some(nav_ctx) = state.structure_navigation_stack.last() {
            // Get the immediate parent row_index from ancestor_row_indices
            if let Some(parent_row_idx_str) = nav_ctx.ancestor_row_indices.last() {
                if let Ok(parent_row_idx) = parent_row_idx_str.parse::<usize>() {
                    bevy::log::info!(
                        "Resolved parent_key={} from structure_navigation_stack",
                        parent_row_idx
                    );
                    return Some((parent_key_col, parent_row_idx.to_string()));
                } else {
                    bevy::log::warn!(
                        "Failed to parse parent_row_index '{}' from structure_navigation_stack",
                        parent_row_idx_str
                    );
                }
            }
        }
        return None;
    }

    // PRIORITY 2: Virtual structure stack (JSON structure fields)
    if !state.virtual_structure_stack.is_empty() {
        let chain_len = state.virtual_structure_stack.len();
        let immediate_parent_idx = chain_len - 1;

        // Check if user wants to override the parent for this level
        let override_flag = *key_overrides
            .get(&(1000 + immediate_parent_idx))
            .unwrap_or(&false);

        if override_flag {
            // Get lineage values up to immediate parent
            let lineage_values: Vec<String> = ancestor_key_values
                .iter()
                .take(chain_len)
                .cloned()
                .collect();

            if !lineage_values.is_empty() {
                // Get parent sheet name from virtual structure stack
                if let Some(parent_ctx) = state.virtual_structure_stack.last() {
                    // Resolve parent_key from lineage
                    if let Some(parent_row_idx) = resolve_parent_key_from_lineage(
                        registry,
                        selected_category,
                        &parent_ctx.parent.parent_sheet,
                        &lineage_values,
                    ) {
                        bevy::log::info!(
                            "Resolved parent_key={} from virtual_structure_stack lineage {:?}",
                            parent_row_idx,
                            lineage_values
                        );
                        return Some((parent_key_col, parent_row_idx.to_string()));
                    } else {
                        bevy::log::warn!(
                            "Failed to resolve parent_key from lineage: {:?}",
                            lineage_values
                        );
                    }
                }
            }
        }
    }

    None
}

/// Update parent_new_row_index for all structure entries after removing an index
/// 
/// When a new row review is removed, all structure entries pointing to higher
/// indices need to be decremented to maintain correct parent references.
pub fn adjust_parent_indices_after_removal(state: &mut EditorWindowState, removed_idx: usize) {
    for entry in state.ai_structure_reviews.iter_mut() {
        if let Some(parent_idx) = entry.parent_new_row_index {
            if parent_idx > removed_idx {
                entry.parent_new_row_index = Some(parent_idx - 1);
            }
        }
    }
}
