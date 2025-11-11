// src/cli/sync_column_names.rs
use rusqlite::{Connection, Result};
use std::path::PathBuf;

pub fn run(db_path: PathBuf) -> Result<()> {
    println!("=== Column Name Sync Tool ===\n");
    println!("Opening: {}\n", db_path.display());
    
    let conn = Connection::open(&db_path)?;
    
    // Read metadata
    let mut stmt = conn.prepare(
        "SELECT column_index, column_name FROM ShipUnits_Metadata WHERE deleted = 0 OR deleted IS NULL ORDER BY column_index"
    )?;
    
    let metadata_cols: Vec<(i32, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    
    // Read physical columns
    let mut stmt = conn.prepare("PRAGMA table_info(ShipUnits)")?;
    let physical_cols: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    
    println!("Metadata columns: {:?}", metadata_cols.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>());
    println!("Physical columns: {:?}\n", physical_cols);
    
    // Find mismatches
    for (idx, meta_name) in &metadata_cols {
        // Skip technical columns
        if matches!(meta_name.as_str(), "id" | "row_index" | "created_at" | "updated_at" | "parent_id") {
            continue;
        }
        
        if !physical_cols.iter().any(|p| p.eq_ignore_ascii_case(meta_name)) {
            println!("⚠ Column '{}' (index {}) exists in metadata but NOT in physical table", meta_name, idx);
            
            // Find if there's an orphaned column that might be this one
            for phys in &physical_cols {
                if matches!(phys.as_str(), "id" | "row_index" | "created_at" | "updated_at" | "parent_id") {
                    continue;
                }
                if !metadata_cols.iter().any(|(_, m)| m.eq_ignore_ascii_case(phys)) {
                    println!("  Possible match: Physical column '{}' is orphaned", phys);
                    println!("  Suggestion: Rename physical '{}' -> '{}'", phys, meta_name);
                    
                    // Ask for confirmation
                    println!("\n  Apply fix? (yes/no): ");
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).ok();
                    
                    if input.trim().eq_ignore_ascii_case("yes") || input.trim().eq_ignore_ascii_case("y") {
                        println!("  Executing: ALTER TABLE ShipUnits RENAME COLUMN \"{}\" TO \"{}\"", phys, meta_name);
                        conn.execute(
                            &format!("ALTER TABLE ShipUnits RENAME COLUMN \"{}\" TO \"{}\"", phys, meta_name),
                            []
                        )?;
                        println!("  ✓ Fixed!\n");
                        break;
                    } else {
                        println!("  Skipped.\n");
                    }
                }
            }
        }
    }
    
    println!("=== Complete ===");
    Ok(())
}
