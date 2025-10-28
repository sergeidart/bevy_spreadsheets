// src/sheets/database/schema/table_creation.rs

use bevy::prelude::*;
use rusqlite::Connection;

use super::super::error::{DbError, DbResult};
use super::helpers::sql_type_for_column;
use super::migrations::{ensure_migration_tracking, is_migration_applied, mark_migration_applied};
use super::queries;
use crate::sheets::definitions::{ColumnDefinition, ColumnValidator, SheetMetadata};

/// Create the global _Metadata table if it doesn't exist
pub fn ensure_global_metadata_table(conn: &Connection) -> DbResult<()> {
    // First ensure migration tracking exists
    ensure_migration_tracking(conn)?;

    // Create the table
    queries::create_global_metadata_table(conn)?;

    // Run migrations only if not already applied
    if !is_migration_applied(conn, 1)? {
        add_metadata_columns_migration(conn)?;
        mark_migration_applied(conn, 1, "Added hidden and grounding columns to _Metadata")?;
    }

    Ok(())
}

/// Migration 1: Add hidden and grounding columns
fn add_metadata_columns_migration(conn: &Connection) -> DbResult<()> {
    let existing_cols = queries::get_table_columns(conn, "_Metadata")?;

    if !existing_cols.iter().any(|c| c.eq_ignore_ascii_case("hidden")) {
        queries::add_column_if_missing(conn, "_Metadata", "hidden", "INTEGER DEFAULT 0")?;
        // Hide structure tables by default
        queries::update_table_metadata_hidden(conn, "table_type = 'structure'")?;
    }

    if !existing_cols
        .iter()
        .any(|c| c.eq_ignore_ascii_case("ai_grounding_with_google_search"))
    {
        queries::add_column_if_missing(
            conn,
            "_Metadata",
            "ai_grounding_with_google_search",
            "INTEGER DEFAULT 0",
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

    queries::create_main_data_table(conn, table_name, &col_defs)?;
    Ok(())
}

/// Create metadata table for a sheet
pub fn create_metadata_table(
    conn: &Connection,
    table_name: &str,
    metadata: &SheetMetadata,
) -> DbResult<()> {
    let meta_table = format!("{}_Metadata", table_name);

    // Create the table structure
    queries::create_sheet_metadata_table(conn, &meta_table)?;

    // Insert column metadata
    for (idx, col) in metadata.columns.iter().enumerate() {
        let (validator_type, validator_config) = build_validator_info(table_name, col)?;

        queries::insert_column_metadata(
            conn,
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
) -> DbResult<()> {
    let meta_table = format!("{}_Metadata", table_name);

    // Create table if missing
    if !queries::table_exists(conn, &meta_table)? {
        create_metadata_table(conn, table_name, metadata)?;
        return Ok(());
    }

    // Ensure required columns exist
    add_missing_metadata_columns(conn, &meta_table)?;

    // Ensure one row for each column index exists
    populate_missing_column_rows(conn, table_name, &meta_table, metadata)?;

    Ok(())
}

/// Add missing columns to metadata table
fn add_missing_metadata_columns(conn: &Connection, meta_table: &str) -> DbResult<()> {
    queries::add_column_if_missing(conn, meta_table, "display_name", "TEXT")?;
    queries::add_column_if_missing(conn, meta_table, "validator_type", "TEXT")?;
    queries::add_column_if_missing(conn, meta_table, "validator_config", "TEXT")?;
    queries::add_column_if_missing(conn, meta_table, "ai_context", "TEXT")?;
    queries::add_column_if_missing(conn, meta_table, "filter_expr", "TEXT")?;
    queries::add_column_if_missing(
        conn,
        meta_table,
        "ai_enable_row_generation",
        "INTEGER DEFAULT 0",
    )?;
    queries::add_column_if_missing(conn, meta_table, "ai_include_in_send", "INTEGER DEFAULT 1")?;
    Ok(())
}

/// Populate missing column rows in metadata table
fn populate_missing_column_rows(
    conn: &Connection,
    table_name: &str,
    meta_table: &str,
    metadata: &SheetMetadata,
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
        queries::insert_column_metadata_if_missing(
            &tx,
            meta_table,
            data_column_idx,
            &col.header,
            &dt,
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
) -> DbResult<()> {
    let groups_table = format!("{}_Metadata_Groups", table_name);
    let meta_table = format!("{}_Metadata", table_name);

    // Create the groups table
    queries::create_ai_groups_table(conn, &groups_table, &meta_table)?;

    // Ensure deleted column exists in metadata table
    queries::add_column_if_missing(conn, &meta_table, "deleted", "INTEGER DEFAULT 0")?;

    // Populate from metadata
    for group in &metadata.ai_schema_groups {
        for &col_idx in &group.included_columns {
            queries::insert_ai_group_column(
                conn,
                &groups_table,
                &meta_table,
                &group.name,
                col_idx as i32,
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
) -> DbResult<()> {
    let structure_table = format!("{}_{}", parent_table, col_def.header);

    info!("======================================");
    info!("CREATE_STRUCTURE_TABLE: {}", structure_table);

    // Build column definitions
    let col_defs = build_structure_column_definitions(col_def, structure_columns)?;

    // Check if table exists and needs recreation
    if should_recreate_structure_table(conn, &structure_table, &col_defs, structure_columns, col_def)? {
        info!("Schema mismatch detected, dropping and recreating table '{}'", structure_table);
        queries::drop_table(conn, &structure_table)?;
    }

    // Create the table
    queries::create_structure_data_table(conn, &structure_table, &col_defs)?;

    // Register in global metadata
    queries::register_structure_table(conn, &structure_table, parent_table, &col_def.header)?;

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

    let existing_cols = queries::get_table_columns(conn, structure_table)?;
    let expected_col_count = col_defs.len() - 1; // -1 for UNIQUE constraint
    let actual_col_count = existing_cols.len();

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
        "Table '{}' exists with {} columns, expected {} columns (from source with {} fields)",
        structure_table, actual_col_count, expected_col_count, source_count
    );

    if actual_col_count != expected_col_count {
        warn!(
            "Schema mismatch for '{}': existing={:?}, expected cols={}",
            structure_table, existing_cols, expected_col_count
        );
        return Ok(true);
    }

    Ok(false)
}

/// Insert table-level metadata
pub fn insert_table_metadata(
    conn: &Connection,
    table_name: &str,
    metadata: &SheetMetadata,
    display_order: Option<i32>,
) -> DbResult<()> {
    queries::upsert_table_metadata(
        conn,
        table_name,
        metadata.ai_enable_row_generation as i32,
        metadata.ai_general_rule.as_deref(),
        metadata.ai_active_schema_group.as_deref(),
        metadata.category.as_deref(),
        display_order,
        metadata.hidden as i32,
    )?;
    Ok(())
}
