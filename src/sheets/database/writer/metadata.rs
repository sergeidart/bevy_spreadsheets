// src/sheets/database/writer/metadata.rs
// Metadata operations - managing table and column metadata, AI settings

use super::super::error::DbResult;
use super::super::schema::sql_type_for_column;
use crate::sheets::definitions::{ColumnDataType, ColumnValidator};
use rusqlite::{params, Connection, OptionalExtension};

/// Convert runtime column index (which includes technical columns) to persisted column index
/// (which excludes technical columns that are added at runtime).
/// 
/// For regular tables: technical column is row_index at index 0
/// For structure tables: technical columns are row_index (0) and parent_key (1)
/// 
/// Returns the persisted index, or None if the column_index refers to a technical column
fn runtime_to_persisted_column_index(
    conn: &Connection,
    table_name: &str,
    runtime_column_index: usize,
) -> DbResult<Option<usize>> {
    // Determine if this is a structure table
    let table_type: Option<String> = conn
        .query_row(
            "SELECT table_type FROM _Metadata WHERE table_name = ?",
            [table_name],
            |row| row.get(0),
        )
        .optional()?;
    
    let is_structure = matches!(table_type.as_deref(), Some("structure"));
    
    if is_structure {
        // Structure tables have row_index (0) and parent_key (1) as technical columns
        if runtime_column_index < 2 {
            // This is a technical column, should not be persisted
            bevy::log::warn!(
                "Attempted to persist metadata for technical column {} in structure table '{}'",
                runtime_column_index,
                table_name
            );
            return Ok(None);
        }
        // Subtract 2 to get persisted index
        Ok(Some(runtime_column_index - 2))
    } else {
        // Regular tables have row_index (0) as technical column
        if runtime_column_index == 0 {
            // This is a technical column, should not be persisted
            bevy::log::warn!(
                "Attempted to persist metadata for technical column 0 (row_index) in regular table '{}'",
                table_name
            );
            return Ok(None);
        }
        // Subtract 1 to get persisted index
        Ok(Some(runtime_column_index - 1))
    }
}

/// Update a table's hidden flag in the global _Metadata table
pub fn update_table_hidden(conn: &Connection, table_name: &str, hidden: bool) -> DbResult<()> {
    conn.execute(
        "INSERT INTO _Metadata (table_name, hidden) VALUES (?, ?) \
         ON CONFLICT(table_name) DO UPDATE SET hidden = excluded.hidden, updated_at = CURRENT_TIMESTAMP",
        params![table_name, hidden as i32],
    )?;
    Ok(())
}

/// Update table-level flags in _Metadata
pub fn update_table_ai_settings(
    conn: &Connection,
    table_name: &str,
    allow_add_rows: Option<bool>,
    table_context: Option<&str>,
    active_group: Option<&str>,
    grounding_with_google_search: Option<bool>,
) -> DbResult<()> {
    // Build dynamic SQL to only update provided fields
    let mut sets: Vec<&str> = Vec::new();
    if allow_add_rows.is_some() {
        sets.push("ai_allow_add_rows = ?");
    }
    if table_context.is_some() {
        sets.push("ai_table_context = ?");
    }
    if active_group.is_some() {
        sets.push("ai_active_group = ?");
    }
    if grounding_with_google_search.is_some() {
        sets.push("ai_grounding_with_google_search = ?");
    }
    if sets.is_empty() {
        return Ok(());
    }
    let sql = format!(
        "UPDATE _Metadata SET {} , updated_at = CURRENT_TIMESTAMP WHERE table_name = ?",
        sets.join(", ")
    );
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(v) = allow_add_rows {
        params_vec.push(Box::new(v as i32));
    }
    if let Some(v) = table_context {
        params_vec.push(Box::new(v.to_string()));
    }
    if let Some(v) = active_group {
        params_vec.push(Box::new(v.to_string()));
    }
    if let Some(v) = grounding_with_google_search {
        params_vec.push(Box::new(v as i32));
    }
    params_vec.push(Box::new(table_name.to_string()));
    conn.execute(&sql, rusqlite::params_from_iter(params_vec.iter()))?;
    Ok(())
}

