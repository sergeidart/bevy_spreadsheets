// src/cli/restore_columns.rs
use rusqlite::{Connection, Result};
use std::path::PathBuf;

pub fn run(db_path: PathBuf) -> Result<()> {
    println!("=== Restore Missing Columns Tool ===\n");
    println!("Opening: {}\n", db_path.display());
    
    let conn = Connection::open(&db_path)?;
    
    // Read metadata
    let mut stmt = conn.prepare(
        "SELECT column_name, data_type FROM ShipUnits_Metadata WHERE (deleted = 0 OR deleted IS NULL) ORDER BY column_index"
    )?;
    
    let metadata_cols: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "String".to_string()))))?
        .collect::<Result<Vec<_>, _>>()?;
    
    // Read physical columns
    let mut stmt = conn.prepare("PRAGMA table_info(ShipUnits)")?;
    let physical_cols: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    
    println!("Metadata columns: {:?}", metadata_cols.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>());
    println!("Physical columns: {:?}\n", physical_cols);
    
    // Find missing columns
    for (col_name, data_type) in &metadata_cols {
        // Skip technical columns
        if matches!(col_name.as_str(), "id" | "row_index" | "created_at" | "updated_at" | "parent_id") {
            continue;
        }
        
        if !physical_cols.iter().any(|p| p.eq_ignore_ascii_case(col_name)) {
            let sql_type = match data_type.as_str() {
                "Int" => "INTEGER",
                "Float" => "REAL",
                "Bool" => "INTEGER",
                _ => "TEXT",
            };
            
            println!("⚠ Column '{}' ({}) missing from physical table", col_name, data_type);
            println!("  Adding: ALTER TABLE ShipUnits ADD COLUMN \"{}\" {}", col_name, sql_type);
            
            conn.execute(
                &format!("ALTER TABLE ShipUnits ADD COLUMN \"{}\" {}", col_name, sql_type),
                []
            )?;
            
            println!("  ✓ Added!\n");
        }
    }
    
    println!("=== Complete ===");
    Ok(())
}
