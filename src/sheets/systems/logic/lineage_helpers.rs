// src/sheets/systems/logic/lineage_helpers.rs
//! Helper functions for building parent lineage chains dynamically
//!
//! **Post-Refactor (2025-10-28):**
//! Now that grand_N_parent columns are removed, we build lineage by walking
//! the parent chain using parent_key references.
//!
//! Example lineage for Games_Platforms_Store:
//! - Row has parent_key = 123 (points to Games_Platforms row)
//! - Games_Platforms row 123 has parent_key = 5 (points to Games row)
//! - Games row 5 has no parent_key (root table)
//! - Result: ["Mass Effect 3", "PC", "Steam"]

use bevy::prelude::*;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::definitions::SheetMetadata;

/// Lineage entry: (table_name, display_value, row_index)
pub type LineageEntry = (String, String, usize);

/// Walk up the parent chain to build full lineage from root to current position
///
/// Returns lineage in ROOT-TO-LEAF order: [root, mid-level, immediate-parent]
///
/// # Arguments
/// * `registry` - Sheet registry for looking up parent tables
/// * `category` - Current category (optional)
/// * `sheet_name` - Current sheet name
/// * `parent_row_index` - Row index of the parent row to start walking from
///
/// # Returns
/// Vector of (table_name, display_value, row_index) tuples in root-to-leaf order
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
        
        // Find grid index where row_index column equals current_row_idx
        // (Grid is sorted DESC, so we can't use row_idx directly as array index)
        let grid_index = sheet.grid.iter().position(|row| {
            row.get(0).and_then(|s| s.parse::<usize>().ok()) == Some(current_row_idx)
        });
        
        let Some(grid_idx) = grid_index else {
            warn!("Lineage walk: Row with row_index={} not found in sheet '{}'", current_row_idx, current_sheet);
            break;
        };
        
        // Get current row using the found grid index
        let Some(row) = sheet.grid.get(grid_idx) else {
            warn!("Lineage walk: Grid index {} out of bounds in sheet '{}'", grid_idx, current_sheet);
            break;
        };
        
        // Get display value from first data column
        let display_value = crate::ui::elements::editor::structure_navigation::get_first_content_column_value(metadata, row);
        
        bevy::log::info!(
            "🔍   Processing: table='{}', row_idx={}, display='{}', row_data={:?}",
            current_sheet, current_row_idx, display_value, row
        );
        
        // Add to lineage (we'll reverse at the end)
        lineage.push((current_sheet.clone(), display_value, current_row_idx));
        
        // Check if this sheet has a parent (structure table)
        let parent_key_col = metadata.columns.iter()
            .position(|c| c.header.eq_ignore_ascii_case("parent_key"));
        
        if let Some(pk_col) = parent_key_col {
            // Get parent_key value
            let parent_key_str = row.get(pk_col).cloned().unwrap_or_default();
            
            if parent_key_str.is_empty() {
                // No parent, we've reached the root
                break;
            }
            
            // Parse parent_key as row_index
            let Ok(parent_idx) = parent_key_str.parse::<usize>() else {
                warn!("Lineage walk: Invalid parent_key '{}' in sheet '{}'", parent_key_str, current_sheet);
                break;
            };
            
            // Determine parent table name from metadata
            if let Some(parent_link) = &metadata.structure_parent {
                current_category = parent_link.parent_category.clone();
                current_sheet = parent_link.parent_sheet.clone();
                current_row_idx = parent_idx;
                continue;
            }
            
            // Fallback: parse parent table from table name (e.g., "Games_Platforms" -> "Games")
            if let Some((parent_table, _)) = current_sheet.rsplit_once('_') {
                current_sheet = parent_table.to_string();
                current_row_idx = parent_idx;
                continue;
            }
            
            // Can't determine parent table
            warn!("Lineage walk: Cannot determine parent table for '{}'", current_sheet);
            break;
        } else {
            // No parent_key column, this is a root table
            break;
        }
    }
    
    if depth_limit == 0 {
        error!("Lineage walk: Hit depth limit (possible circular reference)");
    }
    
    // Reverse to get root-to-leaf order
    lineage.reverse();
    
    bevy::log::info!(
        "🔍 walk_parent_lineage END: {} entries (root-to-leaf): {:?}",
        lineage.len(),
        lineage.iter().map(|(t, d, i)| format!("{}[{}]={}", t, i, d)).collect::<Vec<_>>()
    );
    
    lineage
}

