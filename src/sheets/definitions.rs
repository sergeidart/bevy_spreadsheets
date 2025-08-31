// src/sheets/definitions.rs
use bevy::prelude::{warn, info};
use serde::{Deserialize, Serialize, de::{self, Deserializer}};
use std::fmt;
use std::collections::HashMap;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default,
)]
pub enum ColumnDataType {
    #[default]
    String,
    OptionString,
    Bool,
    OptionBool,
    U8,
    OptionU8,
    U16,
    OptionU16,
    U32,
    OptionU32,
    U64,
    OptionU64,
    I8,
    OptionI8,
    I16,
    OptionI16,
    I32,
    OptionI32,
    I64,
    OptionI64,
    F32,
    OptionF32,
    F64,
    OptionF64,
}

impl fmt::Display for ColumnDataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ColumnValidator {
    Basic(ColumnDataType),
    Linked { target_sheet_name: String, target_column_index: usize },
    // Schema B: Structure validator (schema embedded elsewhere, no indices here)
    Structure,
}

// Custom Deserialize for backward compatibility (accept legacy Structure { source_column_indices: [...] })
impl<'de> Deserialize<'de> for ColumnValidator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(s) = value.as_str() {
            return match s {
                "Structure" => Ok(ColumnValidator::Structure),
                other => parse_column_data_type(other)
                    .map(ColumnValidator::Basic)
                    .ok_or_else(|| de::Error::custom(format!("Unknown ColumnValidator string '{}'.", other))),
            };
        }
        if let Some(obj) = value.as_object() {
            if obj.len() == 1 {
                let (tag, inner) = obj.iter().next().unwrap();
                match tag.as_str() {
                    "Basic" => {
                        if inner.is_string() {
                            if let Some(dt) = inner.as_str().and_then(parse_column_data_type) { return Ok(ColumnValidator::Basic(dt)); }
                        }
                        let dt: ColumnDataType = serde_json::from_value(inner.clone())
                            .map_err(|e| de::Error::custom(format!("Invalid Basic validator payload: {}", e)))?;
                        return Ok(ColumnValidator::Basic(dt));
                    }
                    "Linked" => {
                        #[derive(Deserialize)]
                        struct LinkedHelper { target_sheet_name: String, target_column_index: usize }
                        let helper: LinkedHelper = serde_json::from_value(inner.clone())
                            .map_err(|e| de::Error::custom(format!("Invalid Linked validator payload: {}", e)))?;
                        return Ok(ColumnValidator::Linked { target_sheet_name: helper.target_sheet_name, target_column_index: helper.target_column_index });
                    }
                    "Structure" => { return Ok(ColumnValidator::Structure); }
                    _ => {}
                }
            }
        }
        Err(de::Error::custom("Unrecognized ColumnValidator representation"))
    }
}

impl fmt::Display for ColumnValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColumnValidator::Basic(data_type) => write!(f, "Basic({})", data_type),
            ColumnValidator::Linked { target_sheet_name, target_column_index } => {
                write!(f, "Linked{{target_sheet_name: \"{}\", target_column_index: {}}}", target_sheet_name, target_column_index)
            }
            ColumnValidator::Structure => write!(f, "Structure"),
        }
    }
}

// --- NEW: Random Picker configuration types ---
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RandomPickerMode {
    Simple,
    Complex,
}

impl Default for RandomPickerMode {
    fn default() -> Self { RandomPickerMode::Simple }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RandomPickerSettings {
    #[serde(default)]
    pub mode: RandomPickerMode,
    /// Used when mode == Simple
    #[serde(default)]
    pub simple_result_col_index: usize,
    /// Used when mode == Complex
    #[serde(default)]
    pub complex_result_col_index: usize,
    #[serde(default)]
    pub weight_col_index: Option<usize>,
    #[serde(default)]
    pub second_weight_col_index: Option<usize>,
}

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
    // Legacy width accepted but not serialized
    #[serde(default, skip_serializing)]
    pub width: Option<f32>,
    #[serde(default)]
    pub structure_schema: Option<Vec<StructureFieldDefinition>>,
}

