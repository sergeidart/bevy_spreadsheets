// src/sheets/systems/ai/parent_chain_helpers.rs
// Helper functions for parent chain filtering and row matching

use bevy::prelude::*;

use crate::sheets::resources::SheetRegistry;
use crate::sheets::definitions::SheetMetadata;
use crate::sheets::systems::logic::lineage_helpers;
use crate::ui::elements::editor::state::EditorWindowState;

/// Extract parent_key column from metadata
/// Returns parent_key column index if found
pub fn extract_parent_key_column(metadata: &SheetMetadata) -> Option<usize> {
    metadata
        .columns
        .iter()
        .position(|c| c.header.eq_ignore_ascii_case("parent_key"))
        .or_else(|| if metadata.is_structure_table() { Some(1) } else { None })
}

/// Check if a row matches all expected parent indices in the chain
/// Returns true if all parent relationships match
pub fn row_matches_parent_chain(
    row: &[String],
    expected_parent_indices: &[usize],
    parent_key_col: Option<usize>,
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

    // No longer check grand_N_parent columns (they've been removed)
    // Note: This means we only match on immediate parent, not full chain
    // Full chain filtering would require walking the parent hierarchy
    
    true
}

/// Convert human-readable parent names to row_index values
/// 
/// This function resolves display names (e.g., ["Battlefield 6", "PC"]) to the parent_key
/// row_index by using the unified lineage resolution system.
///
/// Priority order:
/// 1. Virtual structure stack (for JSON structure fields)
/// 2. Structure navigation stack (for real structure tables) - uses pre-resolved indices
/// 3. Lineage resolution (for AI-provided parent names) - resolves display names to row_index
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
    // In this case, we already have the resolved row indices
    if !state.structure_navigation_stack.is_empty() {
        // Use ancestor_row_indices which contain numeric row_index values
        // These are separate from ancestor_keys (which contain display values for UI)
        for nav_ctx in &state.structure_navigation_stack {
            for ancestor_row_idx in &nav_ctx.ancestor_row_indices {
                if let Ok(idx) = ancestor_row_idx.parse::<usize>() {
                    if !indices.contains(&idx) {
                        indices.push(idx);
                    }
                } else {
                    warn!(
                        "convert_parent_names_to_row_indices: Failed to parse ancestor_row_index '{}' as usize",
                        ancestor_row_idx
                    );
                }
            }
        }
        
        info!(
            "convert_parent_names_to_row_indices: Using structure_navigation_stack, got indices={:?}",
            indices
        );
        return indices;
    }

    // PRIORITY 3: Resolve from parent names using unified lineage resolution
    // This is for AI-provided parent names that need to be converted to row_index
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

    // Use the unified lineage resolution to convert parent names to parent_key
    let parent_sheet = &parent_info.parent_sheet;
    
    info!(
        "convert_parent_names_to_row_indices: Resolving parent names {:?} in parent sheet '{}'",
        parent_names, parent_sheet
    );
    
    if let Some(parent_key) = lineage_helpers::resolve_parent_key_from_lineage(
        registry,
        &cat_ctx,
        parent_sheet,
        parent_names,
    ) {
        indices.push(parent_key);
        info!(
            "convert_parent_names_to_row_indices: Resolved to parent_key={}",
            parent_key
        );
    } else {
        warn!(
            "convert_parent_names_to_row_indices: Failed to resolve parent names {:?} in sheet '{}'",
            parent_names, parent_sheet
        );
    }

    info!("Final parent_row_indices={:?}", indices);
    indices
}
