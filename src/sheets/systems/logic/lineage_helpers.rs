// src/sheets/systems/logic/lineage_helpers.rs
//! Helper functions for building parent lineage chains dynamically.
//!
//! Lineage is built by walking parent_key references (no grand_N_parent columns).
//! Example: Games → Games_Platforms → Games_Platforms_Store

use bevy::prelude::*;
use crate::sheets::resources::SheetRegistry;

#[cfg(test)]
mod tests;

/// Lineage entry: (table_name, display_value, row_index)
pub type LineageEntry = (String, String, usize);

/// Find a row in grid by its row_index value (column 0)
fn find_row_by_index(grid: &[Vec<String>], row_index: usize) -> Option<&Vec<String>> {
    grid.iter().find(|row| {
        row.get(0)
            .and_then(|idx_str| idx_str.parse::<usize>().ok())
            .map(|idx| idx == row_index)
            .unwrap_or(false)
    })
}

/// Walk up parent chain to build lineage in root-to-leaf order
pub fn walk_parent_lineage(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    parent_row_index: usize,
) -> Vec<LineageEntry> {
    let mut lineage = Vec::new();
    let mut current_category = category.clone();
    let mut current_sheet = sheet_name.to_string();
    let mut current_row_idx = parent_row_index;
    
    bevy::log::info!(
        "🔍 walk_parent_lineage START: category={:?}, sheet='{}', start_row={}",
        category, sheet_name, parent_row_index
    );
    
    // Safety limit to prevent infinite loops
    let mut depth_limit = 10;
    
    while depth_limit > 0 {
        depth_limit -= 1;
        
        // Get current sheet
        let Some(sheet) = registry.get_sheet(&current_category, &current_sheet) else {
            warn!("Lineage walk: Sheet '{}' not found", current_sheet);
            break;
        };
        
        let Some(metadata) = &sheet.metadata else {
            warn!("Lineage walk: Sheet '{}' has no metadata", current_sheet);
            break;
        };
        
        // Find row with matching row_index
        let Some(row) = find_row_by_index(&sheet.grid, current_row_idx) else {
            warn!("Lineage walk: Row with row_index={} not found in sheet '{}'", current_row_idx, current_sheet);
            break;
        };
        
        let display_value = metadata.get_first_data_column_value(row);
        
        bevy::log::info!(
            "🔍   Processing: table='{}', row_idx={}, display='{}', row_data={:?}",
            current_sheet, current_row_idx, display_value, row
        );
        
        lineage.push((current_sheet.clone(), display_value, current_row_idx));
        
        let parent_key_col = metadata.columns.iter()
            .position(|c| c.header.eq_ignore_ascii_case("parent_key"));
        
        if let Some(pk_col) = parent_key_col {
            let parent_key_str = row.get(pk_col).cloned().unwrap_or_default();
            
            if parent_key_str.is_empty() {
                break; // Root table
            }
            
            let Ok(parent_idx) = parent_key_str.parse::<usize>() else {
                warn!("Lineage walk: Invalid parent_key '{}' in sheet '{}'", parent_key_str, current_sheet);
                break;
            };
            
            if let Some(parent_link) = &metadata.structure_parent {
                current_category = parent_link.parent_category.clone();
                current_sheet = parent_link.parent_sheet.clone();
                current_row_idx = parent_idx;
                continue;
            }
            
            // Fallback: parse from table name (e.g., "Games_Platforms" → "Games")
            if let Some((parent_table, _)) = current_sheet.rsplit_once('_') {
                current_sheet = parent_table.to_string();
                current_row_idx = parent_idx;
                continue;
            }
            
            warn!("Lineage walk: Cannot determine parent table for '{}'", current_sheet);
            break;
        } else {
            break; // Root table
        }
    }
    
    if depth_limit == 0 {
        error!("Lineage walk: Hit depth limit (possible circular reference)");
    }
    
    lineage.reverse(); // Convert to root-to-leaf order
    
    bevy::log::info!(
        "🔍 walk_parent_lineage END: {} entries (root-to-leaf): {:?}",
        lineage.len(),
        lineage.iter().map(|(t, d, i)| format!("{}[{}]={}", t, i, d)).collect::<Vec<_>>()
    );
    
    lineage
}