impl From<&ColumnDefinition> for StructureFieldDefinition {
    fn from(c: &ColumnDefinition) -> Self {
        StructureFieldDefinition {
            header: c.header.clone(),
            validator: c.validator.clone(),
            data_type: c.data_type,
            filter: c.filter.clone(),
            ai_context: c.ai_context.clone(),
            width: None,
            structure_schema: c.structure_schema.clone(),
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

// Default function for ai_model_id
pub fn default_ai_model_id() -> String {
    "gemini-2.5-pro-preview-06-05".to_string() // Default model
}

// Default functions for existing AI parameters
pub fn default_temperature() -> Option<f32> { Some(0.9) }
pub fn default_top_k() -> Option<i32> { Some(1) }
pub fn default_top_p() -> Option<f32> { Some(1.0) }

// --- CORRECTED: Definition of the default function for grounding ---
/// Default function for `requested_grounding_with_Google Search` field in `SheetMetadata`.
pub fn default_grounding_with_google_search() -> Option<bool> {
    Some(false) // Default to false (or true if you prefer grounding by default)
}
// --- END CORRECTION ---

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
    #[serde(default = "default_temperature")]
    pub ai_temperature: Option<f32>,
    #[serde(default = "default_top_k")]
    pub ai_top_k: Option<i32>,
    #[serde(default = "default_top_p")]
    pub ai_top_p: Option<f32>,

    // Use the defined default function for serde
    #[serde(default = "default_grounding_with_google_search")]
    pub requested_grounding_with_google_search: Option<bool>,

    // NEW: Optional per-sheet Random Picker settings
    #[serde(default)]
    pub random_picker: Option<RandomPickerSettings>,
    // If this is a virtual structure sheet, link back to parent sheet & column
    #[serde(default)]
    pub structure_parent: Option<StructureParentLink>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureParentLink {
    pub parent_category: Option<String>,
    pub parent_sheet: String,
    pub parent_column_index: usize,
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
            random_picker: Option<RandomPickerSettings>,
            #[serde(default)]
            structure_parent: Option<StructureParentLink>,
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
                ai_temperature: cur.ai_temperature,
                ai_top_k: cur.ai_top_k,
                ai_top_p: cur.ai_top_p,
                requested_grounding_with_google_search: cur.requested_grounding_with_google_search,
                random_picker: cur.random_picker,
                structure_parent: cur.structure_parent,
            };
            // Migrate legacy map + inline legacy schemas
            let mut legacy_map = cur.structures_meta;
            for (idx, col) in meta.columns.iter_mut().enumerate() {
                if let Some(inline_schema) = col.structure_schema.as_ref() {
                    if col.structure_column_order.is_none() { col.structure_column_order = Some((0..inline_schema.len()).collect()); }
                }
                if let Some(entry) = legacy_map.remove(&format!("column_{}", idx)) {
                    col.structure_schema = Some(entry.columns.clone());
                    if col.structure_column_order.is_none() { col.structure_column_order = Some(entry.column_order.clone()); }
                    if let Some(k) = entry.key_parent_column_index { col.structure_key_parent_column_index = Some(k); }
                    if !entry.ancestor_key_parent_column_indices.is_empty() { col.structure_ancestor_key_parent_column_indices = Some(entry.ancestor_key_parent_column_indices.clone()); }
                }
                col.width = None; // discard legacy width
            }
            meta.ensure_column_consistency();
            return Ok(meta);
        }

        // Fallback legacy
        let legacy: LegacySheetMetadataHelper = LegacySheetMetadataHelper::deserialize(value.clone())
            .map_err(|e| de::Error::custom(format!("Failed to parse SheetMetadata (current or legacy): {}", e)))?;

        let headers = legacy.column_headers.unwrap_or_default();
        let types = legacy.column_types.unwrap_or_default();
        let filters = legacy.column_filters.unwrap_or_default();
        let validators = legacy.column_validators.unwrap_or_default();

        let len = headers.len();
        let mut columns: Vec<ColumnDefinition> = Vec::with_capacity(len);
        for i in 0..len {
            let header = headers.get(i).cloned().unwrap_or_else(|| format!("Column {}", i+1));
            let type_str = types.get(i).cloned().unwrap_or_else(|| "String".to_string());
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
            data_filename: legacy.data_filename.unwrap_or_else(|| "unknown.json".to_string()),
            columns,
            ai_general_rule: legacy.ai_general_rule,
            ai_model_id: legacy.ai_model_id.unwrap_or_else(|| default_ai_model_id()),
            ai_temperature: legacy.ai_temperature.or_else(default_temperature),
            ai_top_k: legacy.ai_top_k.or_else(default_top_k),
            ai_top_p: legacy.ai_top_p.or_else(default_top_p),
            requested_grounding_with_google_search: legacy.requested_grounding_with_google_search.or_else(default_grounding_with_google_search),
            random_picker: legacy.random_picker,
            structure_parent: None,
        };
        meta.ensure_column_consistency();
        info!("Loaded legacy metadata for sheet '{}': {} columns", meta.sheet_name, meta.columns.len());
        Ok(meta)
    }
}

