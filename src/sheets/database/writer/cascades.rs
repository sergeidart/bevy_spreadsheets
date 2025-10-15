// src/sheets/database/writer/cascades.rs
// Cascading updates - maintaining referential integrity across related tables

use super::super::error::DbResult;
use rusqlite::{params, Connection};

/// Cascade parent key value change to child and descendant structure tables.
/// When a cell value changes in a parent table's key column, ALL child structure tables
/// that reference this parent via parent_key must have their parent_key values updated.
/// Similarly, grandchildren and deeper descendants with parent_key or grand_N_parent columns must be updated.
///
/// # Arguments
/// * `conn` - Database connection
/// * `parent_table` - Name of the parent table whose key value changed
/// * `parent_column_name` - The column name in parent table that serves as the key (for logging only)
/// * `old_value` - Old key value
/// * `new_value` - New key value
///
/// # Example
/// If you change a value in the "Name" column of table "Games" from "Portal" to "Portal 2":
/// - ALL child tables like `Games_Platforms`, `Games_Items` have rows with `parent_key = "Portal"` 
/// - After cascade: those rows have `parent_key = "Portal 2"`
/// - Grandchild tables like `Games_Platforms_Stores` update their `parent_key` and `grand_1_parent` values similarly
pub fn cascade_key_value_change_to_children(
    conn: &Connection,
    parent_table: &str,
    parent_column_name: &str,
    old_value: &str,
    new_value: &str,
) -> DbResult<()> {
    bevy::log::info!(
        "Cascading key value change to child tables: parent='{}', column='{}', old='{}', new='{}'",
        parent_table, parent_column_name, old_value, new_value
    );

    // Get all structure tables in database (for recursive descent lookups)
    let mut stmt = conn.prepare(
        "SELECT table_name FROM _Metadata WHERE table_type = 'structure'"
    )?;
    
    let all_structure_tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    
    // Get all structure tables that are direct children of parent_table
    // Query _Metadata.parent_table to find direct children reliably
    // This is more accurate than string matching because table/column names can contain underscores
    let mut child_stmt = conn.prepare(
        "SELECT table_name FROM _Metadata WHERE table_type = 'structure' AND parent_table = ?"
    )?;
    
    let child_tables: Vec<String> = child_stmt
        .query_map([parent_table], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    
    if child_tables.is_empty() {
        bevy::log::debug!(
            "No child structure tables found for parent='{}'",
            parent_table
        );
        return Ok(());
    }
    
    bevy::log::info!(
        "Found {} direct child tables of '{}': {:?}",
        child_tables.len(),
        parent_table,
        child_tables
    );
    
    let mut total_updates = 0;
    
    // Update parent_key values in direct children
    for child_table in &child_tables {
        bevy::log::debug!("Processing child table '{}'", child_table);
        
        // Update parent_key values where they match the old value
        let updated = conn.execute(
            &format!(
                "UPDATE \"{}\" SET parent_key = ? WHERE parent_key = ?",
                child_table
            ),
            params![new_value, old_value],
        )?;
        
        if updated > 0 {
            bevy::log::info!(
                "  ✓ Updated {} parent_key values in '{}'",
                updated, child_table
            );
            total_updates += updated;
        }
        
        // Recursively update grandchildren and deeper descendants
        total_updates += cascade_value_to_descendants(
            conn,
            child_table,
            &all_structure_tables,
            old_value,
            new_value,
        )?;
    }
    
    if total_updates > 0 {
        bevy::log::info!(
            "✓ Cascade complete: Updated {} references for value '{}' → '{}'",
            total_updates, old_value, new_value
        );
    } else {
        bevy::log::debug!(
            "No cascade updates needed for value '{}' → '{}'",
            old_value, new_value
        );
    }
    
    Ok(())
}

/// Recursively cascade value updates to descendant tables (grandchildren and beyond)
fn cascade_value_to_descendants(
    conn: &Connection,
    parent_table: &str,
    all_structure_tables: &[String],
    old_value: &str,
    new_value: &str,
) -> DbResult<usize> {
    let prefix = format!("{}_", parent_table);
    let descendants: Vec<String> = all_structure_tables
        .iter()
        .filter(|table| table.starts_with(&prefix))
        .cloned()
        .collect();
    
    if descendants.is_empty() {
        return Ok(0);
    }
    
    bevy::log::debug!(
        "  Found {} descendants of '{}': {:?}",
        descendants.len(),
        parent_table,
        descendants
    );
    
    let mut total_updates = 0;
    
    for descendant_table in &descendants {
        // Get all columns in this descendant table
        let columns: Vec<String> = conn
            .prepare(&format!("PRAGMA table_info(\"{}\")", descendant_table))?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        
        // First, update parent_key column if it exists and matches the old value
        if columns.iter().any(|col| col == "parent_key") {
            bevy::log::debug!(
                "    Checking parent_key column in '{}'",
                descendant_table
            );
            
            let updated = conn.execute(
                &format!(
                    "UPDATE \"{}\" SET parent_key = ? WHERE parent_key = ?",
                    descendant_table
                ),
                params![new_value, old_value],
            )?;
            
            if updated > 0 {
                bevy::log::info!(
                    "    ✓ Updated {} parent_key values in '{}'",
                    updated, descendant_table
                );
                total_updates += updated;
            }
        }
        
        // Update all grand_N_parent columns
        for column_name in &columns {
            if column_name.starts_with("grand_") && column_name.ends_with("_parent") {
                bevy::log::debug!(
                    "    Checking column '{}' in '{}'",
                    column_name, descendant_table
                );
                
                // Update values where they match the old value
                let updated = conn.execute(
                    &format!(
                        "UPDATE \"{}\" SET \"{}\" = ? WHERE \"{}\" = ?",
                        descendant_table, column_name, column_name
                    ),
                    params![new_value, old_value],
                )?;
                
                if updated > 0 {
                    bevy::log::info!(
                        "    ✓ Updated {} {} values in '{}'",
                        updated, column_name, descendant_table
                    );
                    total_updates += updated;
                }
            }
        }
        
        // Recursively process even deeper descendants
        total_updates += cascade_value_to_descendants(
            conn,
            descendant_table,
            all_structure_tables,
            old_value,
            new_value,
        )?;
    }
    
    Ok(total_updates)
}