/// Gather AI contexts from lineage chain (from first data column definitions)
pub fn gather_lineage_ai_contexts(
    registry: &SheetRegistry,
    lineage: &[LineageEntry],
) -> Vec<String> {
    let mut contexts = Vec::new();
    
    for (table_name, _, _) in lineage {
        // Parse category and sheet from table_name if needed
        // For now, assume no category (can be enhanced later)
        let category = None;
        
        let ai_context = registry
            .get_sheet(&category, table_name)
            .and_then(|sheet| sheet.metadata.as_ref())
            .and_then(|metadata| {
                // Find first data column using the metadata helper
                let first_idx = metadata.find_first_data_column_index()?;
                metadata.columns.get(first_idx)
            })
            .and_then(|col| col.ai_context.clone())
            .unwrap_or_default();
        
        contexts.push(ai_context);
    }
    
    contexts
}

/// Convert lineage display values back to parent row_index (inverse of walk_parent_lineage)
pub fn resolve_parent_key_from_lineage(
    registry: &SheetRegistry,
    category: &Option<String>,
    parent_sheet_name: &str,
    lineage_values: &[String],
) -> Option<usize> {
    if lineage_values.is_empty() {
        return None;
    }
    
    // Build table chain: "Games_Platforms_Store" → ["Games", "Games_Platforms", "Games_Platforms_Store"]
    let mut table_chain = Vec::new();
    let parts: Vec<&str> = parent_sheet_name.split('_').collect();
    
    for i in 1..=parts.len() {
        table_chain.push(parts[0..i].join("_"));
    }
    
    if lineage_values.len() != table_chain.len() {
        warn!(
            "resolve_parent_key_from_lineage: Lineage length mismatch: got {} values for {} tables (sheet: {})",
            lineage_values.len(),
            table_chain.len(),
            parent_sheet_name
        );
        warn!("  Values: {:?}", lineage_values);
        warn!("  Tables: {:?}", table_chain);
        return None;
    }
    
    let mut current_row_idx: Option<usize> = None;
    
    for (level, (table_name, display_value)) in table_chain.iter().zip(lineage_values.iter()).enumerate() {
        let sheet = registry.get_sheet(category, table_name)?;
        let metadata = sheet.metadata.as_ref()?;
        
        let first_data_col = metadata.find_first_data_column_index()?;
        
        let parent_key_col = if level > 0 {
            metadata.columns.iter().position(|c| c.header.eq_ignore_ascii_case("parent_key"))
        } else {
            None
        };
        
        let found_row = sheet.grid.iter().find(|row| {
            let display_matches = row.get(first_data_col)
                .map(|v| v == display_value)
                .unwrap_or(false);
            
            if !display_matches {
                return false;
            }
            
            if level > 0 {
                if let Some(expected_parent_idx) = current_row_idx {
                    if let Some(pk_col) = parent_key_col {
                        let actual_parent = row.get(pk_col)
                            .and_then(|s| s.parse::<usize>().ok());
                        
                        if actual_parent != Some(expected_parent_idx) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            
            true
        });
        
        current_row_idx = found_row.and_then(|row| {
            row.get(0)?.parse::<usize>().ok()
        });
        
        if current_row_idx.is_none() {
            warn!(
                "resolve_parent_key_from_lineage: Failed to find row at level {} in table '{}' with display value '{}' and parent {:?}",
                level, table_name, display_value, if level > 0 { current_row_idx } else { None }
            );
            return None;
        }
        
        info!(
            "resolve_parent_key_from_lineage: Level {}: Found '{}' in table '{}' at row_index {}",
            level, display_value, table_name, current_row_idx.unwrap()
        );
    }
    
    current_row_idx
}

/// Get unique display values from parent sheet for dropdown options
pub fn get_parent_sheet_options(
    registry: &SheetRegistry,
    category: &Option<String>,
    parent_sheet_name: &str,
) -> Vec<String> {
    let Some(parent_sheet) = registry.get_sheet(category, parent_sheet_name) else {
        return Vec::new();
    };

    let Some(metadata) = &parent_sheet.metadata else {
        return Vec::new();
    };

    // Find first data column
    let Some(display_col_idx) = metadata.find_first_data_column_index() else {
        return Vec::new();
    };

    // Collect unique display values
    let mut options = std::collections::HashSet::new();
    for row in &parent_sheet.grid {
        if let Some(display_value) = row.get(display_col_idx) {
            if !display_value.is_empty() {
                options.insert(display_value.clone());
            }
        }
    }

    // Convert to sorted vector
    let mut result: Vec<String> = options.into_iter().collect();
    result.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    result
}

/// Get display values from parent sheet filtered by parent_key (for hierarchical dropdowns)
pub fn get_parent_sheet_options_filtered(
    registry: &SheetRegistry,
    category: &Option<String>,
    parent_sheet_name: &str,
    parent_key_filter: i64,
) -> Vec<String> {
    let Some(parent_sheet) = registry.get_sheet(category, parent_sheet_name) else {
        return Vec::new();
    };

    let Some(metadata) = &parent_sheet.metadata else {
        return Vec::new();
    };

    // Find first data column
    let Some(display_col_idx) = metadata.find_first_data_column_index() else {
        return Vec::new();
    };

    // Find parent_key column
    let Some(parent_key_col) = metadata.columns.iter()
        .position(|c| c.header.eq_ignore_ascii_case("parent_key"))
    else {
        // No parent_key column, return all options
        return get_parent_sheet_options(registry, category, parent_sheet_name);
    };

    // Collect display values where parent_key matches
    let mut options = std::collections::HashSet::new();
    for row in &parent_sheet.grid {
        if let Some(pk_val) = row.get(parent_key_col).and_then(|v| v.parse::<i64>().ok()) {
            if pk_val == parent_key_filter {
                if let Some(display_value) = row.get(display_col_idx) {
                    if !display_value.is_empty() {
                        options.insert(display_value.clone());
                    }
                }
            }
        }
    }

    // Convert to sorted vector
    let mut result: Vec<String> = options.into_iter().collect();
    result.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    result
}

/// Convert display value to row_index in parent sheet (optionally filtered by parent_key)
pub fn display_value_to_row_index(
    registry: &SheetRegistry,
    category: &Option<String>,
    parent_sheet_name: &str,
    display_value: &str,
    parent_key_filter: Option<i64>,
) -> Option<i64> {
    let Some(parent_sheet) = registry.get_sheet(category, parent_sheet_name) else {
        return None;
    };

    let Some(metadata) = &parent_sheet.metadata else {
        return None;
    };

    // Find first data column
    let display_col_idx = metadata.find_first_data_column_index()?;

    // Find parent_key column if we need to filter
    let parent_key_col = if parent_key_filter.is_some() {
        metadata.columns.iter()
            .position(|c| c.header.eq_ignore_ascii_case("parent_key"))
    } else {
        None
    };

    // Find row matching display value (and optionally parent_key)
    let found_row = parent_sheet.grid.iter().find(|row| {
        // Check display value matches
        let display_matches = row.get(display_col_idx)
            .map(|v| v == display_value)
            .unwrap_or(false);

        if !display_matches {
            return false;
        }

        // If parent_key filter is specified, check it matches
        if let Some(expected_pk) = parent_key_filter {
            if let Some(pk_col) = parent_key_col {
                let actual_pk = row.get(pk_col)
                    .and_then(|s| s.parse::<i64>().ok());
                return actual_pk == Some(expected_pk);
            }
        }

        true
    })?;

    // Extract row_index from column 0
    found_row.get(0)?.parse::<i64>().ok()
}
