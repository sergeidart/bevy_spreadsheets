// src/sheets/definitions/structure_field.rs
use serde::{Deserialize, Serialize};

use super::column_data_type::ColumnDataType;
use super::column_validator::ColumnValidator;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructureFieldDefinition {
    pub header: String,
    #[serde(default)]
    pub validator: Option<ColumnValidator>,
    #[serde(default)]
    pub data_type: ColumnDataType,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub ai_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_enable_row_generation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_include_in_send: Option<bool>,
    // Legacy width accepted but not serialized
    #[serde(default, skip_serializing)]
    pub width: Option<f32>,
    #[serde(default)]
    pub structure_schema: Option<Vec<StructureFieldDefinition>>,
    // NEW: Persist nested structure metadata for deeper levels
    #[serde(default)]
    pub structure_column_order: Option<Vec<usize>>,
    #[serde(default)]
    pub structure_key_parent_column_index: Option<usize>,
    /// DEPRECATED: Previously stored indices of grand_N_parent columns.
    /// Now always empty/None - lineage is walked programmatically.
    /// Kept for backward compatibility with old metadata files.
    #[serde(default)]
    pub structure_ancestor_key_parent_column_indices: Option<Vec<usize>>,
}
