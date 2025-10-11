// src/sheets/sheet_metadata/deserialization.rs
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

use crate::sheets::ai_schema::AiSchemaGroup;
use crate::sheets::column_definition::ColumnDefinition;
use crate::sheets::random_picker::RandomPickerSettings;
use crate::sheets::structure_field::StructureFieldDefinition;

use super::{SheetMetadata, StructureParentLink};

// Default function for ai_model_id
pub fn default_ai_model_id() -> String {
    "gemini-flash-latest".to_string()
}

// Deprecated AI sampling parameters - return None for deserialization
pub fn default_temperature_skip() -> Option<f32> {
    None
}

/// Default function for `requested_grounding_with_Google Search` field in `SheetMetadata`.
pub fn default_grounding_with_google_search() -> Option<bool> {
    Some(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LegacyStructureColumnMeta {
    pub columns: Vec<StructureFieldDefinition>,
    #[serde(default)]
    pub column_order: Vec<usize>,
    #[serde(default)]
    pub key_parent_column_index: Option<usize>,
    #[serde(default)]
    pub ancestor_key_parent_column_indices: Vec<usize>,
}

// Custom backward-compatible Deserialize implementation
impl<'de> Deserialize<'de> for SheetMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct CurrentSheetMetadataHelper {
            sheet_name: String,
            #[serde(default)]
            category: Option<String>,
            data_filename: String,
            #[serde(default)]
            columns: Vec<ColumnDefinition>,
            #[serde(default)]
            ai_general_rule: Option<String>,
            #[serde(default = "default_ai_model_id")]
            ai_model_id: String,
            #[serde(default)]
            ai_temperature: Option<f32>,
            #[serde(default)]
            ai_top_k: Option<i32>,
            #[serde(default)]
            ai_top_p: Option<f32>,
            #[serde(default = "default_grounding_with_google_search")]
            requested_grounding_with_google_search: Option<bool>,
            #[serde(default)]
            ai_enable_row_generation: bool,
            #[serde(default)]
            ai_schema_groups: Vec<AiSchemaGroup>,
            #[serde(default)]
            ai_active_schema_group: Option<String>,
            #[serde(default)]
            random_picker: Option<RandomPickerSettings>,
            #[serde(default)]
            structure_parent: Option<StructureParentLink>,
            #[serde(default)]
            hidden: bool,
            #[serde(default)]
            structures_meta: HashMap<String, LegacyStructureColumnMeta>,
        }

        let value = serde_json::Value::deserialize(deserializer)?;

        // Attempt current format
        if let Ok(cur) = CurrentSheetMetadataHelper::deserialize(value.clone()) {
            let mut meta = SheetMetadata {
                sheet_name: cur.sheet_name,
                category: cur.category,
                data_filename: cur.data_filename,
                columns: cur.columns,
                ai_general_rule: cur.ai_general_rule,
                ai_model_id: cur.ai_model_id,
                ai_temperature: cur.ai_temperature,
                requested_grounding_with_google_search: cur.requested_grounding_with_google_search,
                ai_enable_row_generation: cur.ai_enable_row_generation,
                ai_schema_groups: cur.ai_schema_groups,
                ai_active_schema_group: cur.ai_active_schema_group,
                random_picker: cur.random_picker,
                structure_parent: cur.structure_parent,
                hidden: cur.hidden,
            };

            // Auto-migrate deprecated AI sampling params if they equal legacy defaults
            if matches!(meta.ai_temperature, Some(t) if (t - 0.9).abs() < f32::EPSILON || (t - 1.0).abs() < f32::EPSILON)
            {
                meta.ai_temperature = None;
            }

            // Migrate legacy structures_meta map
            let mut legacy_map = cur.structures_meta;
            for (idx, col) in meta.columns.iter_mut().enumerate() {
                if let Some(inline_schema) = col.structure_schema.as_ref() {
                    if col.structure_column_order.is_none() {
                        col.structure_column_order = Some((0..inline_schema.len()).collect());
                    }
                }
                if let Some(entry) = legacy_map.remove(&format!("column_{}", idx)) {
                    col.structure_schema = Some(entry.columns.clone());
                    if col.structure_column_order.is_none() {
                        col.structure_column_order = Some(entry.column_order.clone());
                    }
                    if let Some(k) = entry.key_parent_column_index {
                        col.structure_key_parent_column_index = Some(k);
                    }
                    if !entry.ancestor_key_parent_column_indices.is_empty() {
                        col.structure_ancestor_key_parent_column_indices =
                            Some(entry.ancestor_key_parent_column_indices.clone());
                    }
                }
                col.width = None; // discard legacy width
            }

            meta.ensure_column_consistency();
            meta.ensure_ai_schema_groups_initialized();
            return Ok(meta);
        }

        // Fallback to legacy format
        super::legacy::deserialize_legacy_format(value)
    }
}
