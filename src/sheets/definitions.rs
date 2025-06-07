// src/sheets/definitions.rs
use bevy::prelude::warn;
use serde::{Deserialize, Serialize};
use std::fmt;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ColumnValidator {
    Basic(ColumnDataType),
    Linked {
        target_sheet_name: String,
        target_column_index: usize,
    },
}

impl fmt::Display for ColumnValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColumnValidator::Basic(data_type) => write!(f, "Basic({})", data_type),
            ColumnValidator::Linked { target_sheet_name, target_column_index } => {
                write!(f, "Linked{{target_sheet_name: \"{}\", target_column_index: {}}}", target_sheet_name, target_column_index)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    pub header: String,
    pub validator: Option<ColumnValidator>,
    pub data_type: ColumnDataType,
    pub filter: Option<String>,
    #[serde(default)]
    pub ai_context: Option<String>,
    #[serde(default)]
    pub width: Option<f32>,
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
        }
    }

    pub fn ensure_type_consistency(&mut self) -> bool {
        let expected_type = match &self.validator {
            Some(ColumnValidator::Basic(t)) => *t,
            Some(ColumnValidator::Linked { .. }) => ColumnDataType::String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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