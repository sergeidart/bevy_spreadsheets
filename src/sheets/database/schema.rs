// src/sheets/database/schema.rs

use rusqlite::{Connection, params};
use crate::sheets::definitions::{ColumnDataType, ColumnDefinition, ColumnValidator, SheetMetadata};
use super::error::{DbResult, DbError};

/// SQL type mapping for column data types
pub fn sql_type_for_column(data_type: ColumnDataType) -> &'static str {
    match data_type {
        ColumnDataType::String => "TEXT",
        ColumnDataType::Bool => "INTEGER",
        ColumnDataType::I64 => "INTEGER",
        ColumnDataType::F64 => "REAL",
    }
}

/// Create the global _Metadata table if it doesn't exist
pub fn ensure_global_metadata_table(conn: &Connection) -> DbResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _Metadata (
            table_name TEXT PRIMARY KEY,
            table_type TEXT DEFAULT 'main',  -- 'main' or 'structure'
            parent_table TEXT,               -- For structure tables, name of parent
            parent_column TEXT,              -- For structure tables, parent column name
            ai_allow_add_rows INTEGER DEFAULT 0,
            ai_table_context TEXT,
            ai_grounding_with_google_search INTEGER DEFAULT 0,
            ai_active_group TEXT,
            display_order INTEGER,
            category TEXT,                   -- Category becomes DB file name
            hidden INTEGER DEFAULT 0,        -- Persisted hidden flag per table (0/1)
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    // Migration for existing databases: ensure 'hidden' column exists
    let mut has_hidden = false;
    let mut has_grounding = false;
    let mut stmt = conn.prepare("PRAGMA table_info('_Metadata')")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for col in rows {
        if let Ok(name) = col {
            if name.eq_ignore_ascii_case("hidden") { has_hidden = true; }
            if name.eq_ignore_ascii_case("ai_grounding_with_google_search") { has_grounding = true; }
        }
    }
    if !has_hidden {
        conn.execute("ALTER TABLE _Metadata ADD COLUMN hidden INTEGER DEFAULT 0", [])?;
        // Reasonable default: hide structure tables by default
        let _ = conn.execute(
            "UPDATE _Metadata SET hidden = 1 WHERE table_type = 'structure'",
            [],
        );
    }
    if !has_grounding {
        conn.execute(
            "ALTER TABLE _Metadata ADD COLUMN ai_grounding_with_google_search INTEGER DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}

/// Create main data table from metadata
pub fn create_data_table(
    conn: &Connection,
    table_name: &str,
    columns: &[ColumnDefinition],
) -> DbResult<()> {
    let mut col_defs = vec![
        "id INTEGER PRIMARY KEY AUTOINCREMENT".to_string(),
        "row_index INTEGER NOT NULL UNIQUE".to_string(),
    ];
    
    for col in columns {
        // Skip structure columns - they get their own tables
        if matches!(col.validator, Some(ColumnValidator::Structure)) {
            continue;
        }
        
        let sql_type = sql_type_for_column(col.data_type);
        col_defs.push(format!("\"{}\" {}", col.header, sql_type));
    }
    
    col_defs.push("created_at TEXT DEFAULT CURRENT_TIMESTAMP".to_string());
    col_defs.push("updated_at TEXT DEFAULT CURRENT_TIMESTAMP".to_string());
    
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" ({})",
        table_name,
        col_defs.join(", ")
    );
    
    conn.execute(&create_sql, [])?;
    
    // Create index
    conn.execute(
        &format!("CREATE INDEX IF NOT EXISTS idx_{}_row_index ON \"{}\"(row_index)", 
                 table_name, table_name),
        [],
    )?;
    
    Ok(())
}

/// Create metadata table for a sheet
pub fn create_metadata_table(
    conn: &Connection,
    table_name: &str,
    metadata: &SheetMetadata,
) -> DbResult<()> {
    let meta_table = format!("{}_Metadata", table_name);
    
    conn.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                column_index INTEGER UNIQUE NOT NULL,
                column_name TEXT NOT NULL UNIQUE,
                data_type TEXT NOT NULL,
                validator_type TEXT,
                validator_config TEXT,
                ai_context TEXT,
                filter_expr TEXT,  -- JSON array of filter expressions (up to 12 OR filters)
                ai_enable_row_generation INTEGER DEFAULT 0,
                ai_include_in_send INTEGER DEFAULT 1
            )",
            meta_table
        ),
        [],
    )?;
    
    // Insert column metadata
    for (idx, col) in metadata.columns.iter().enumerate() {
        let validator_type = match &col.validator {
            Some(ColumnValidator::Basic(_)) => Some("Basic"),
            Some(ColumnValidator::Linked { .. }) => Some("Linked"),
            Some(ColumnValidator::Structure) => Some("Structure"),
            None => None,
        };
        
        let validator_config = match &col.validator {
            Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                Some(serde_json::json!({
                    "target_table": target_sheet_name,
                    "target_column_index": target_column_index
                }).to_string())
            }
            Some(ColumnValidator::Structure) => {
                let structure_table = format!("{}_{}", table_name, col.header);
                Some(serde_json::json!({
                    "structure_table": structure_table
                }).to_string())
            }
            _ => None,
        };
        
        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO \"{}\" 
                 (column_index, column_name, data_type, validator_type, validator_config, ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                meta_table
            ),
            params![
                idx as i32,
                col.header,
                format!("{:?}", col.data_type),
                validator_type,
                validator_config,
                col.ai_context,
                col.filter,
                col.ai_enable_row_generation.unwrap_or(false) as i32,
                col.ai_include_in_send.unwrap_or(true) as i32
            ],
        )?;
    }
    
    Ok(())
}

