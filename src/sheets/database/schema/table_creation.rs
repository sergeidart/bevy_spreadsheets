// src/sheets/database/schema/table_creation.rs

use bevy::prelude::*;
use rusqlite::Connection;

use super::super::error::{DbError, DbResult};
use super::super::daemon_client::DaemonClient;
use super::helpers::sql_type_for_column;
use super::migrations::{ensure_migration_tracking, is_migration_applied, mark_migration_applied};
use super::queries;
use super::writer;
use crate::sheets::definitions::{ColumnDefinition, ColumnValidator, SheetMetadata};

/// Create the global _Metadata table if it doesn't exist
pub fn ensure_global_metadata_table(conn: &Connection, daemon_client: &DaemonClient) -> DbResult<()> {
    // First ensure migration tracking exists
    ensure_migration_tracking(conn, daemon_client)?;

    // Create the table
    writer::create_global_metadata_table(daemon_client)?;

    // Run migrations only if not already applied
    if !is_migration_applied(conn, 1)? {
        add_metadata_columns_migration(conn, daemon_client)?;
        mark_migration_applied(conn, 1, "Added hidden and grounding columns to _Metadata", daemon_client)?;
    }

    if !is_migration_applied(conn, 2)? {
        add_ai_model_id_column_migration(conn, daemon_client)?;
        mark_migration_applied(conn, 2, "Added ai_model_id column to _Metadata", daemon_client)?;
    }

    Ok(())
}

/// Migration 1: Add hidden and grounding columns
fn add_metadata_columns_migration(conn: &Connection, daemon_client: &DaemonClient) -> DbResult<()> {
    let existing_cols = queries::get_table_columns(conn, "_Metadata")?;

    if !existing_cols.iter().any(|c| c.eq_ignore_ascii_case("hidden")) {
        writer::add_column_if_missing(conn, "_Metadata", "hidden", "INTEGER DEFAULT 0", daemon_client, None)?;
        // Hide structure tables by default
        writer::update_table_metadata_hidden("table_type = 'structure'", daemon_client)?;
    }

    if !existing_cols
        .iter()
        .any(|c| c.eq_ignore_ascii_case("ai_grounding_with_google_search"))
    {
        writer::add_column_if_missing(
            conn,
            "_Metadata",
            "ai_grounding_with_google_search",
            "INTEGER DEFAULT 0",
            daemon_client,
            None,
        )?;
    }

    Ok(())
}

/// Migration 2: Add ai_model_id column
fn add_ai_model_id_column_migration(conn: &Connection, daemon_client: &DaemonClient) -> DbResult<()> {
    let existing_cols = queries::get_table_columns(conn, "_Metadata")?;

    if !existing_cols.iter().any(|c| c.eq_ignore_ascii_case("ai_model_id")) {
        writer::add_column_if_missing(conn, "_Metadata", "ai_model_id", "TEXT", daemon_client, None)?;
        info!("Added ai_model_id column to _Metadata table");
    }

    Ok(())
}

/// Create main data table from metadata
pub fn create_data_table(
    table_name: &str,
    columns: &[ColumnDefinition],
    daemon_client: &DaemonClient,
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

    writer::create_main_data_table(table_name, &col_defs, daemon_client)?;
    Ok(())
}

/// Create metadata table for a sheet
pub fn create_metadata_table(
    table_name: &str,
    metadata: &SheetMetadata,
    daemon_client: &DaemonClient,
    db_name: Option<&str>,
) -> DbResult<()> {
    let meta_table = format!("{}_Metadata", table_name);

    // Create the table structure
    writer::create_sheet_metadata_table(&meta_table, daemon_client, db_name)?;

    // Insert column metadata
    for (idx, col) in metadata.columns.iter().enumerate() {
        let (validator_type, validator_config) = build_validator_info(table_name, col)?;

        writer::insert_column_metadata(
            &meta_table,
            idx as i32,
            &col.header,
            &format!("{:?}", col.data_type),
            validator_type.as_deref(),
            validator_config.as_deref(),
            col.ai_context.as_deref(),
            col.filter.as_deref(),
            col.ai_enable_row_generation.unwrap_or(false) as i32,
            col.ai_include_in_send.unwrap_or(true) as i32,
            col.deleted as i32,
            daemon_client,
            db_name,
        )?;
    }

    Ok(())
}