/// Update a column's filter, ai_context, and include flag in the table's metadata table
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn update_column_metadata(
    conn: &Connection,
    table_name: &str,
    column_index: usize,
    filter_expr: Option<&str>,
    ai_context: Option<&str>,
    ai_include_in_send: Option<bool>,
) -> DbResult<()> {
    // Convert runtime column index to persisted index
    let persisted_index = match runtime_to_persisted_column_index(conn, table_name, column_index)? {
        Some(idx) => idx,
        None => {
            // This is a technical column, skip the update
            bevy::log::debug!(
                "Skipping metadata update for technical column {} in table '{}'",
                column_index,
                table_name
            );
            return Ok(());
        }
    };
    
    // Defensive: ensure per-table metadata table structure and rows exist
    // We don't know the full metadata here; construct a minimal synthetic one from DB if needed.
    // Call global _Metadata ensure first (no-op if exists)
    let _ = crate::sheets::database::schema::ensure_global_metadata_table(conn);
    // Try to read current metadata; if that fails, synthesize minimal using existing table columns
    let inferred_meta =
        match crate::sheets::database::reader::DbReader::read_metadata(conn, table_name) {
            Ok(m) => m,
            Err(_) => {
                // If we cannot read, build a placeholder with a single String column for safety
                crate::sheets::definitions::SheetMetadata::create_generic(
                    table_name.to_string(),
                    format!("{}.json", table_name),
                    (column_index + 1).max(1),
                    None,
                )
            }
        };
    let _ = crate::sheets::database::schema::ensure_table_metadata_schema(
        conn,
        table_name,
        &inferred_meta,
    );
    let meta_table = format!("{}_Metadata", table_name);
    let mut sets: Vec<&str> = Vec::new();
    if filter_expr.is_some() {
        sets.push("filter_expr = ?");
    }
    if ai_context.is_some() {
        sets.push("ai_context = ?");
    }
    if ai_include_in_send.is_some() {
        sets.push("ai_include_in_send = ?");
    }
    if sets.is_empty() {
        return Ok(());
    }
    let sql = format!(
        "UPDATE \"{}\" SET {} WHERE column_index = ?",
        meta_table,
        sets.join(", ")
    );
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    // For textual fields, treat an explicitly provided empty string as a request to clear (set to NULL)
    if let Some(v) = filter_expr {
        if v.trim().is_empty() {
            params_vec.push(Box::new(rusqlite::types::Null));
        } else {
            params_vec.push(Box::new(v.to_string()));
        }
    }
    if let Some(v) = ai_context {
        if v.trim().is_empty() {
            params_vec.push(Box::new(rusqlite::types::Null));
        } else {
            params_vec.push(Box::new(v.to_string()));
        }
    }
    if let Some(v) = ai_include_in_send {
        params_vec.push(Box::new(v as i32));
    }
    params_vec.push(Box::new(persisted_index as i32));
    // Log SQL and high-level params for debugging visibility
    bevy::log::info!(
        "SQL update_column_metadata: {} ; params_count={} ; runtime_idx={} -> persisted_idx={}",
        sql, params_vec.len(), column_index, persisted_index
    );
    conn.execute(&sql, rusqlite::params_from_iter(params_vec.iter()))?;
    Ok(())
}

/// Explicitly set the AI include flag for a column in the metadata table (true = 1, false = 0)
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn update_column_ai_include(
    conn: &Connection,
    table_name: &str,
    column_index: usize,
    include: bool,
) -> DbResult<()> {
    // Convert runtime column index to persisted index
    let persisted_index = match runtime_to_persisted_column_index(conn, table_name, column_index)? {
        Some(idx) => idx,
        None => {
            // This is a technical column, skip the update
            bevy::log::debug!(
                "Skipping AI include update for technical column {} in table '{}'",
                column_index,
                table_name
            );
            return Ok(());
        }
    };
    
    let meta_table = format!("{}_Metadata", table_name);
    bevy::log::info!(
        "SQL update_column_ai_include: table='{}' runtime_idx={} -> persisted_idx={} include={}",
        table_name, column_index, persisted_index, include
    );
    conn.execute(
        &format!(
            "UPDATE \"{}\" SET ai_include_in_send = ? WHERE column_index = ?",
            meta_table
        ),
        params![include as i32, persisted_index as i32],
    )?;
    Ok(())
}

