// src/sheets/sheet_metadata/mod.rs
mod deserialization;
mod legacy;
mod structure_helpers;
mod ai_schema_helpers;

use bevy::prelude::warn;
use serde::{Deserialize, Serialize};

use super::ai_schema::{AiSchemaGroup, AiSchemaGroupStructureOverride};
use super::column_data_type::ColumnDataType;
use super::column_definition::ColumnDefinition;
use super::column_validator::ColumnValidator;
use super::random_picker::RandomPickerSettings;
use super::structure_field::StructureFieldDefinition;

// Re-export for backward compatibility
pub use deserialization::default_ai_model_id;
pub use deserialization::default_grounding_with_google_search;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureParentLink {
    pub parent_category: Option<String>,
    pub parent_sheet: String,
    pub parent_column_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SheetMetadata {
    pub sheet_name: String,
    #[serde(default)]
    pub category: Option<String>,
    pub data_filename: String,
    #[serde(default)]
    pub columns: Vec<ColumnDefinition>,
    #[serde(default)]
    pub ai_general_rule: Option<String>,
    #[serde(default = "default_ai_model_id")]
    pub ai_model_id: String,
    #[serde(
        default = "deserialization::default_temperature_skip",
        skip_serializing_if = "Option::is_none"
    )]
    pub ai_temperature: Option<f32>,

    #[serde(default = "default_grounding_with_google_search")]
    pub requested_grounding_with_google_search: Option<bool>,
    #[serde(default)]
    pub ai_enable_row_generation: bool,
    #[serde(default)]
    pub ai_schema_groups: Vec<AiSchemaGroup>,
    #[serde(default)]
    pub ai_active_schema_group: Option<String>,

    #[serde(default)]
    pub random_picker: Option<RandomPickerSettings>,
    #[serde(default)]
    pub structure_parent: Option<StructureParentLink>,
    #[serde(default)]
    pub hidden: bool,
}

impl SheetMetadata {
    pub fn create_generic(
        name: String,
        filename: String,
        num_cols: usize,
        category: Option<String>,
    ) -> Self {
        let columns = (0..num_cols)
            .map(|i| {
                ColumnDefinition::new_basic(format!("Column {}", i + 1), ColumnDataType::String)
            })
            .collect();

        let mut meta = SheetMetadata {
            sheet_name: name,
            category,
            data_filename: filename,
            columns,
            ai_general_rule: None,
            ai_model_id: default_ai_model_id(),
            ai_temperature: None,
            requested_grounding_with_google_search: default_grounding_with_google_search(),
            ai_enable_row_generation: false,
            ai_schema_groups: Vec::new(),
            ai_active_schema_group: None,
            random_picker: None,
            structure_parent: None,
            hidden: false,
        };
        meta.ensure_ai_schema_groups_initialized();
        meta
    }

    pub fn ensure_column_consistency(&mut self) -> bool {
        let mut changed = false;
        for column in self.columns.iter_mut() {
            if column.validator.is_none() {
                warn!(
                    "Initializing missing validator for column '{}' in sheet '{}' based on type {:?}.",
                    column.header, self.sheet_name, column.data_type
                );
                column.validator = Some(ColumnValidator::Basic(column.data_type));
                changed = true;
            }
            if column.ensure_type_consistency() {
                warn!(
                    "Corrected data type inconsistency for column '{}' in sheet '{}'.",
                    column.header, self.sheet_name
                );
                changed = true;
            }
        }
        changed
    }

    pub fn get_headers(&self) -> Vec<String> {
        self.columns.iter().map(|c| c.header.clone()).collect()
    }

    pub fn get_filters(&self) -> Vec<Option<String>> {
        self.columns.iter().map(|c| c.filter.clone()).collect()
    }

    pub fn ai_included_column_indices(&self) -> Vec<usize> {
        self.columns
            .iter()
            .enumerate()
            .filter_map(|(idx, column)| {
                if matches!(column.validator, Some(ColumnValidator::Structure)) {
                    return None;
                }
                if matches!(column.ai_include_in_send, Some(false)) {
                    None
                } else {
                    Some(idx)
                }
            })
            .collect()
    }

    pub fn ai_included_structure_paths(&self) -> Vec<Vec<usize>> {
        structure_helpers::collect_included_structure_paths(&self.columns)
    }

    pub fn ensure_ai_schema_groups_initialized(&mut self) {
        ai_schema_helpers::ensure_ai_schema_groups_initialized(self);
    }

    pub fn set_active_ai_schema_group_included_columns(&mut self, included: &[usize]) -> bool {
        ai_schema_helpers::set_active_ai_schema_group_included_columns(self, included)
    }

    pub fn set_active_ai_schema_group_included_structures(
        &mut self,
        included_paths: &[Vec<usize>],
    ) -> bool {
        ai_schema_helpers::set_active_ai_schema_group_included_structures(self, included_paths)
    }

    pub fn set_active_ai_schema_group_allow_rows(&mut self, allow: bool) -> bool {
        ai_schema_helpers::set_active_ai_schema_group_allow_rows(self, allow)
    }

    pub fn apply_structure_send_inclusion(&mut self, included_paths: &[Vec<usize>]) -> bool {
        structure_helpers::apply_structure_send_inclusion(&mut self.columns, included_paths)
    }

    pub fn set_active_ai_schema_group_structure_override(
        &mut self,
        path: &[usize],
        override_value: Option<bool>,
    ) -> bool {
        ai_schema_helpers::set_active_ai_schema_group_structure_override(
            self,
            path,
            override_value,
        )
    }

    pub fn describe_structure_path(&self, path: &[usize]) -> Option<String> {
        structure_helpers::describe_structure_path(&self.columns, path)
    }

    pub fn structure_fields_for_path(
        &self,
        path: &[usize],
    ) -> Option<Vec<StructureFieldDefinition>> {
        structure_helpers::structure_fields_for_path(&self.columns, path)
    }

    pub fn collect_structure_row_generation_overrides(
        &self,
    ) -> Vec<AiSchemaGroupStructureOverride> {
        structure_helpers::collect_structure_row_generation_overrides(&self.columns)
    }

    pub fn apply_structure_row_generation_overrides(
        &mut self,
        overrides: &[AiSchemaGroupStructureOverride],
    ) -> bool {
        structure_helpers::apply_structure_row_generation_overrides(&mut self.columns, overrides)
    }

    pub fn structure_path_exists(&self, path: &[usize]) -> bool {
        structure_helpers::structure_path_exists(&self.columns, path)
    }

    pub fn apply_ai_schema_group(&mut self, group_name: &str) -> Result<bool, String> {
        ai_schema_helpers::apply_ai_schema_group(self, group_name)
    }

    pub fn ensure_unique_schema_group_name(&self, desired: &str) -> String {
        ai_schema_helpers::ensure_unique_schema_group_name(self, desired)
    }
}
