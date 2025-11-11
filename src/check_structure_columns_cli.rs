use rusqlite::{Connection, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    let db_path = PathBuf::from("C:\\Users\\Serge\\Documents\\SkylineDB\\Tactical Frontlines.db");
    
    println!("=== Check Structure Columns ===\n");
    
    let conn = Connection::open(&db_path)?;
    
    let mut stmt = conn.prepare(
        "SELECT column_index, column_name, data_type, validator_type, validator_config FROM ShipUnits_Metadata ORDER BY column_index"
    )?;
    
    println!("{:<6} {:<20} {:<15} {:<20} {}", "Index", "Name", "DataType", "ValidatorType", "Config (first 50 chars)");
    println!("{}", "=".repeat(120));
    
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i32>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    
    for row in rows {
        let (idx, name, dtype, vtype, vconfig) = row?;
        let config_preview = vconfig.as_ref().map(|c| {
            if c.len() > 50 {
                format!("{}...", &c[..50])
            } else {
                c.clone()
            }
        }).unwrap_or_else(|| "NULL".to_string());
        
        println!(
            "{:<6} {:<20} {:<15} {:<20} {}",
            idx,
            name,
            dtype.as_deref().unwrap_or("NULL"),
            vtype.as_deref().unwrap_or("NULL"),
            config_preview
        );
    }
    
    Ok(())
}
