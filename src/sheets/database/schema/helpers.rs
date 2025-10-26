// src/sheets/database/schema/helpers.rs

use crate::sheets::definitions::ColumnDataType;

/// SQL type mapping for column data types
pub fn sql_type_for_column(data_type: ColumnDataType) -> &'static str {
    match data_type {
        ColumnDataType::String => "TEXT",
        ColumnDataType::Bool => "INTEGER",
        ColumnDataType::I64 => "INTEGER",
        ColumnDataType::F64 => "REAL",
    }
}

/// Produce a safe SQL identifier fragment suitable for use in unquoted index names.
/// Replaces any character that is not [A-Za-z0-9_] with an underscore and collapses repeats.
pub fn sanitize_identifier(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_us = false;
    for ch in name.chars() {
        let is_ok = ch.is_ascii_alphanumeric() || ch == '_';
        if is_ok {
            out.push(ch);
            last_us = false;
        } else if !last_us {
            out.push('_');
            last_us = true;
        }
    }
    // Trim leading/trailing underscores
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "idx".to_string()
    } else {
        trimmed
    }
}
