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
