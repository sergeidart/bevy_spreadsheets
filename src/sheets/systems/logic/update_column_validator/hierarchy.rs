// src/sheets/systems/logic/update_column_validator/hierarchy.rs
// Hierarchy depth calculation and technical column creation

use bevy::prelude::*;

use crate::sheets::{
    definitions::{ColumnDataType, ColumnDefinition},
    resources::SheetRegistry,
};

/// Calculate the hierarchy depth using heuristic detection based on existing columns
/// Returns the number of parent levels (0 for root, 1 for child)
/// 
/// Heuristic rules:
/// - No parent_key column: depth = 0 (root table)
/// - Has parent_key: depth = 1 (child table)
pub fn calculate_hierarchy_depth(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
) -> usize {
    info!(
        "Calculating hierarchy depth for sheet '{:?}/{}' using heuristic detection",
        category, sheet_name
    );

    // First, try the metadata-based approach (fallback to heuristic if fails)
    let metadata_depth = {
        let mut depth = 0;
        let mut current_category = category.clone();
        let mut current_sheet = sheet_name.to_string();
        let mut safety = 0;

        while safety < 32 {
            safety += 1;
            
            if let Some(sheet) = registry.get_sheet(&current_category, &current_sheet) {
                if let Some(meta) = &sheet.metadata {
                    if let Some(parent_link) = &meta.structure_parent {
                        depth += 1;
                        info!(
                            "  Found parent via metadata: '{:?}/{}', depth now {}",
                            parent_link.parent_category, parent_link.parent_sheet, depth
                        );
                        current_category = parent_link.parent_category.clone();
                        current_sheet = parent_link.parent_sheet.clone();
                        continue;
                    }
                }
            }
            break;
        }
        depth
    };

    // Use heuristic detection based on column names
    let heuristic_depth = if let Some(sheet) = registry.get_sheet(category, sheet_name) {
        if let Some(meta) = &sheet.metadata {
            // Check for parent_key column
            let has_parent_key = meta.columns.iter()
                .any(|col| col.header.eq_ignore_ascii_case("parent_key"));
            
            if !has_parent_key {
                info!("  Heuristic: No parent_key found, depth = 0 (root table)");
                0
            } else {
                info!("  Heuristic: Has parent_key, depth = 1 (child table)");
                1
            }
        } else {
            info!("  Heuristic: No metadata found, assuming depth = 0");
            0
        }
    } else {
        info!("  Heuristic: Sheet not found, assuming depth = 0");
        0
    };

    // Use the maximum of both methods
    let final_depth = metadata_depth.max(heuristic_depth);
    
    info!(
        "Final hierarchy depth for '{:?}/{}': {} (metadata: {}, heuristic: {})",
        category, sheet_name, final_depth, metadata_depth, heuristic_depth
    );
    
    final_depth
}

/// Create technical columns (row_index, parent_key) for a structure sheet
/// 
/// **Post-Refactor (2025-10-28):**
/// Now only creates row_index and parent_key columns, regardless of depth.
/// The grand_N_parent columns have been removed as they were redundant - we can
/// walk the parent chain programmatically using parent_key references.
/// 
/// Lineage display: `Mass Effect 3 › PC › Steam` (built dynamically)
pub fn create_structure_technical_columns(_depth: usize) -> Vec<ColumnDefinition> {
    vec![
        ColumnDefinition {
            header: "row_index".to_string(),
                display_header: None,
            data_type: ColumnDataType::String,
            validator: None,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: Some(false),
            deleted: false,
            hidden: true, // row_index is always hidden
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        },
        ColumnDefinition {
            header: "parent_key".to_string(),
                display_header: None,
            data_type: ColumnDataType::String,
            validator: None,
            filter: None,
            ai_context: Some("Parent identifier for hierarchical structure filtering".to_string()),
            ai_enable_row_generation: None,
            ai_include_in_send: Some(true),
            deleted: false,
            hidden: false,
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        },
    ]
}



