// src/cli/list_columns.rs
use rusqlite::{Connection, Result};
use std::path::PathBuf;

pub fn run(db_path: PathBuf) -> Result<()> {
    println!("Opening: {}\n", db_path.display());
    
    let conn = Connection::open(&db_path)?;
    
    println!("=== ShipUnits_Metadata Contents ===\n");
    
    let mut stmt = conn.prepare(
        "SELECT column_index, column_name, display_name, data_type, deleted FROM ShipUnits_Metadata ORDER BY column_index"
    )?;
    
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i32>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i32>>(4)?,
        ))
    })?;
    
    println!("{:<6} {:<20} {:<20} {:<15} {}", "Index", "Column Name", "Display Name", "Data Type", "Deleted");
    println!("{}", "-".repeat(80));
    
    for row in rows {
        let (idx, name, display, dtype, deleted) = row?;
        println!(
            "{:<6} {:<20} {:<20} {:<15} {}",
            idx,
            name,
            display.as_deref().unwrap_or("NULL"),
            dtype.as_deref().unwrap_or("NULL"),
            deleted.unwrap_or(0)
        );
    }
    
    println!("\n=== ShipUnits Physical Columns ===\n");
    
    let mut stmt = conn.prepare("PRAGMA table_info(ShipUnits)")?;
    let cols = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    
    for col in cols {
        let (name, sql_type) = col?;
        println!("  {}: {}", name, sql_type);
    }
    
    Ok(())
}