/// Build validator type and config JSON for a column
fn build_validator_info(
    table_name: &str,
    col: &ColumnDefinition,
) -> DbResult<(Option<String>, Option<String>)> {
    let validator_type = match &col.validator {
        Some(ColumnValidator::Basic(_)) => Some("Basic".to_string()),
        Some(ColumnValidator::Linked { .. }) => Some("Linked".to_string()),
        Some(ColumnValidator::Structure) => Some("Structure".to_string()),
        None => None,
    };

    let validator_config = match &col.validator {
        Some(ColumnValidator::Linked {
            target_sheet_name,
            target_column_index,
        }) => Some(
            serde_json::json!({
                "target_table": target_sheet_name,
                "target_column_index": target_column_index
            })
            .to_string(),
        ),
        Some(ColumnValidator::Structure) => {
            let structure_table = format!("{}_{}", table_name, col.header);
            Some(
                serde_json::json!({
                    "structure_table": structure_table
                })
                .to_string(),
            )
        }
        _ => None,
    };

    Ok((validator_type, validator_config))
}

/// Ensure the per-table metadata table exists and contains the expected columns/rows.
/// This is a best-effort migration helper for older or foreign databases.
pub fn ensure_table_metadata_schema(
    conn: &Connection,
    table_name: &str,
    metadata: &SheetMetadata,
    daemon_client: &DaemonClient,
    db_name: Option<&str>,
) -> DbResult<()> {
    let meta_table = format!("{}_Metadata", table_name);

    // Create table if missing
    if !queries::table_exists(conn, &meta_table)? {
        create_metadata_table(table_name, metadata, daemon_client, db_name)?;
        return Ok(());
    }

    // Ensure required columns exist
    add_missing_metadata_columns(conn, &meta_table, daemon_client, db_name)?;

    // Ensure one row for each column index exists
    populate_missing_column_rows(conn, table_name, &meta_table, metadata, daemon_client)?;

    Ok(())
}

/// Add missing columns to metadata table
fn add_missing_metadata_columns(conn: &Connection, meta_table: &str, daemon_client: &DaemonClient, db_name: Option<&str>) -> DbResult<()> {
    writer::add_column_if_missing(conn, meta_table, "display_name", "TEXT", daemon_client, db_name)?;
    writer::add_column_if_missing(conn, meta_table, "validator_type", "TEXT", daemon_client, db_name)?;
    writer::add_column_if_missing(conn, meta_table, "validator_config", "TEXT", daemon_client, db_name)?;
    writer::add_column_if_missing(conn, meta_table, "ai_context", "TEXT", daemon_client, db_name)?;
    writer::add_column_if_missing(conn, meta_table, "filter_expr", "TEXT", daemon_client, db_name)?;
    writer::add_column_if_missing(
        conn,
        meta_table,
        "ai_enable_row_generation",
        "INTEGER DEFAULT 0",
        daemon_client,
        db_name,
    )?;
    writer::add_column_if_missing(conn, meta_table, "ai_include_in_send", "INTEGER DEFAULT 1", daemon_client, db_name)?;
    writer::add_column_if_missing(conn, meta_table, "deleted", "INTEGER DEFAULT 0", daemon_client, db_name)?;
    Ok(())
}

