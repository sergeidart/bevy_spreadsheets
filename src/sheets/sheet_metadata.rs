// src/sheets/definitions/sheet_metadata.rs
use bevy::prelude::{info, warn};
use serde::{
    de::{self, Deserializer},
    Deserialize, Serialize,
};
use std::collections::{HashMap, HashSet};

use super::ai_schema::{AiSchemaGroup, AiSchemaGroupStructureOverride};
use super::column_data_type::{parse_column_data_type, ColumnDataType};
use super::column_definition::ColumnDefinition;
use super::column_validator::{parse_legacy_validator, ColumnValidator};
use super::random_picker::RandomPickerSettings;
use super::structure_field::StructureFieldDefinition;

// Default function for ai_model_id
pub fn default_ai_model_id() -> String {
    "gemini-flash-latest".to_string() // Default model
}

// Deprecated AI sampling parameters (kept for backward compatibility on load only)
pub fn default_temperature() -> Option<f32> {
    None
}
pub fn default_top_k() -> Option<i32> {
    None
}
pub fn default_top_p() -> Option<f32> {
    None
}

/// Default function for `requested_grounding_with_Google Search` field in `SheetMetadata`.
pub fn default_grounding_with_google_search() -> Option<bool> {
    Some(false) // Default to false (or true if you prefer grounding by default)
}

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
        default = "default_temperature",
        skip_serializing_if = "Option::is_none"
    )]
    pub ai_temperature: Option<f32>,
    #[serde(default = "default_top_k", skip_serializing_if = "Option::is_none")]
    pub ai_top_k: Option<i32>,
    #[serde(default = "default_top_p", skip_serializing_if = "Option::is_none")]
    pub ai_top_p: Option<f32>,

    // Use the defined default function for serde
    #[serde(default = "default_grounding_with_google_search")]
    pub requested_grounding_with_google_search: Option<bool>,
    // NEW: Allow AI to generate additional rows (persisted per sheet)
    #[serde(default)]
    pub ai_enable_row_generation: bool,
    #[serde(default)]
    pub ai_schema_groups: Vec<AiSchemaGroup>,
    #[serde(default)]
    pub ai_active_schema_group: Option<String>,

    // NEW: Optional per-sheet Random Picker settings
    #[serde(default)]
    pub random_picker: Option<RandomPickerSettings>,
    // If this is a virtual structure sheet, link back to parent sheet & column
    #[serde(default)]
    pub structure_parent: Option<StructureParentLink>,
    /// New: whether the sheet is hidden from lists by default (can be overridden in Settings)
    #[serde(default)]
    pub hidden: bool,
    // structures_meta removed; legacy field is migrated during deserialization into column-level fields
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyStructureColumnMeta {
    pub columns: Vec<StructureFieldDefinition>,
    #[serde(default)]
    pub column_order: Vec<usize>,
    #[serde(default)]
    pub key_parent_column_index: Option<usize>,
    #[serde(default)]
    pub ancestor_key_parent_column_indices: Vec<usize>,
}