/// Update a column's validator (data_type, validator_type, validator_config) and optional AI flags in metadata
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn update_column_validator(
    conn: &Connection,
    table_name: &str,
    column_index: usize,
    data_type: ColumnDataType,
    validator: &Option<ColumnValidator>,
    ai_include_in_send: Option<bool>,
    ai_enable_row_generation: Option<bool>,
) -> DbResult<()> {
    // Convert runtime column index to persisted index
    let persisted_index = match runtime_to_persisted_column_index(conn, table_name, column_index)? {
        Some(idx) => idx,
        None => {
            // This is a technical column, skip the update
            bevy::log::debug!(
                "Skipping validator update for technical column {} in table '{}'",
                column_index,
                table_name
            );
            return Ok(());
        }
    };
    
    // Defensive: ensure per-table metadata table structure and rows exist
    let _ = crate::sheets::database::schema::ensure_global_metadata_table(conn);
    let inferred_meta =
        match crate::sheets::database::reader::DbReader::read_metadata(conn, table_name) {
            Ok(m) => m,
            Err(_) => crate::sheets::definitions::SheetMetadata::create_generic(
                table_name.to_string(),
                format!("{}.json", table_name),
                (column_index + 1).max(1),
                None,
            ),
        };
    let _ = crate::sheets::database::schema::ensure_table_metadata_schema(
        conn,
        table_name,
        &inferred_meta,
    );
    let meta_table = format!("{}_Metadata", table_name);
    let (validator_type, validator_config): (Option<String>, Option<String>) = match validator {
        Some(ColumnValidator::Basic(_)) => (Some("Basic".to_string()), None),
        Some(ColumnValidator::Linked {
            target_sheet_name,
            target_column_index,
        }) => {
            let cfg = serde_json::json!({
                "target_table": target_sheet_name,
                "target_column_index": target_column_index
            })
            .to_string();
            (Some("Linked".to_string()), Some(cfg))
        }
        Some(ColumnValidator::Structure) => {
            // Persist structure reference for completeness
            let cfg = serde_json::json!({
                "structure_table": format!("{}_{}", table_name, "")
            })
            .to_string();
            (Some("Structure".to_string()), Some(cfg))
        }
        None => (None, None),
    };

    // Build dynamic SQL to include optional AI flags only when provided
    // Note: Metadata tables don't have an updated_at column; only main data tables do.
    // Keep the payload updates minimal and valid for the metadata schema.
    let mut sets = vec![
        "data_type = ?",
        "validator_type = ?",
        "validator_config = ?",
    ];
    if ai_include_in_send.is_some() {
        sets.push("ai_include_in_send = ?");
    }
    if ai_enable_row_generation.is_some() {
        sets.push("ai_enable_row_generation = ?");
    }
    let sql = format!(
        "UPDATE \"{}\" SET {} WHERE column_index = ?",
        meta_table,
        sets.join(", ")
    );

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    params_vec.push(Box::new(format!("{:?}", data_type)));
    params_vec.push(Box::new(validator_type.clone()));
    params_vec.push(Box::new(validator_config.clone()));
    if let Some(v) = ai_include_in_send {
        params_vec.push(Box::new(v as i32));
    }
    if let Some(v) = ai_enable_row_generation {
        params_vec.push(Box::new(v as i32));
    }
    params_vec.push(Box::new(persisted_index as i32));

    // Log SQL and parameter summary before executing (show param values derived from known locals)
    let mut param_preview: Vec<String> = Vec::new();
    param_preview.push(format!("data_type={:?}", data_type));
    param_preview.push(format!("validator_type={:?}", validator_type.clone()));
    param_preview.push(format!("validator_config={:?}", validator_config.clone()));
    if let Some(v) = ai_include_in_send {
        param_preview.push(format!("ai_include_in_send={}", v));
    }
    if let Some(v) = ai_enable_row_generation {
        param_preview.push(format!("ai_enable_row_generation={}", v));
    }
    param_preview.push(format!("runtime_idx={} -> persisted_idx={}", column_index, persisted_index));
    bevy::log::info!(
        "SQL update_column_validator: {} ; params_count={} ; params={:?}",
        sql,
        params_vec.len(),
        param_preview
    );

    conn.execute(&sql, rusqlite::params_from_iter(params_vec.iter()))?;
    Ok(())
}

/// Update a column's display name (UI-only) in the table's metadata table
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn update_column_display_name(
    conn: &Connection,
    table_name: &str,
    column_index: usize,
    display_name: &str,
) -> DbResult<()> {
    // Convert runtime column index to persisted index
    let persisted_index = match runtime_to_persisted_column_index(conn, table_name, column_index)? {
        Some(idx) => idx,
        None => {
            // Technical column, ignore
            return Ok(());
        }
    };

    let meta_table = format!("{}_Metadata", table_name);
    bevy::log::info!(
        "SQL update_column_display_name: table='{}' runtime_idx={} -> persisted_idx={} display_name='{}'",
        table_name, column_index, persisted_index, display_name
    );
    conn.execute(
        &format!(
            "UPDATE \"{}\" SET display_name = ? WHERE column_index = ?",
            meta_table
        ),
        params![display_name, persisted_index as i32],
    )?;
    Ok(())
}