fn parse_column_data_type(s: &str) -> Option<ColumnDataType> {
    let norm = s.trim();
    // Accept exact Debug variants or lowercase
    match norm {
        "String" | "string" => Some(ColumnDataType::String),
        "OptionString" | "optionstring" | "Option<String>" => Some(ColumnDataType::OptionString),
        "Bool" | "bool" => Some(ColumnDataType::Bool),
        "OptionBool" | "optionbool" | "Option<Bool>" => Some(ColumnDataType::OptionBool),
        "U8" | "u8" => Some(ColumnDataType::U8),
        "OptionU8" | "optionu8" => Some(ColumnDataType::OptionU8),
        "U16" | "u16" => Some(ColumnDataType::U16),
        "OptionU16" | "optionu16" => Some(ColumnDataType::OptionU16),
        "U32" | "u32" => Some(ColumnDataType::U32),
        "OptionU32" | "optionu32" => Some(ColumnDataType::OptionU32),
        "U64" | "u64" => Some(ColumnDataType::U64),
        "OptionU64" | "optionu64" => Some(ColumnDataType::OptionU64),
        "I8" | "i8" => Some(ColumnDataType::I8),
        "OptionI8" | "optioni8" => Some(ColumnDataType::OptionI8),
        "I16" | "i16" => Some(ColumnDataType::I16),
        "OptionI16" | "optioni16" => Some(ColumnDataType::OptionI16),
        "I32" | "i32" => Some(ColumnDataType::I32),
        "OptionI32" | "optioni32" => Some(ColumnDataType::OptionI32),
        "I64" | "i64" => Some(ColumnDataType::I64),
        "OptionI64" | "optioni64" => Some(ColumnDataType::OptionI64),
        "F32" | "f32" => Some(ColumnDataType::F32),
        "OptionF32" | "optionf32" => Some(ColumnDataType::OptionF32),
        "F64" | "f64" => Some(ColumnDataType::F64),
        "OptionF64" | "optionf64" => Some(ColumnDataType::OptionF64),
        _ => None,
    }
}

fn parse_legacy_validator(raw: &str, fallback_type: ColumnDataType) -> Option<ColumnValidator> {
    let trimmed = raw.trim();
    if trimmed.is_empty() { return None; }
    if let Some(stripped) = trimmed.strip_prefix("Basic(").and_then(|r| r.strip_suffix(')')) {
        if let Some(dt) = parse_column_data_type(stripped) { return Some(ColumnValidator::Basic(dt)); }
        return Some(ColumnValidator::Basic(fallback_type));
    }
    if let Some(dt) = parse_column_data_type(trimmed) { return Some(ColumnValidator::Basic(dt)); }
    Some(ColumnValidator::Basic(fallback_type))
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
                ColumnDefinition::new_basic(
                    format!("Column {}", i + 1),
                    ColumnDataType::String,
                )
            })
            .collect();

        SheetMetadata {
            sheet_name: name,
            category,
            data_filename: filename,
            columns,
            ai_general_rule: None,
            ai_model_id: default_ai_model_id(),
            ai_temperature: default_temperature(),
            ai_top_k: default_top_k(),
            ai_top_p: default_top_p(),
            // Call the defined function for initialization
            requested_grounding_with_google_search: default_grounding_with_google_search(),
            random_picker: None,
            structure_parent: None,
        }
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
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SheetGridData {
    #[serde(skip)]
    pub metadata: Option<SheetMetadata>,
    pub grid: Vec<Vec<String>>,
}