/// Create AI groups table
pub fn create_ai_groups_table(
    conn: &Connection,
    table_name: &str,
    metadata: &SheetMetadata,
) -> DbResult<()> {
    let groups_table = format!("{}_Metadata_Groups", table_name);
    let meta_table = format!("{}_Metadata", table_name);
    
    conn.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                column_id INTEGER NOT NULL,
                group_name TEXT NOT NULL,
                is_enabled INTEGER DEFAULT 0,
                FOREIGN KEY (column_id) REFERENCES \"{}\"(id) ON DELETE CASCADE,
                UNIQUE(column_id, group_name)
            )",
            groups_table, meta_table
        ),
        [],
    )?;
    
    // Populate from metadata
    for group in &metadata.ai_schema_groups {
        for &col_idx in &group.included_columns {
            conn.execute(
                &format!(
                    "INSERT OR IGNORE INTO \"{}\" (column_id, group_name, is_enabled)
                     SELECT id, ?, 1 FROM \"{}\" WHERE column_index = ?",
                    groups_table, meta_table
                ),
                params![&group.name, col_idx as i32],
            )?;
        }
    }
    
    Ok(())
}

/// Create structure table for nested data
pub fn create_structure_table(
    conn: &Connection,
    parent_table: &str,
    col_def: &ColumnDefinition,
) -> DbResult<()> {
    let structure_table = format!("{}_{}", parent_table, col_def.header);
    
    let schema = col_def.structure_schema.as_ref()
        .ok_or_else(|| DbError::InvalidMetadata("Structure column missing schema".into()))?;
    
    let mut col_defs = vec![
        "id INTEGER PRIMARY KEY AUTOINCREMENT".to_string(),
        format!("parent_id INTEGER NOT NULL REFERENCES \"{}\"(id) ON DELETE CASCADE", parent_table),
        "row_index INTEGER NOT NULL".to_string(),
        // Use parent_key column name to match UI/JSON structure sheets
        "parent_key TEXT NOT NULL".to_string(),
    ];
    
    for field in schema {
        let sql_type = sql_type_for_column(field.data_type);
        col_defs.push(format!("\"{}\" {}", field.header, sql_type));
    }
    
    col_defs.push("created_at TEXT DEFAULT CURRENT_TIMESTAMP".to_string());
    col_defs.push("updated_at TEXT DEFAULT CURRENT_TIMESTAMP".to_string());
    col_defs.push("UNIQUE(parent_id, row_index)".to_string());
    
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" ({})",
        structure_table,
        col_defs.join(", ")
    );
    
    conn.execute(&create_sql, [])?;
    
    // Indexes
    conn.execute(
        &format!("CREATE INDEX IF NOT EXISTS idx_{}_parent ON \"{}\"(parent_id)", 
                 structure_table, structure_table),
        [],
    )?;
    
    // Register structure table in _Metadata
    conn.execute(
        "INSERT OR REPLACE INTO _Metadata (table_name, table_type, parent_table, parent_column, hidden)
         VALUES (?, 'structure', ?, ?, 1)",
        params![&structure_table, parent_table, &col_def.header],
    )?;
    
    Ok(())
}

/// Insert table-level metadata
pub fn insert_table_metadata(
    conn: &Connection,
    table_name: &str,
    metadata: &SheetMetadata,
    display_order: Option<i32>,
) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO _Metadata 
         (table_name, table_type, ai_allow_add_rows, ai_table_context, ai_active_group, category, display_order, hidden)
         VALUES (?, 'main', ?, ?, ?, ?, ?, ?)",
        params![
            table_name,
            metadata.ai_enable_row_generation as i32,
            metadata.ai_general_rule,
            metadata.ai_active_schema_group,
            metadata.category,
            display_order,
            metadata.hidden as i32
        ],
    )?;
    Ok(())
}
