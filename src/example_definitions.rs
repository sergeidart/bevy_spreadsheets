// src/example_definitions.rs
use crate::sheets::{
    definitions::{ColumnDataType, SheetMetadata, ColumnValidator}, // Corrected import path
};

// --- Example Struct ---
#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct ExampleItem {
    pub id: String,
    pub name: Option<String>,
    pub value: i32,
    pub cost: Option<f32>,
    pub enabled: bool,
}

// --- Metadata for the Example Sheet ---
const EXAMPLE_ITEMS_SHEET_NAME: &str = "ExampleItems";
const EXAMPLE_ITEMS_FILENAME: &str = "ExampleItems.json";
const EXAMPLE_ITEMS_COLUMN_TYPES: &[ColumnDataType] = &[
    ColumnDataType::String, ColumnDataType::OptionString, ColumnDataType::I32,
    ColumnDataType::OptionF32, ColumnDataType::Bool,
];
const EXAMPLE_ITEMS_HEADERS: &[&str] = &[
    "ID", "Optional Name", "Value (i32)", "Optional Cost (f32)", "Enabled (bool)",
];

pub fn create_example_items_metadata() -> SheetMetadata {
    let num_cols = EXAMPLE_ITEMS_HEADERS.len();
    assert_eq!(num_cols, EXAMPLE_ITEMS_COLUMN_TYPES.len(), "ExampleItems headers/types length mismatch!");
    let types = EXAMPLE_ITEMS_COLUMN_TYPES.to_vec();
    SheetMetadata {
        sheet_name: EXAMPLE_ITEMS_SHEET_NAME.to_string(),
        data_filename: EXAMPLE_ITEMS_FILENAME.to_string(),
        column_headers: EXAMPLE_ITEMS_HEADERS.iter().map(|&s| s.to_string()).collect(),
        column_validators: types.iter().map(|&t| Some(ColumnValidator::Basic(t))).collect(),
        column_types: types,
        column_filters: vec![None; num_cols],
    }
}

// --- Metadata for Another Example Sheet ---
const SIMPLE_CONFIG_SHEET_NAME: &str = "SimpleConfig";
const SIMPLE_CONFIG_FILENAME: &str = "SimpleConfig.json";
const SIMPLE_CONFIG_COLUMN_TYPES: &[ColumnDataType] = &[
    ColumnDataType::String, ColumnDataType::String, ColumnDataType::OptionU16,
];
const SIMPLE_CONFIG_HEADERS: &[&str] = &["Setting Key", "Setting Value", "Priority (Optional u16)"];

pub fn create_simple_config_metadata() -> SheetMetadata {
    let num_cols = SIMPLE_CONFIG_HEADERS.len();
    assert_eq!(num_cols, SIMPLE_CONFIG_COLUMN_TYPES.len(), "SimpleConfig headers/types length mismatch!");
    let types = SIMPLE_CONFIG_COLUMN_TYPES.to_vec();
    SheetMetadata {
        sheet_name: SIMPLE_CONFIG_SHEET_NAME.to_string(),
        data_filename: SIMPLE_CONFIG_FILENAME.to_string(),
        column_headers: SIMPLE_CONFIG_HEADERS.iter().map(|&s| s.to_string()).collect(),
        column_validators: types.iter().map(|&t| Some(ColumnValidator::Basic(t))).collect(),
        column_types: types,
        column_filters: vec![None; num_cols],
    }
}