/// Format lineage as display string with separator
///
/// Example: "Mass Effect 3 › PC › Steam"
pub fn format_lineage_display(lineage: &[LineageEntry], separator: &str) -> String {
    lineage.iter()
        .map(|(_, display_val, _)| display_val.as_str())
        .collect::<Vec<_>>()
        .join(separator)
}

/// Format lineage for AI context (comma-separated)
///
/// Example: "Mass Effect 3, PC, Steam"
pub fn format_lineage_for_ai(lineage: &[LineageEntry]) -> String {
    lineage.iter()
        .map(|(_, display_val, _)| display_val.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build lineage for the current row's context (used in navigation)
///
/// This extracts the parent lineage from a parent row for navigating into structure
pub fn build_navigation_lineage(
    registry: &SheetRegistry,
    parent_category: &Option<String>,
    parent_sheet_name: &str,
    parent_row_index: usize,
) -> Vec<String> {
    let lineage = walk_parent_lineage(registry, parent_category, parent_sheet_name, parent_row_index);
    
    // Return just the display values
    lineage.iter()
        .map(|(_, display_val, _)| display_val.clone())
        .collect()
}

/// Resolve parent_key (row_index) to display value for a single level
///
/// Given a parent_key value (row_index), looks up the parent row and returns
/// its display value (first data column).
///
/// # Arguments
/// * `registry` - Sheet registry for looking up tables
/// * `category` - Parent category (optional)
/// * `parent_sheet_name` - Name of the parent table
/// * `parent_key_row_idx` - The row_index value stored in parent_key column
///
/// # Returns
/// Display value of the parent row, or empty string if not found
pub fn resolve_parent_display_value(
    registry: &SheetRegistry,
    category: &Option<String>,
    parent_sheet_name: &str,
    parent_key_row_idx: usize,
) -> String {
    let Some(parent_sheet) = registry.get_sheet(category, parent_sheet_name) else {
        warn!("resolve_parent_display_value: Sheet '{}' not found", parent_sheet_name);
        return String::new();
    };
    
    let Some(parent_metadata) = &parent_sheet.metadata else {
        warn!("resolve_parent_display_value: Sheet '{}' has no metadata", parent_sheet_name);
        return String::new();
    };
    
    // Find row with matching row_index (column 0)
    let parent_row = parent_sheet.grid.iter().find(|row| {
        row.get(0)
            .and_then(|idx_str| idx_str.parse::<usize>().ok())
            .map(|idx| idx == parent_key_row_idx)
            .unwrap_or(false)
    });
    
    if let Some(row) = parent_row {
        crate::ui::elements::editor::structure_navigation::get_first_content_column_value(parent_metadata, row)
    } else {
        warn!(
            "resolve_parent_display_value: Row index {} not found in sheet '{}'",
            parent_key_row_idx, parent_sheet_name
        );
        String::new()
    }
}

/// Gather AI contexts from the lineage chain
///
/// Walks through the lineage and collects the ai_context from each level's
/// first data column definition.
///
/// # Arguments
/// * `registry` - Sheet registry for looking up metadata
/// * `lineage` - Lineage entries (from walk_parent_lineage)
///
/// # Returns
/// Vector of AI context strings, one per lineage level (may be empty strings if no context defined)
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
                // Find first data column
                metadata.columns.iter().find(|col| {
                    if col.deleted || col.hidden {
                        return false;
                    }
                    let h = col.header.to_lowercase();
                    h != "row_index" && h != "id" && h != "parent_key"
                        && h != "temp_new_row_index" && h != "_obsolete_temp_new_row_index"
                })
            })
            .and_then(|col| col.ai_context.clone())
            .unwrap_or_default();
        
        contexts.push(ai_context);
    }
    
    contexts
}

/// Build full AI attachment chain (lineage display values + current row data)
///
/// Combines parent lineage display values with the current row's data values
/// to create the full chain that AI needs for context.
///
/// # Arguments
/// * `lineage` - Parent lineage entries (from walk_parent_lineage)
/// * `current_row_values` - Current row's data column values (excluding technical columns)
///
/// # Returns
/// Vector combining lineage display values followed by current row values
pub fn build_ai_attachment_chain(
    lineage: &[LineageEntry],
    current_row_values: &[String],
) -> Vec<String> {
    let mut chain = Vec::new();
    
    // Add parent lineage display values
    for (_, display_val, _) in lineage {
        chain.push(display_val.clone());
    }
    
    // Add current row values
    chain.extend_from_slice(current_row_values);
    
    chain
}

