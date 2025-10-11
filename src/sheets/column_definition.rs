// src/sheets/definitions/column_definition.rs
use serde::{Deserialize, Serialize};

use super::column_data_type::ColumnDataType;
use super::column_validator::ColumnValidator;
use super::structure_field::StructureFieldDefinition;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
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
    /// Marked deleted (hidden for reuse)
    #[serde(default)]
    pub deleted: bool,
    // Legacy width accepted but never serialized (feature removed)
    #[serde(default, skip_serializing)]
    pub width: Option<f32>,
    #[serde(default)]
    pub structure_schema: Option<Vec<StructureFieldDefinition>>, // Only when validator == Some(Structure)
    // Inline structure metadata (replaces SheetMetadata.structures_meta)
    #[serde(default)]
    pub structure_column_order: Option<Vec<usize>>,
    #[serde(default)]
    pub structure_key_parent_column_index: Option<usize>,
    #[serde(default)]
    pub structure_ancestor_key_parent_column_indices: Option<Vec<usize>>,
}

impl From<&ColumnDefinition> for StructureFieldDefinition {
    fn from(c: &ColumnDefinition) -> Self {
        StructureFieldDefinition {
            header: c.header.clone(),
            validator: c.validator.clone(),
            data_type: c.data_type,
            filter: c.filter.clone(),
            ai_context: c.ai_context.clone(),
            ai_enable_row_generation: c.ai_enable_row_generation,
            ai_include_in_send: c.ai_include_in_send,
            width: None,
            structure_schema: c.structure_schema.clone(),
            structure_column_order: c.structure_column_order.clone(),
            structure_key_parent_column_index: c.structure_key_parent_column_index,
            structure_ancestor_key_parent_column_indices: c
                .structure_ancestor_key_parent_column_indices
                .clone(),
        }
    }
}

impl ColumnDefinition {
    pub fn new_basic(header: String, data_type: ColumnDataType) -> Self {
        ColumnDefinition {
            header,
            validator: Some(ColumnValidator::Basic(data_type)),
            data_type,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            deleted: false,
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        }
    }

    pub fn ensure_type_consistency(&mut self) -> bool {
        let expected_type = match &self.validator {
            Some(ColumnValidator::Basic(t)) => *t,
            Some(ColumnValidator::Linked { .. }) => ColumnDataType::String,
            Some(ColumnValidator::Structure) => ColumnDataType::String,
            None => ColumnDataType::String,
        };
        if self.data_type != expected_type {
            self.data_type = expected_type;
            true
        } else {
            false
        }
    }
}