/// Add a new column to a table (main or structure) and insert its metadata row with given index.
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn add_column_with_metadata(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    data_type: crate::sheets::definitions::ColumnDataType,
    validator: Option<crate::sheets::definitions::ColumnValidator>,
    column_index: usize,
    ai_context: Option<&str>,
    filter_expr: Option<&str>,
    ai_enable_row_generation: Option<bool>,
    ai_include_in_send: Option<bool>,
) -> DbResult<()> {
    // Convert runtime column index to persisted index
    let persisted_index = match runtime_to_persisted_column_index(conn, table_name, column_index)? {
        Some(idx) => idx,
        None => {
            // This is a technical column, skip the add
            bevy::log::debug!(
                "Skipping add for technical column {} in table '{}'",
                column_index,
                table_name
            );
            return Ok(());
        }
    };
    // Check if column exists physically; if not, add it
    let mut exists_stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    let mut col_exists = false;
    for row in exists_stmt.query_map([], |r| r.get::<_, String>(1))? {
        if row? == column_name {
            col_exists = true;
            break;
        }
    }
    if !col_exists {
        let sql_type = sql_type_for_column(data_type);
        conn.execute(
            &format!(
                "ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}",
                table_name, column_name, sql_type
            ),
            [],
        )?;
        bevy::log::info!("SQL add_column: ALTER TABLE '{}' ADD COLUMN '{}' {}", table_name, column_name, sql_type);
    } else {
        bevy::log::info!("SQL add_column: column '{}' already exists on '{}', skipping ALTER TABLE", column_name, table_name);
    }

    // Compute validator metadata for both reuse and insert
    let (validator_type, validator_config): (Option<String>, Option<String>) = match &validator {
        Some(ColumnValidator::Basic(_)) => (Some("Basic".to_string()), None),
        Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
            let cfg = serde_json::json!({
                "target_table": target_sheet_name,
                "target_column_index": target_column_index
            }).to_string();
            (Some("Linked".to_string()), Some(cfg))
        }
        Some(ColumnValidator::Structure) => (Some("Structure".to_string()), Some(serde_json::json!({"structure_table": format!("{}_{}", table_name, column_name)}).to_string())),
        None => (None, None),
    };
    // Try to reuse a deleted metadata slot before inserting
    let meta_table = format!("{}_Metadata", table_name);
    let reuse_sql = format!(
        "UPDATE \"{}\" SET column_name = ?, data_type = ?, validator_type = ?, validator_config = ?, ai_context = ?, filter_expr = ?, ai_enable_row_generation = ?, ai_include_in_send = ?, deleted = 0 WHERE column_index = ? AND deleted = 1",
        meta_table
    );
    bevy::log::info!(
        "SQL add_column_with_metadata: table='{}' runtime_idx={} -> persisted_idx={} col_name='{}'",
        table_name, column_index, persisted_index, column_name
    );
    let reused = conn.execute(&reuse_sql, params![
        column_name,
        format!("{:?}", data_type),
        validator_type.clone(),
        validator_config.clone(),
        ai_context,
        filter_expr,
        ai_enable_row_generation.unwrap_or(false) as i32,
        ai_include_in_send.unwrap_or(true) as i32,
        persisted_index as i32,
    ])?;
    if reused > 0 {
        bevy::log::info!(
            "Reused deleted metadata slot for persisted_index={} (runtime={}) in '{}'.",
            persisted_index,
            column_index,
            meta_table
        );
        return Ok(());
    }
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO \"{}\" (column_index, column_name, data_type, validator_type, validator_config, ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            meta_table
        ),
        params![
            persisted_index as i32,
            column_name,
            format!("{:?}", data_type),
            validator_type,
            validator_config,
            ai_context,
            filter_expr,
            ai_enable_row_generation.unwrap_or(false) as i32,
            ai_include_in_send.unwrap_or(true) as i32
        ],
    )?;
    bevy::log::info!("SQL add_column metadata: INSERT OR REPLACE INTO '{}' (column_index={}, column_name='{}')", meta_table, persisted_index, column_name);
    Ok(())
}
