// src/cli/add_display_name.rs
use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};

pub fn run(data_path: PathBuf) -> Result<()> {
    println!("=== Add display_name Column Tool ===\n");
    println!("Data directory: {}\n", data_path.display());
    
    // Find all .db files
    let db_files: Vec<PathBuf> = std::fs::read_dir(&data_path)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("db"))
        .collect();
    
    if db_files.is_empty() {
        println!("No database files found in {}", data_path.display());
        return Ok(());
    }
    
    println!("Found {} database file(s):\n", db_files.len());
    for (i, db) in db_files.iter().enumerate() {
        println!("  {}. {}", i + 1, db.display());
    }
    println!();
    
    // Process ShipUnits_Metadata specifically
    let db_path = db_files.iter()
        .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some("Tactical Frontlines"))
        .expect("Could not find 'Tactical Frontlines.db'");
    
    add_display_name_column(db_path)?;
    
    println!("\n=== Complete ===");
    Ok(())
}

fn add_display_name_column(db_path: &Path) -> Result<()> {
    println!("Processing: {}", db_path.display());
    
    let conn = Connection::open(db_path)?;
    let table_name = "ShipUnits_Metadata";
    
    // Check if display_name column already exists
    let check_sql = format!("PRAGMA table_info(\"{}\")", table_name);
    let mut stmt = conn.prepare(&check_sql)?;
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    
    if columns.contains(&"display_name".to_string()) {
        println!("  ✓ display_name column already exists");
        return Ok(());
    }
    
    println!("  ⚠ display_name column missing - adding...");
    
    // Add the column
    let add_sql = format!("ALTER TABLE \"{}\" ADD COLUMN display_name TEXT", table_name);
    conn.execute(&add_sql, [])?;
    
    println!("  ✓ ADDED display_name column");
    
    Ok(())
}
