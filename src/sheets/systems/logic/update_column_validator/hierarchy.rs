// src/sheets/systems/logic/update_column_validator/hierarchy.rs
// Hierarchy depth calculation and technical column creation

use bevy::prelude::*;

use crate::sheets::{
    definitions::{ColumnDataType, ColumnDefinition},
    resources::SheetRegistry,
};

/// Calculate the hierarchy depth using heuristic detection based on existing columns
/// Returns the number of parent levels (0 for root, 1 for first child, etc.)
/// 
/// Heuristic rules:
/// - No parent_key column: depth = 0 (root table)
/// - Has parent_key but no grand_N_parent: depth = 1 (first level child)
/// - Has grand_1_parent: depth = 2
/// - Has grand_N_parent: depth = N + 1 (highest N determines depth)
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
                // Find highest grand_N_parent column
                let max_grand = meta.columns.iter()
                    .filter_map(|col| {
                        if col.header.starts_with("grand_") && col.header.ends_with("_parent") {
                            let n_str = col.header.trim_start_matches("grand_").trim_end_matches("_parent");
                            n_str.parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .max();
                
                if let Some(n) = max_grand {
                    info!("  Heuristic: Found grand_{}_parent, depth = {} (N+1)", n, n + 1);
                    n + 1
                } else {
                    info!("  Heuristic: Has parent_key but no grand_N_parent, depth = 1 (first level child)");
                    1
                }
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

/// Create technical columns (row_index, parent_key, grand_N_parent) for a structure sheet
/// based on its hierarchy depth
pub fn create_structure_technical_columns(depth: usize) -> Vec<ColumnDefinition> {
    let mut columns = vec![
        ColumnDefinition {
            header: "row_index".to_string(),
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
    ];

    // Add grandparent keys from deepest to shallowest (grand_2_parent, grand_1_parent, parent_key)
    // This matches the requirement: row_index, grand_2_parent, grand_1_parent, parent_key, data...
    if depth > 2 {
        for level in (2..depth).rev() {
            columns.push(ColumnDefinition {
                header: format!("grand_{}_parent", level - 1),
                data_type: ColumnDataType::String,
                validator: None,
                filter: None,
                ai_context: Some(format!("Level {} parent identifier for hierarchical filtering", level - 1)),
                ai_enable_row_generation: None,
                ai_include_in_send: Some(true), // Send all parent keys to AI
                deleted: false,
                hidden: false, // Technical but sent to AI
                width: None,
                structure_schema: None,
                structure_column_order: None,
                structure_key_parent_column_index: None,
                structure_ancestor_key_parent_column_indices: None,
            });
        }
    }

    // Add grand_1_parent if depth > 1
    if depth > 1 {
        columns.push(ColumnDefinition {
            header: "grand_1_parent".to_string(),
            data_type: ColumnDataType::String,
            validator: None,
            filter: None,
            ai_context: Some("Grandparent identifier for hierarchical filtering".to_string()),
            ai_enable_row_generation: None,
            ai_include_in_send: Some(true),
            deleted: false,
            hidden: false,
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        });
    }

    // Always add parent_key last (as it's the immediate parent)
    columns.push(ColumnDefinition {
        header: "parent_key".to_string(),
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
    });

    columns
}