// Custom backward-compatible Deserialize implementation to support legacy metadata formats.
impl<'de> Deserialize<'de> for SheetMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // First try the current format directly by attempting to deserialize into an identical helper.
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
            #[serde(default = "default_temperature")]
            ai_temperature: Option<f32>,
            #[serde(default = "default_top_k")]
            ai_top_k: Option<i32>,
            #[serde(default = "default_top_p")]
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

        // Legacy format helper (arrays instead of columns vector)
        #[derive(Deserialize, Default)]
        struct LegacySheetMetadataHelper {
            sheet_name: Option<String>,
            category: Option<String>,
            data_filename: Option<String>,
            column_headers: Option<Vec<String>>,
            column_types: Option<Vec<String>>,
            column_validators: Option<Vec<String>>, // Legacy simple representation; ignored mostly
            column_filters: Option<Vec<Option<String>>>,
            ai_general_rule: Option<String>,
            ai_model_id: Option<String>,
            ai_temperature: Option<f32>,
            ai_top_k: Option<i32>,
            ai_top_p: Option<f32>,
            requested_grounding_with_google_search: Option<bool>,
            random_picker: Option<RandomPickerSettings>,
        }

        // We need the raw value to attempt both.
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
                ai_temperature: cur.ai_temperature, // retained for backward compatibility
                ai_top_k: cur.ai_top_k,
                ai_top_p: cur.ai_top_p,
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
            if matches!(meta.ai_top_k, Some(k) if k == 1) {
                meta.ai_top_k = None;
            }
            if matches!(meta.ai_top_p, Some(p) if (p - 0.95).abs() < f32::EPSILON || (p - 1.0).abs() < f32::EPSILON)
            {
                meta.ai_top_p = None;
            }
            // Migrate legacy map + inline legacy schemas
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

        // Fallback legacy
        let legacy: LegacySheetMetadataHelper =
            LegacySheetMetadataHelper::deserialize(value.clone()).map_err(|e| {
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
            // Attempt validator parse (basic only); ignore complex variants in legacy for now
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
                hidden: false, // Legacy migration: default to visible
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
            ai_model_id: legacy.ai_model_id.unwrap_or_else(|| default_ai_model_id()),
            ai_temperature: legacy.ai_temperature,
            ai_top_k: legacy.ai_top_k,
            ai_top_p: legacy.ai_top_p,
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
        if matches!(meta.ai_temperature, Some(t) if (t - 0.9).abs() < f32::EPSILON || (t - 1.0).abs() < f32::EPSILON)
        {
            meta.ai_temperature = None;
        }
        if matches!(meta.ai_top_k, Some(k) if k == 1) {
            meta.ai_top_k = None;
        }
        if matches!(meta.ai_top_p, Some(p) if (p - 0.95).abs() < f32::EPSILON || (p - 1.0).abs() < f32::EPSILON)
        {
            meta.ai_top_p = None;
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
            ai_top_k: None,
            ai_top_p: None,
            // Call the defined function for initialization
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
        let mut paths: Vec<Vec<usize>> = Vec::new();
        for (column_index, column) in self.columns.iter().enumerate() {
            if matches!(column.validator, Some(ColumnValidator::Structure)) {
                let mut path = vec![column_index];
                if matches!(column.ai_include_in_send, Some(true)) {
                    paths.push(path.clone());
                }
                if let Some(schema) = column.structure_schema.as_ref() {
                    collect_included_structure_paths_from_fields(schema, &mut path, &mut paths);
                }
            } else if let Some(schema) = column.structure_schema.as_ref() {
                let mut path = vec![column_index];
                collect_included_structure_paths_from_fields(schema, &mut path, &mut paths);
            }
        }
        paths.sort();
        paths.dedup();
        paths
    }

    pub fn ensure_ai_schema_groups_initialized(&mut self) {
        let column_count = self.columns.len();
        let valid_structure_paths = collect_all_structure_paths(&self.columns);
        for group in self.ai_schema_groups.iter_mut() {
            group.included_columns.retain(|idx| {
                *idx < column_count
                    && !matches!(
                        self.columns[*idx].validator,
                        Some(ColumnValidator::Structure)
                    )
            });
            group.included_columns.sort_unstable();
            group.included_columns.dedup();
            group
                .structure_row_generation_overrides
                .retain(|override_entry| valid_structure_paths.contains(&override_entry.path));
            group
                .structure_row_generation_overrides
                .sort_by(|a, b| a.path.cmp(&b.path));
            group
                .included_structures
                .retain(|path| valid_structure_paths.contains(path));
            group.included_structures.sort();
            group.included_structures.dedup();
        }

        if self.ai_schema_groups.is_empty() {
            let default_name = "Default".to_string();
            self.ai_schema_groups.push(AiSchemaGroup {
                name: default_name.clone(),
                included_columns: self.ai_included_column_indices(),
                allow_add_rows: self.ai_enable_row_generation,
                structure_row_generation_overrides: self
                    .collect_structure_row_generation_overrides(),
                included_structures: self.ai_included_structure_paths(),
            });
            self.ai_active_schema_group = Some(default_name);
            return;
        }

        if let Some(active_name) = self.ai_active_schema_group.clone() {
            if !self
                .ai_schema_groups
                .iter()
                .any(|group| group.name == active_name)
            {
                self.ai_active_schema_group = None;
            }
        }

        if self.ai_active_schema_group.is_none() {
            if let Some(first) = self.ai_schema_groups.first() {
                self.ai_active_schema_group = Some(first.name.clone());
            }
        }
    }

    pub fn set_active_ai_schema_group_included_columns(&mut self, included: &[usize]) -> bool {
        let filtered: Vec<usize> = included
            .iter()
            .copied()
            .filter(|idx| {
                *idx < self.columns.len()
                    && !matches!(
                        self.columns[*idx].validator,
                        Some(ColumnValidator::Structure)
                    )
            })
            .collect();
        if let Some(active_name) = self.ai_active_schema_group.clone() {
            if let Some(group) = self
                .ai_schema_groups
                .iter_mut()
                .find(|g| g.name == active_name)
            {
                if group.included_columns != filtered {
                    group.included_columns = filtered;
                    return true;
                }
            }
        }
        false
    }

    pub fn set_active_ai_schema_group_included_structures(
        &mut self,
        included_paths: &[Vec<usize>],
    ) -> bool {
        let valid_paths = collect_all_structure_paths(&self.columns);
        let mut filtered: Vec<Vec<usize>> = included_paths
            .iter()
            .filter(|path| valid_paths.contains(*path))
            .cloned()
            .collect();
        filtered.sort();
        filtered.dedup();

        if let Some(active_name) = self.ai_active_schema_group.clone() {
            if let Some(group) = self
                .ai_schema_groups
                .iter_mut()
                .find(|g| g.name == active_name)
            {
                if group.included_structures != filtered {
                    group.included_structures = filtered;
                    return true;
                }
            }
        }
        false
    }

    pub fn set_active_ai_schema_group_allow_rows(&mut self, allow: bool) -> bool {
        if let Some(active_name) = self.ai_active_schema_group.clone() {
            if let Some(group) = self
                .ai_schema_groups
                .iter_mut()
                .find(|g| g.name == active_name)
            {
                if group.allow_add_rows != allow {
                    group.allow_add_rows = allow;
                    return true;
                }
            }
        }
        false
    }

    pub fn apply_structure_send_inclusion(&mut self, included_paths: &[Vec<usize>]) -> bool {
        let included_set: HashSet<Vec<usize>> = included_paths.iter().cloned().collect();
        let mut changed = false;
        for (idx, column) in self.columns.iter_mut().enumerate() {
            let mut path = vec![idx];
            if apply_structure_send_flag_column(column, &mut path, &included_set) {
                changed = true;
            }
        }
        changed
    }

    pub fn set_active_ai_schema_group_structure_override(
        &mut self,
        path: &[usize],
        override_value: Option<bool>,
    ) -> bool {
        if path.is_empty() || !self.structure_path_exists(path) {
            return false;
        }

        let Some(active_name) = self.ai_active_schema_group.clone() else {
            return false;
        };

        let Some(group) = self
            .ai_schema_groups
            .iter_mut()
            .find(|g| g.name == active_name)
        else {
            return false;
        };

        if let Some(value) = override_value {
            if let Some(entry) = group
                .structure_row_generation_overrides
                .iter_mut()
                .find(|entry| entry.path == path)
            {
                if entry.allow_add_rows != value {
                    entry.allow_add_rows = value;
                    return true;
                }
                return false;
            }

            group
                .structure_row_generation_overrides
                .push(AiSchemaGroupStructureOverride {
                    path: path.to_vec(),
                    allow_add_rows: value,
                });
            group
                .structure_row_generation_overrides
                .sort_by(|a, b| a.path.cmp(&b.path));
            true
        } else {
            let original_len = group.structure_row_generation_overrides.len();
            group
                .structure_row_generation_overrides
                .retain(|entry| entry.path != path);
            original_len != group.structure_row_generation_overrides.len()
        }
    }

    pub fn describe_structure_path(&self, path: &[usize]) -> Option<String> {
        if path.is_empty() {
            return None;
        }
        let mut names: Vec<String> = Vec::new();
        let column = self.columns.get(path[0])?;
        names.push(column.header.clone());
        if path.len() == 1 {
            return Some(names.join(" -> "));
        }
        let mut field = column.structure_schema.as_ref()?.get(path[1])?;
        names.push(field.header.clone());
        for idx in path.iter().skip(2) {
            field = field.structure_schema.as_ref()?.get(*idx)?;
            names.push(field.header.clone());
        }
        Some(names.join(" -> "))
    }

    pub fn structure_fields_for_path(
        &self,
        path: &[usize],
    ) -> Option<Vec<StructureFieldDefinition>> {
        if path.is_empty() {
            return None;
        }
        let column = self.columns.get(path[0])?;
        if path.len() == 1 {
            return column.structure_schema.clone();
        }
        let mut field = column.structure_schema.as_ref()?.get(path[1])?;
        if path.len() == 2 {
            return field.structure_schema.clone();
        }
        for idx in path.iter().skip(2) {
            field = field.structure_schema.as_ref()?.get(*idx)?;
        }
        field.structure_schema.clone()
    }

    pub fn collect_structure_row_generation_overrides(
        &self,
    ) -> Vec<AiSchemaGroupStructureOverride> {
        let mut overrides: Vec<AiSchemaGroupStructureOverride> = Vec::new();
        for (column_index, column) in self.columns.iter().enumerate() {
            collect_structure_row_overrides_from_column(column, column_index, &mut overrides);
        }
        overrides.sort_by(|a, b| a.path.cmp(&b.path));
        overrides
    }

    pub fn apply_structure_row_generation_overrides(
        &mut self,
        overrides: &[AiSchemaGroupStructureOverride],
    ) -> bool {
        let mut desired: HashMap<Vec<usize>, bool> = overrides
            .iter()
            .filter_map(|entry| {
                if self.structure_path_exists(&entry.path) {
                    Some((entry.path.clone(), entry.allow_add_rows))
                } else {
                    warn!(
                        "Skipping AI schema group structure override with invalid path: {:?}",
                        entry.path
                    );
                    None
                }
            })
            .collect();

        let mut changed = false;

        for (column_index, column) in self.columns.iter_mut().enumerate() {
            let mut path = vec![column_index];
            if reconcile_column_structure_overrides(column, &mut path, &mut desired) {
                changed = true;
            }
        }

        for (path, value) in desired.drain() {
            if self.apply_structure_row_generation_override_path(&path, value) {
                changed = true;
            }
        }

        changed
    }

    fn structure_path_exists(&self, path: &[usize]) -> bool {
        structure_path_exists_in_columns(&self.columns, path)
    }

    fn apply_structure_row_generation_override_path(
        &mut self,
        path: &[usize],
        allow: bool,
    ) -> bool {
        apply_structure_row_generation_override_to_columns(&mut self.columns, path, allow)
    }

    pub fn apply_ai_schema_group(&mut self, group_name: &str) -> Result<bool, String> {
        let group = self
            .ai_schema_groups
            .iter()
            .find(|g| g.name == group_name)
            .cloned()
            .ok_or_else(|| format!("AI schema group '{}' not found", group_name))?;

        let included_set: HashSet<usize> = group.included_columns.iter().copied().collect();
        let mut changed = false;

        for (idx, column) in self.columns.iter_mut().enumerate() {
            if matches!(column.validator, Some(ColumnValidator::Structure)) {
                continue;
            }
            let should_include = included_set.contains(&idx);
            if should_include {
                if column.ai_include_in_send.is_some() {
                    column.ai_include_in_send = None;
                    changed = true;
                }
            } else if column.ai_include_in_send != Some(false) {
                column.ai_include_in_send = Some(false);
                changed = true;
            }
        }

        if self.ai_enable_row_generation != group.allow_add_rows {
            self.ai_enable_row_generation = group.allow_add_rows;
            changed = true;
        }

        if self.apply_structure_row_generation_overrides(&group.structure_row_generation_overrides)
        {
            changed = true;
        }

        if self.apply_structure_send_inclusion(&group.included_structures) {
            changed = true;
        }

        if self.ai_active_schema_group.as_deref() != Some(group_name) {
            self.ai_active_schema_group = Some(group_name.to_string());
            changed = true;
        }

        Ok(changed)
    }

    pub fn ensure_unique_schema_group_name(&self, desired: &str) -> String {
        if !self
            .ai_schema_groups
            .iter()
            .any(|g| g.name.eq_ignore_ascii_case(desired))
        {
            return desired.to_string();
        }

        let mut counter = 2usize;
        let base = desired.trim();
        loop {
            let candidate = format!("{} {}", base, counter);
            if !self
                .ai_schema_groups
                .iter()
                .any(|g| g.name.eq_ignore_ascii_case(&candidate))
            {
                return candidate;
            }
            counter += 1;
        }
    }
}

// Helper functions used by SheetMetadata

fn collect_structure_row_overrides_from_column(
    column: &ColumnDefinition,
    column_index: usize,
    output: &mut Vec<AiSchemaGroupStructureOverride>,
) {
    if let Some(value) = column.ai_enable_row_generation {
        output.push(AiSchemaGroupStructureOverride {
            path: vec![column_index],
            allow_add_rows: value,
        });
    }

    if let Some(schema) = column.structure_schema.as_ref() {
        for (field_index, field) in schema.iter().enumerate() {
            let mut path = vec![column_index, field_index];
            collect_structure_row_overrides_from_field(field, &mut path, output);
        }
    }
}

fn collect_structure_row_overrides_from_field(
    field: &StructureFieldDefinition,
    path: &mut Vec<usize>,
    output: &mut Vec<AiSchemaGroupStructureOverride>,
) {
    if let Some(value) = field.ai_enable_row_generation {
        output.push(AiSchemaGroupStructureOverride {
            path: path.clone(),
            allow_add_rows: value,
        });
    }

    if let Some(schema) = field.structure_schema.as_ref() {
        for (child_index, child_field) in schema.iter().enumerate() {
            path.push(child_index);
            collect_structure_row_overrides_from_field(child_field, path, output);
            path.pop();
        }
    }
}

fn collect_all_structure_paths(columns: &[ColumnDefinition]) -> HashSet<Vec<usize>> {
    let mut paths: HashSet<Vec<usize>> = HashSet::new();
    for (column_index, column) in columns.iter().enumerate() {
        if !matches!(column.validator, Some(ColumnValidator::Structure))
            && column.structure_schema.is_none()
        {
            continue;
        }
        let mut path = vec![column_index];
        paths.insert(path.clone());
        if let Some(schema) = column.structure_schema.as_ref() {
            collect_structure_paths_from_fields(schema, &mut path, &mut paths);
        }
    }
    paths
}

fn collect_structure_paths_from_fields(
    fields: &[StructureFieldDefinition],
    path: &mut Vec<usize>,
    output: &mut HashSet<Vec<usize>>,
) {
    for (index, field) in fields.iter().enumerate() {
        path.push(index);
        output.insert(path.clone());
        if let Some(schema) = field.structure_schema.as_ref() {
            collect_structure_paths_from_fields(schema, path, output);
        }
        path.pop();
    }
}

fn collect_included_structure_paths_from_fields(
    fields: &[StructureFieldDefinition],
    path: &mut Vec<usize>,
    output: &mut Vec<Vec<usize>>,
) {
    for (index, field) in fields.iter().enumerate() {
        path.push(index);
        if matches!(field.validator, Some(ColumnValidator::Structure)) {
            if matches!(field.ai_include_in_send, Some(true)) {
                output.push(path.clone());
            }
            if let Some(schema) = field.structure_schema.as_ref() {
                collect_included_structure_paths_from_fields(schema, path, output);
            }
        } else if let Some(schema) = field.structure_schema.as_ref() {
            collect_included_structure_paths_from_fields(schema, path, output);
        }
        path.pop();
    }
}

fn apply_structure_send_flag_column(
    column: &mut ColumnDefinition,
    path: &mut Vec<usize>,
    included: &HashSet<Vec<usize>>,
) -> bool {
    let mut changed = false;
    if matches!(column.validator, Some(ColumnValidator::Structure)) {
        if included.contains(path) {
            if column.ai_include_in_send != Some(true) {
                column.ai_include_in_send = Some(true);
                changed = true;
            }
        } else if column.ai_include_in_send != Some(false) {
            column.ai_include_in_send = Some(false);
            changed = true;
        }
    }
    if let Some(schema) = column.structure_schema.as_mut() {
        for (idx, field) in schema.iter_mut().enumerate() {
            path.push(idx);
            if apply_structure_send_flag_field(field, path, included) {
                changed = true;
            }
            path.pop();
        }
    }
    changed
}

fn apply_structure_send_flag_field(
    field: &mut StructureFieldDefinition,
    path: &mut Vec<usize>,
    included: &HashSet<Vec<usize>>,
) -> bool {
    let mut changed = false;
    if matches!(field.validator, Some(ColumnValidator::Structure)) {
        if included.contains(path) {
            if field.ai_include_in_send != Some(true) {
                field.ai_include_in_send = Some(true);
                changed = true;
            }
        } else if field.ai_include_in_send != Some(false) {
            field.ai_include_in_send = Some(false);
            changed = true;
        }
    }
    if let Some(schema) = field.structure_schema.as_mut() {
        for (idx, child) in schema.iter_mut().enumerate() {
            path.push(idx);
            if apply_structure_send_flag_field(child, path, included) {
                changed = true;
            }
            path.pop();
        }
    }
    changed
}

fn structure_path_exists_in_columns(columns: &[ColumnDefinition], path: &[usize]) -> bool {
    let (first, rest) = match path.split_first() {
        Some(split) => split,
        None => return false,
    };
    let Some(column) = columns.get(*first) else {
        return false;
    };

    if rest.is_empty() {
        matches!(column.validator, Some(ColumnValidator::Structure))
            || column.structure_schema.is_some()
    } else if let Some(schema) = column.structure_schema.as_ref() {
        structure_path_exists_in_fields(schema, rest)
    } else {
        false
    }
}

fn structure_path_exists_in_fields(fields: &[StructureFieldDefinition], path: &[usize]) -> bool {
    let (first, rest) = match path.split_first() {
        Some(split) => split,
        None => return true,
    };
    let Some(field) = fields.get(*first) else {
        return false;
    };

    if rest.is_empty() {
        true
    } else if let Some(schema) = field.structure_schema.as_ref() {
        structure_path_exists_in_fields(schema, rest)
    } else {
        false
    }
}

fn apply_structure_row_generation_override_to_columns(
    columns: &mut [ColumnDefinition],
    path: &[usize],
    allow: bool,
) -> bool {
    let (first, rest) = match path.split_first() {
        Some(split) => split,
        None => return false,
    };
    let Some(column) = columns.get_mut(*first) else {
        return false;
    };

    if rest.is_empty() {
        if column.ai_enable_row_generation != Some(allow) {
            column.ai_enable_row_generation = Some(allow);
            return true;
        }
        return false;
    }

    let Some(schema) = column.structure_schema.as_mut() else {
        return false;
    };
    apply_structure_row_generation_override_to_fields(schema, rest, allow)
}

fn apply_structure_row_generation_override_to_fields(
    fields: &mut [StructureFieldDefinition],
    path: &[usize],
    allow: bool,
) -> bool {
    let (first, rest) = match path.split_first() {
        Some(split) => split,
        None => return false,
    };
    let Some(field) = fields.get_mut(*first) else {
        return false;
    };

    if rest.is_empty() {
        if field.ai_enable_row_generation != Some(allow) {
            field.ai_enable_row_generation = Some(allow);
            return true;
        }
        return false;
    }

    let Some(schema) = field.structure_schema.as_mut() else {
        return false;
    };
    apply_structure_row_generation_override_to_fields(schema, rest, allow)
}

fn reconcile_column_structure_overrides(
    column: &mut ColumnDefinition,
    path: &mut Vec<usize>,
    desired: &mut HashMap<Vec<usize>, bool>,
) -> bool {
    let mut changed = false;
    let key = path.clone();
    if let Some(&target) = desired.get(&key) {
        if column.ai_enable_row_generation != Some(target) {
            column.ai_enable_row_generation = Some(target);
            changed = true;
        }
        desired.remove(&key);
    } else if column.ai_enable_row_generation.is_some() {
        column.ai_enable_row_generation = None;
        changed = true;
    }

    if let Some(schema) = column.structure_schema.as_mut() {
        for (field_index, field) in schema.iter_mut().enumerate() {
            path.push(field_index);
            if reconcile_field_structure_overrides(field, path, desired) {
                changed = true;
            }
            path.pop();
        }
    }

    changed
}

fn reconcile_field_structure_overrides(
    field: &mut StructureFieldDefinition,
    path: &mut Vec<usize>,
    desired: &mut HashMap<Vec<usize>, bool>,
) -> bool {
    let mut changed = false;
    let key = path.clone();
    if let Some(&target) = desired.get(&key) {
        if field.ai_enable_row_generation != Some(target) {
            field.ai_enable_row_generation = Some(target);
            changed = true;
        }
        desired.remove(&key);
    } else if field.ai_enable_row_generation.is_some() {
        field.ai_enable_row_generation = None;
        changed = true;
    }

    if let Some(schema) = field.structure_schema.as_mut() {
        for (child_index, child_field) in schema.iter_mut().enumerate() {
            path.push(child_index);
            if reconcile_field_structure_overrides(child_field, path, desired) {
                changed = true;
            }
            path.pop();
        }
    }

    changed
}
