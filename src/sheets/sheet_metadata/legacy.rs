// src/sheets/sheet_metadata/legacy.rs
use bevy::prelude::info;
use serde::{de, Deserialize};
use serde_json::Value;

use crate::sheets::column_data_type::{parse_column_data_type, ColumnDataType};
use crate::sheets::column_definition::ColumnDefinition;
use crate::sheets::column_validator::{parse_legacy_validator, ColumnValidator};
use crate::sheets::random_picker::RandomPickerSettings;

use super::deserialization::{default_ai_model_id, default_grounding_with_google_search};
use super::SheetMetadata;

#[derive(Deserialize, Default)]
struct LegacySheetMetadataHelper {
    sheet_name: Option<String>,
    category: Option<String>,
    data_filename: Option<String>,
    column_headers: Option<Vec<String>>,
    column_types: Option<Vec<String>>,
    column_validators: Option<Vec<String>>,
    column_filters: Option<Vec<Option<String>>>,
    ai_general_rule: Option<String>,
    ai_model_id: Option<String>,
    ai_temperature: Option<f32>,
    ai_top_k: Option<i32>,
    ai_top_p: Option<f32>,
    requested_grounding_with_google_search: Option<bool>,
    random_picker: Option<RandomPickerSettings>,
}

pub(super) fn deserialize_legacy_format<E: de::Error>(value: Value) -> Result<SheetMetadata, E> {
    let legacy: LegacySheetMetadataHelper =
        LegacySheetMetadataHelper::deserialize(value).map_err(|e| {
            de::Error::custom(format!(
                "Failed to parse SheetMetadata (current or legacy): {}",
                e
            ))
        })?;

    let headers = legacy.column_headers.unwrap_or_default();
    let types = legacy.column_types.unwrap_or_default();
    let filters = legacy.column_filters.unwrap_or_default();
    let validators = legacy.column_validators.unwrap_or_default();

    let len = headers.len();
    let mut columns: Vec<ColumnDefinition> = Vec::with_capacity(len);
    for i in 0..len {
        let header = headers
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("Column {}", i + 1));
        let type_str = types
            .get(i)
            .cloned()
            .unwrap_or_else(|| "String".to_string());
        let data_type = parse_column_data_type(&type_str).unwrap_or(ColumnDataType::String);
        let filter_val = filters.get(i).cloned().unwrap_or(None);
        let validator_val: Option<ColumnValidator> = validators
            .get(i)
            .and_then(|raw| parse_legacy_validator(raw, data_type));
        let final_validator = validator_val.or(Some(ColumnValidator::Basic(data_type)));
        columns.push(ColumnDefinition {
            header,
            validator: final_validator,
            data_type,
            filter: filter_val,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            deleted: false,
            hidden: false,
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        });
    }

    let mut meta = SheetMetadata {
        sheet_name: legacy.sheet_name.unwrap_or_else(|| "Unnamed".to_string()),
        category: legacy.category,
        data_filename: legacy
            .data_filename
            .unwrap_or_else(|| "unknown.json".to_string()),
        columns,
        ai_general_rule: legacy.ai_general_rule,
        ai_model_id: legacy.ai_model_id.unwrap_or_else(default_ai_model_id),
        ai_temperature: legacy.ai_temperature,
        requested_grounding_with_google_search: legacy
            .requested_grounding_with_google_search
            .or_else(default_grounding_with_google_search),
        ai_enable_row_generation: false,
        ai_schema_groups: Vec::new(),
        ai_active_schema_group: None,
        random_picker: legacy.random_picker,
        structure_parent: None,
        hidden: false,
    };

    // Auto-migrate deprecated AI sampling params if they equal legacy defaults
    if matches!(meta.ai_temperature, Some(t) if (t - 0.9).abs() < f32::EPSILON || (t - 1.0).abs() < f32::EPSILON)
    {
        meta.ai_temperature = None;
    }

    meta.ensure_column_consistency();
    meta.ensure_ai_schema_groups_initialized();
    info!(
        "Loaded legacy metadata for sheet '{}': {} columns",
        meta.sheet_name,
        meta.columns.len()
    );
    Ok(meta)
}
