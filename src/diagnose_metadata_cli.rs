// src/diagnose_metadata_cli.rs
// Diagnostic tool to see actual metadata table contents
// Usage: cargo run --bin diagnose_metadata

use rusqlite::{Connection, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    println!("=== Metadata Table Diagnostic Tool ===\n");
    
    let args: Vec<String> = std::env::args().collect();
    let db_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from(format!(
            "{}\\Documents\\SkylineDB\\Tactical Frontlines.db",
            std::env::var("USERPROFILE").unwrap_or_default()
        ))
    };
    
    println!("Opening: {}\n", db_path.display());
    
    let conn = Connection::open(&db_path)?;
    
    // Get ShipUnits_Metadata specifically
    let table_name = "ShipUnits_Metadata";
    
    println!("=== Table: {} ===\n", table_name);
    
    // Show schema
    println!("Schema:");
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let cid: i32 = row.get(0)?;
        let name: String = row.get(1)?;
        let type_name: String = row.get(2)?;
        let notnull: i32 = row.get(3)?;
        let pk: i32 = row.get(5)?;
        println!("  {} {} {} {} {}", cid, name, type_name, 
                 if notnull == 1 { "NOT NULL" } else { "" },
                 if pk == 1 { "PRIMARY KEY" } else { "" });
    }
    
    println!("\nFirst 5 rows with type information:");
    let mut stmt = conn.prepare(&format!(
        "SELECT column_index, typeof(column_index), column_name, data_type, deleted 
         FROM \"{}\" 
         LIMIT 5",
        table_name
    ))?;
    
    let mut rows = stmt.query([])?;
    let mut row_num = 0;
    while let Some(row) = rows.next()? {
        row_num += 1;
        let col_idx_raw: String = row.get(0).unwrap_or_else(|_| "ERROR".to_string());
        let col_idx_type: String = row.get(1)?;
        let col_name: String = row.get(2)?;
        let data_type: String = row.get(3).unwrap_or_else(|_| "NULL".to_string());
        let deleted: i32 = row.get(4).unwrap_or(0);
        
        println!("  Row {}: column_index='{}' (type={}), name='{}', data_type='{}', deleted={}",
                 row_num, col_idx_raw, col_idx_type, col_name, data_type, deleted);
    }
    
    println!("\nAll rows count:");
    let count: i32 = conn.query_row(
        &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
        [],
        |row| row.get(0),
    )?;
    println!("  Total rows: {}", count);
    
    println!("\nRows where typeof(column_index) != 'integer':");
    let mut stmt = conn.prepare(&format!(
        "SELECT column_index, typeof(column_index), column_name 
         FROM \"{}\" 
         WHERE typeof(column_index) != 'integer'",
        table_name
    ))?;
    
    let mut rows = stmt.query([])?;
    let mut bad_count = 0;
    while let Some(row) = rows.next()? {
        bad_count += 1;
        let col_idx_raw: String = row.get(0).unwrap_or_else(|_| "ERROR".to_string());
        let col_idx_type: String = row.get(1)?;
        let col_name: String = row.get(2)?;
        println!("  ⚠ column_index='{}' (type={}), name='{}'", col_idx_raw, col_idx_type, col_name);
    }
    
    if bad_count == 0 {
        println!("  ✓ All rows have INTEGER column_index");
    } else {
        println!("\n  ⚠ Found {} row(s) with non-INTEGER column_index!", bad_count);
    }
    
    Ok(())
}