/// Populate missing column rows in metadata table
fn populate_missing_column_rows(
    conn: &Connection,
    table_name: &str,
    meta_table: &str,
    metadata: &SheetMetadata,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let table_type = queries::get_table_type(conn, table_name)?;
    let is_structure = matches!(table_type.as_deref(), Some("structure"));

    let tx = conn.unchecked_transaction()?;

    let mut data_column_idx = 0i32;
    for col in metadata.columns.iter() {
        // Skip runtime-only technical columns for structure tables
        if is_structure
            && (col.header == "row_index" || col.header == "parent_key" || col.header == "id")
        {
            bevy::log::debug!(
                "ensure_table_metadata_schema: skipping runtime-only column '{}' for structure table '{}'",
                col.header,
                table_name
            );
            continue;
        }

        let dt = format!("{:?}", col.data_type);
        writer::insert_column_metadata_if_missing(
            meta_table,
            data_column_idx,
            &col.header,
            &dt,
            daemon_client,
        )?;
        data_column_idx += 1;
    }

    tx.commit()?;
    Ok(())
}

/// Create AI groups table
pub fn create_ai_groups_table(
    conn: &Connection,
    table_name: &str,
    metadata: &SheetMetadata,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let groups_table = format!("{}_Metadata_Groups", table_name);
    let meta_table = format!("{}_Metadata", table_name);

    // Create the groups table
    writer::create_ai_groups_table(&groups_table, &meta_table, daemon_client)?;

    // Ensure deleted column exists in metadata table
    writer::add_column_if_missing(conn, &meta_table, "deleted", "INTEGER DEFAULT 0", daemon_client, None)?;

    // Populate from metadata
    for group in &metadata.ai_schema_groups {
        for &col_idx in &group.included_columns {
            writer::insert_ai_group_column(
                &groups_table,
                &meta_table,
                &group.name,
                col_idx as i32,
                daemon_client,
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
    structure_columns: Option<&[ColumnDefinition]>,
    daemon_client: &DaemonClient,
    db_name: Option<&str>,
) -> DbResult<()> {
    let structure_table = format!("{}_{}", parent_table, col_def.header);

    info!("======================================");
    info!("CREATE_STRUCTURE_TABLE: {}", structure_table);

    // Build column definitions
    let col_defs = build_structure_column_definitions(col_def, structure_columns)?;

    // Check if table exists and needs recreation
    if should_recreate_structure_table(conn, &structure_table, &col_defs, structure_columns, col_def)? {
        info!("Schema mismatch detected, dropping and recreating table '{}'", structure_table);
        writer::drop_table(&structure_table, daemon_client, db_name)?;
    }

    // Create the table
    writer::create_structure_data_table(&structure_table, &col_defs, daemon_client, db_name)?;

    // Register in global metadata
    writer::register_structure_table(&structure_table, parent_table, &col_def.header, daemon_client, db_name)?;

    // CRITICAL: Checkpoint WAL to ensure daemon writes are persisted to disk
    // Without this, the _Metadata entry may not be visible after app restart
    info!("Checkpointing WAL to persist structure table registration");
    let _ = conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(()))
        .map_err(|e| warn!("Failed to checkpoint WAL after structure table creation: {}", e));

    info!("======================================");
    Ok(())
}

/// Build column definitions for structure table
fn build_structure_column_definitions(
    col_def: &ColumnDefinition,
    structure_columns: Option<&[ColumnDefinition]>,
) -> DbResult<Vec<String>> {
    let mut col_defs = vec![
        "id INTEGER PRIMARY KEY AUTOINCREMENT".to_string(),
        "row_index INTEGER NOT NULL".to_string(),
    ];

    let mut has_parent_key = false;

    if let Some(struct_cols) = structure_columns {
        // Using structure_columns - includes ALL columns (technical + data)
        info!(
            "Using provided structure_columns with {} columns:",
            struct_cols.len()
        );
        for (i, col) in struct_cols.iter().enumerate() {
            info!(
                "  [{}] header='{}', data_type={:?}",
                i, col.header, col.data_type
            );

            if col.header.eq_ignore_ascii_case("row_index") {
                continue; // Already added
            } else if col.header.eq_ignore_ascii_case("parent_key") {
                col_defs.push("parent_key TEXT NOT NULL".to_string());
                has_parent_key = true;
            } else {
                let sql_type = sql_type_for_column(col.data_type);
                col_defs.push(format!("\"{}\" {}", col.header, sql_type));
            }
        }
    } else {
        // Using structure_schema - data columns only (backward compatibility)
        let schema = col_def
            .structure_schema
            .as_ref()
            .ok_or_else(|| DbError::InvalidMetadata("Structure column missing schema".into()))?;
        info!("Using structure_schema with {} fields:", schema.len());
        for (i, field) in schema.iter().enumerate() {
            info!(
                "  [{}] header='{}', data_type={:?}, validator={:?}",
                i, field.header, field.data_type, field.validator
            );

            if field.header.eq_ignore_ascii_case("row_index") {
                continue;
            } else if field.header.eq_ignore_ascii_case("parent_key") {
                col_defs.push("parent_key TEXT NOT NULL".to_string());
                has_parent_key = true;
            } else {
                let sql_type = sql_type_for_column(field.data_type);
                col_defs.push(format!("\"{}\" {}", field.header, sql_type));
            }
        }
    }

    // Ensure parent_key exists (backward compatibility)
    if !has_parent_key {
        col_defs.push("parent_key TEXT NOT NULL".to_string());
    }

    col_defs.push("UNIQUE(parent_key, row_index)".to_string());

    Ok(col_defs)
}

/// Check if structure table needs to be recreated due to schema mismatch
fn should_recreate_structure_table(
    conn: &Connection,
    structure_table: &str,
    col_defs: &[String],
    structure_columns: Option<&[ColumnDefinition]>,
    col_def: &ColumnDefinition,
) -> DbResult<bool> {
    if !queries::table_exists(conn, structure_table)? {
        return Ok(false); // Table doesn't exist, no need to recreate
    }

    // Extract expected column names from col_defs (e.g., "id INTEGER PRIMARY KEY" -> "id")
    let expected_columns: Vec<String> = col_defs
        .iter()
        .filter_map(|def| {
            // Skip UNIQUE constraints and other table-level constraints
            if def.starts_with("UNIQUE") || def.starts_with("FOREIGN KEY") {
                return None;
            }
            // Extract column name (first token, removing quotes)
            def.split_whitespace()
                .next()
                .map(|s| s.trim_matches('"').to_string())
        })
        .collect();

    let source_count = if let Some(struct_cols) = structure_columns {
        struct_cols.len()
    } else {
        col_def
            .structure_schema
            .as_ref()
            .map(|s| s.len())
            .unwrap_or(0)
    };

    info!(
        "Table '{}' exists, verifying structure with {} expected columns (from source with {} fields)",
        structure_table, expected_columns.len(), source_count
    );

    // Use verify_table_structure to check both column count and names
    match queries::verify_table_structure(conn, structure_table, &expected_columns) {
        Ok(_) => {
            info!("Structure table '{}' schema matches expectations", structure_table);
            Ok(false) // No need to recreate
        }
        Err(e) => {
            warn!(
                "Schema mismatch detected for '{}': {}",
                structure_table, e
            );
            Ok(true) // Needs recreation
        }
    }
}

/// Insert table-level metadata
pub fn insert_table_metadata(
    table_name: &str,
    metadata: &SheetMetadata,
    display_order: Option<i32>,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    writer::upsert_table_metadata(
        table_name,
        metadata.ai_enable_row_generation as i32,
        metadata.ai_general_rule.as_deref(),
        metadata.ai_active_schema_group.as_deref(),
        metadata.category.as_deref(),
        display_order,
        metadata.hidden as i32,
        daemon_client,
    )?;
    Ok(())
}
