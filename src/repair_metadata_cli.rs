// src/repair_metadata_cli.rs
// One-time CLI tool to repair corrupted metadata tables
// Usage: cargo run --bin repair_metadata

use rusqlite::{Connection, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    println!("=== Metadata Table Repair Tool ===\n");
    
    // Get the data directory from arguments or use default
    let args: Vec<String> = std::env::args().collect();
    let data_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        get_data_path()
    };
    
    println!("Data directory: {}\n", data_path.display());
    
    if !data_path.exists() {
        println!("Error: Directory does not exist: {}", data_path.display());
        println!("\nUsage: {} [path/to/data/directory]", args.get(0).map(|s| s.as_str()).unwrap_or("repair_metadata"));
        std::process::exit(1);
    }
    
    // Scan for .db files
    let db_files = find_db_files(&data_path)?;
    
    if db_files.is_empty() {
        println!("No database files found in {}", data_path.display());
        println!("\nUsage: {} [path/to/data/directory]", args.get(0).map(|s| s.as_str()).unwrap_or("repair_metadata"));
        return Ok(());
    }
    
    println!("Found {} database file(s):\n", db_files.len());
    for (i, db) in db_files.iter().enumerate() {
        println!("  {}. {}", i + 1, db.display());
    }
    println!();
    
    // Process each database
    for db_path in db_files {
        process_database(&db_path)?;
    }
    
    println!("\n=== Repair Complete ===");
    Ok(())
}

fn get_data_path() -> PathBuf {
    let path = std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("."))
        .parent()
        .unwrap_or(&PathBuf::from("."))
        .to_path_buf();
    
    // Try common locations
    let data_dir = path.join("data");
    if data_dir.exists() {
        return data_dir;
    }
    
    // Try from project root
    let project_data = PathBuf::from("data");
    if project_data.exists() {
        return project_data;
    }
    
    // Default to current directory
    PathBuf::from(".")
}

fn find_db_files(path: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut db_files = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("db") {
                db_files.push(path);
            }
        }
    }
    
    Ok(db_files)
}

fn process_database(db_path: &PathBuf) -> Result<()> {
    println!("Processing: {}", db_path.display());
    
    let conn = Connection::open(db_path)?;
    
    // Find all metadata tables
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE '%_Metadata'"
    )?;
    
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>>>()?;
    
    if tables.is_empty() {
        println!("  No metadata tables found.\n");
        return Ok(());
    }
    
    println!("  Found {} metadata table(s)", tables.len());
    
    for table in tables {
        repair_metadata_table(&conn, &table)?;
    }
    
    println!();
    Ok(())
}

fn repair_metadata_table(conn: &Connection, table_name: &str) -> Result<()> {
    println!("    Checking '{}'...", table_name);
    
    // Skip the global _Metadata table (it has a different schema)
    if table_name == "_Metadata" {
        println!("      ⊘ SKIPPED (global metadata table, different schema)");
        return Ok(());
    }
    
    // Check if ANY row has TEXT values in column_index (not just the first row)
    let check_query = format!(
        "SELECT COUNT(*) FROM \"{}\" WHERE typeof(column_index) != 'integer'",
        table_name
    );
    
    let bad_count: i64 = conn.query_row(&check_query, [], |row| row.get(0))?;
    
    if bad_count == 0 {
        println!("      ✓ OK (all column_index values are INTEGER)");
        return Ok(());
    }
    
    println!("      ⚠ CORRUPTED ({} row(s) with TEXT in column_index) - repairing...", bad_count);
    
    // Perform repair
    repair_corrupted_table(conn, table_name)?;
    
    println!("      ✓ REPAIRED");
    
    Ok(())
}

fn repair_corrupted_table(conn: &Connection, table_name: &str) -> Result<()> {
    let backup_table = format!("{}_backup_temp", table_name);
    
    // Temporarily disable foreign keys to allow table rebuild
    conn.execute("PRAGMA foreign_keys = OFF", [])?;
    
    // Start transaction
    conn.execute("BEGIN TRANSACTION", [])?;
    
    // Create backup
    conn.execute(
        &format!("CREATE TEMPORARY TABLE \"{}\" AS SELECT * FROM \"{}\"", backup_table, table_name),
        [],
    )?;
    
    // Drop corrupted table
    conn.execute(&format!("DROP TABLE \"{}\"", table_name), [])?;
    
    // Recreate with proper schema
    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (
                column_index INTEGER PRIMARY KEY NOT NULL,
                column_name TEXT NOT NULL UNIQUE,
                display_name TEXT,
                data_type TEXT,
                validator_type TEXT,
                validator_config TEXT,
                ai_context TEXT,
                filter_expr TEXT,
                ai_enable_row_generation INTEGER DEFAULT 0,
                ai_include_in_send INTEGER DEFAULT 1,
                deleted INTEGER DEFAULT 0
            )",
            table_name
        ),
        [],
    )?;
    
    // Re-insert non-deleted columns with proper indices
    conn.execute(
        &format!(
            "INSERT INTO \"{}\" (
                column_index, column_name, display_name, data_type, validator_type, validator_config,
                ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send, deleted
            )
            SELECT 
                ROW_NUMBER() OVER (ORDER BY rowid) - 1 AS column_index,
                column_name, display_name, data_type, validator_type, validator_config,
                ai_context, filter_expr, 
                COALESCE(ai_enable_row_generation, 0),
                COALESCE(ai_include_in_send, 1),
                COALESCE(deleted, 0)
            FROM \"{}\"
            WHERE deleted IS NULL OR deleted = 0
            ORDER BY rowid",
            table_name, backup_table
        ),
        [],
    )?;
    
    // Re-insert deleted columns at the end
    let deleted_count: i32 = conn.query_row(
        &format!("SELECT COUNT(*) FROM \"{}\" WHERE deleted = 1", backup_table),
        [],
        |row| row.get(0),
    )?;
    
    if deleted_count > 0 {
        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (
                    column_index, column_name, display_name, data_type, validator_type, validator_config,
                    ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send, deleted
                )
                SELECT 
                    (SELECT COALESCE(MAX(column_index), -1) FROM \"{}\") + ROW_NUMBER() OVER (ORDER BY rowid) AS column_index,
                    column_name, display_name, data_type, validator_type, validator_config,
                    ai_context, filter_expr,
                    COALESCE(ai_enable_row_generation, 0),
                    COALESCE(ai_include_in_send, 1),
                    1 AS deleted
                FROM \"{}\"
                WHERE deleted = 1
                ORDER BY rowid",
                table_name, table_name, backup_table
            ),
            [],
        )?;
    }
    
    
    // Commit transaction
    conn.execute("COMMIT", [])?;
    
    // Re-enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    
    Ok(())
}

