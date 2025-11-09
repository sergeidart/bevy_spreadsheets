// src/sheets/database/reader/column_parser.rs

use super::super::error::DbResult;
use super::super::schema::metadata_type_to_column_data_type;
use super::queries::MetadataColumnRow;
use crate::sheets::definitions::{ColumnDefinition, ColumnValidator};

/// Parse metadata column rows into ColumnDefinition objects
/// 
/// Converts raw database metadata rows into structured ColumnDefinition objects,
/// handling validator deserialization, type mapping, and filtering deleted columns.
pub fn parse_metadata_columns(
    meta_rows: Vec<MetadataColumnRow>,
) -> DbResult<Vec<ColumnDefinition>> {
    let mut columns = Vec::new();

    for row in meta_rows {
        let data_type = metadata_type_to_column_data_type(&row.data_type);

        let validator = match row.validator_type.as_deref() {
            Some("Basic") => Some(ColumnValidator::Basic(data_type)),
            Some("Linked") => {
                if let Some(config_json) = row.validator_config {
                    let config: serde_json::Value =
                        serde_json::from_str(&config_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;
                    Some(ColumnValidator::Linked {
                        target_sheet_name: config["target_table"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        target_column_index: config["target_column_index"]
                            .as_u64()
                            .unwrap_or(0) as usize,
                    })
                } else {
                    None
                }
            }
            Some("Structure") => Some(ColumnValidator::Structure),
            _ => None,
        };

        let deleted = row.deleted.map(|v| v != 0).unwrap_or(false);

        // Skip deleted columns
        if deleted {
            continue;
        }

        columns.push(ColumnDefinition {
            header: row.column_name,
            display_header: row.display_name,
            validator,
            data_type,
            filter: row.filter_expr,
            ai_context: row.ai_context,
            ai_enable_row_generation: row.ai_enable_row_generation.map(|v| v != 0),
            ai_include_in_send: row.ai_include_in_send.map(|v| v != 0),
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
            deleted: false,
            hidden: false,
        });
    }

    Ok(columns)
}