/// Resolve parent_key from lineage display values
///
/// Given ancestor display values (e.g., ["Mass Effect 3", "PC"]), finds the parent row's row_index
/// by walking down the hierarchy and matching display values.
///
/// This is the INVERSE of walk_parent_lineage - it converts display names back to row_index.
///
/// # Arguments
/// * `registry` - Sheet registry for looking up tables
/// * `category` - Current category (optional)
/// * `parent_sheet_name` - Name of the immediate parent table (e.g., "Games_Platforms")
/// * `lineage_values` - Display values from root to immediate parent (e.g., ["Mass Effect 3", "PC"])
///
/// # Returns
/// The row_index of the parent row if found, None otherwise
///
/// # Example
/// ```
/// // For Games_Platforms_Store with lineage ["Mass Effect 3", "PC"]
/// // This finds the row_index of the "PC" row under "Mass Effect 3" in Games_Platforms table
/// let parent_key = resolve_parent_key_from_lineage(
///     registry,
///     &category,
///     "Games_Platforms",
///     &["Mass Effect 3", "PC"]
/// );
/// ```
pub fn resolve_parent_key_from_lineage(
    registry: &SheetRegistry,
    category: &Option<String>,
    parent_sheet_name: &str,
    lineage_values: &[String],
) -> Option<usize> {
    if lineage_values.is_empty() {
        return None;
    }
    
    // Build table chain from parent sheet name
    // e.g., "Games_Platforms_Store" -> ["Games", "Games_Platforms", "Games_Platforms_Store"]
    let mut table_chain = Vec::new();
    let mut parts: Vec<&str> = parent_sheet_name.split('_').collect();
    
    // Build cumulative table names
    for i in 1..=parts.len() {
        table_chain.push(parts[0..i].join("_"));
    }
    
    // lineage_values should match the table_chain length
    // Each lineage value corresponds to a table level
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
    
    // Walk down the hierarchy to find the matching parent row
    let mut current_row_idx: Option<usize> = None;
    
    for (level, (table_name, display_value)) in table_chain.iter().zip(lineage_values.iter()).enumerate() {
        let sheet = registry.get_sheet(category, table_name)?;
        let metadata = sheet.metadata.as_ref()?;
        
        // Find first data column index (skip technical columns)
        let first_data_col = metadata.columns.iter().enumerate().find_map(|(idx, col)| {
            if col.deleted || col.hidden {
                return None;
            }
            let h = col.header.to_lowercase();
            if h == "row_index" || h == "parent_key" || h == "id" 
                || h == "temp_new_row_index" || h == "_obsolete_temp_new_row_index" {
                return None;
            }
            Some(idx)
        })?;
        
        // Find parent_key column (if not at root level)
        let parent_key_col = if level > 0 {
            metadata.columns.iter().position(|c| c.header.eq_ignore_ascii_case("parent_key"))
        } else {
            None
        };
        
        // Find row that matches the display value and parent constraint
        let found_row = sheet.grid.iter().find(|row| {
            // Check if display value matches
            let display_matches = row.get(first_data_col)
                .map(|v| v == display_value)
                .unwrap_or(false);
            
            if !display_matches {
                return false;
            }
            
            // If this is not the root level, also check parent_key matches
            if level > 0 {
                if let Some(expected_parent_idx) = current_row_idx {
                    if let Some(pk_col) = parent_key_col {
                        let actual_parent = row.get(pk_col)
                            .and_then(|s| s.parse::<usize>().ok());
                        
                        if actual_parent != Some(expected_parent_idx) {
                            return false; // Parent doesn't match
                        }
                    } else {
                        return false; // No parent_key column found
                    }
                } else {
                    return false; // Expected parent but none found
                }
            }
            
            true
        });
        
        // Extract row_index from the found row
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_format_lineage_display() {
        let lineage = vec![
            ("Games".to_string(), "Mass Effect 3".to_string(), 5),
            ("Games_Platforms".to_string(), "PC".to_string(), 123),
            ("Games_Platforms_Store".to_string(), "Steam".to_string(), 456),
        ];
        
        assert_eq!(
            format_lineage_display(&lineage, " › "),
            "Mass Effect 3 › PC › Steam"
        );
    }
    
    #[test]
    fn test_format_lineage_for_ai() {
        let lineage = vec![
            ("Games".to_string(), "Mass Effect 3".to_string(), 5),
            ("Games_Platforms".to_string(), "PC".to_string(), 123),
        ];
        
        assert_eq!(
            format_lineage_for_ai(&lineage),
            "Mass Effect 3, PC"
        );
    }
}
