// src/sheets/database/writer/cascades.rs
// Cascading updates - maintaining referential integrity across related tables

use super::super::error::DbResult;
use rusqlite::{params, Connection};

/// Update child structure tables to reflect parent column rename.
/// When a column in a parent table is renamed, all child structure tables that reference
/// it in their technical columns (parent_key, grand_1_parent, grand_2_parent, etc.) 
/// need to have those **values** updated to maintain filtering integrity.
///
/// This is a cascading update that ensures referential integrity - the column names
/// in technical columns stay the same (parent_key, grand_N_parent), but the **values**
/// stored in those columns are updated to reflect the new parent column name.
///
/// # Arguments
/// * `conn` - Database connection
/// * `parent_table` - Name of the parent table whose column was renamed
/// * `old_column_name` - Old column name
/// * `new_column_name` - New column name
///
/// # Example
/// If you rename `Platform` → `System` in table `Games`:
/// - Child table `Games_Platforms` has `parent_key` column with values like "Platform"
/// - After cascade: those values become "System"
/// - Grandchild `Games_Platforms_Store` has `grand_1_parent` values updated similarly
pub fn cascade_column_rename_to_children(
    conn: &Connection,
    parent_table: &str,
    old_column_name: &str,
    new_column_name: &str,
) -> DbResult<()> {
    bevy::log::info!(
        "Cascading column rename to child tables: parent='{}', old='{}', new='{}'",
        parent_table, old_column_name, new_column_name
    );

    // Get all structure tables that could be affected
    // Structure tables are named {parent_table}_{column_name}
    let prefix = format!("{}_", parent_table);
    
    let mut stmt = conn.prepare(
        "SELECT table_name FROM _Metadata WHERE table_type = 'structure'"
    )?;
    
    let all_structure_tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    
    bevy::log::debug!(
        "Found {} total structure tables in database",
        all_structure_tables.len()
    );
    
    // Filter to direct children of parent_table
    let child_tables: Vec<String> = all_structure_tables
        .iter()
        .filter(|table| table.starts_with(&prefix))
        .cloned()
        .collect();
    
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
        
        // Check if this table has a parent_key column
        let has_parent_key: bool = conn
            .prepare(&format!("PRAGMA table_info(\"{}\")", child_table))?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|col| col == "parent_key");
        
        if has_parent_key {
            bevy::log::debug!("  Table '{}' has parent_key column", child_table);
            
            // Update parent_key values where they match the old column name
            let updated = conn.execute(
                &format!(
                    "UPDATE \"{}\" SET parent_key = ? WHERE parent_key = ?",
                    child_table
                ),
                params![new_column_name, old_column_name],
            )?;
            
            if updated > 0 {
                bevy::log::info!(
                    "  ✓ Updated {} parent_key references in '{}'",
                    updated, child_table
                );
                total_updates += updated;
            }
        }
        
        // Recursively update grandchildren and deeper descendants
        total_updates += cascade_to_descendants(
            conn,
            child_table,
            &all_structure_tables,
            old_column_name,
            new_column_name,
        )?;
    }
    
    if total_updates > 0 {
        bevy::log::info!(
            "✓ Cascade complete: Updated {} references for '{}' → '{}'",
            total_updates, old_column_name, new_column_name
        );
    } else {
        bevy::log::debug!(
            "No cascade updates needed for '{}' → '{}'",
            old_column_name, new_column_name
        );
    }
    
    Ok(())
}

/// Recursively cascade updates to descendant tables (grandchildren and beyond)
fn cascade_to_descendants(
    conn: &Connection,
    parent_table: &str,
    all_structure_tables: &[String],
    old_column_name: &str,
    new_column_name: &str,
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
        
        // Update all grand_N_parent columns
        for column_name in &columns {
            if column_name.starts_with("grand_") && column_name.ends_with("_parent") {
                bevy::log::debug!(
                    "    Checking column '{}' in '{}'",
                    column_name, descendant_table
                );
                
                // Update values where they match the old column name
                let updated = conn.execute(
                    &format!(
                        "UPDATE \"{}\" SET \"{}\" = ? WHERE \"{}\" = ?",
                        descendant_table, column_name, column_name
                    ),
                    params![new_column_name, old_column_name],
                )?;
                
                if updated > 0 {
                    bevy::log::info!(
                        "    ✓ Updated {} {} references in '{}'",
                        updated, column_name, descendant_table
                    );
                    total_updates += updated;
                }
            }
        }
        
        // Recursively process even deeper descendants
        total_updates += cascade_to_descendants(
            conn,
            descendant_table,
            all_structure_tables,
            old_column_name,
            new_column_name,
        )?;
    }
    
    Ok(total_updates)
}
