// src/example_definitions.rs
use crate::sheets::{ColumnDataType, SheetMetadata};

// --- Example Struct (Remains the same) ---
#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct ExampleItem {
    pub id: String,
    pub name: Option<String>,
    pub value: i32,
    pub cost: Option<f32>,
    pub enabled: bool,
}

// --- Metadata for the Example Sheet ---
// Keep constants for raw data
const EXAMPLE_ITEMS_SHEET_NAME: &str = "ExampleItems";
const EXAMPLE_ITEMS_FILENAME: &str = "ExampleItems.json";
const EXAMPLE_ITEMS_COLUMN_TYPES: &[ColumnDataType] = &[
    ColumnDataType::String, ColumnDataType::OptionString, ColumnDataType::I32,
    ColumnDataType::OptionF32, ColumnDataType::Bool,
];
const EXAMPLE_ITEMS_HEADERS: &[&str] = &[
    "ID", "Optional Name", "Value (i32)", "Optional Cost (f32)", "Enabled (bool)",
];

// Function to create the owned metadata instance
pub fn create_example_items_metadata() -> SheetMetadata {
    let num_cols = EXAMPLE_ITEMS_HEADERS.len();
    assert_eq!(num_cols, EXAMPLE_ITEMS_COLUMN_TYPES.len(), "ExampleItems headers/types length mismatch!");
    SheetMetadata {
        sheet_name: EXAMPLE_ITEMS_SHEET_NAME.to_string(),
        data_filename: EXAMPLE_ITEMS_FILENAME.to_string(),
        column_headers: EXAMPLE_ITEMS_HEADERS.iter().map(|&s| s.to_string()).collect(),
        column_types: EXAMPLE_ITEMS_COLUMN_TYPES.to_vec(),
        column_filters: vec![None; num_cols], // Initialize filters
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
    SheetMetadata {
        sheet_name: SIMPLE_CONFIG_SHEET_NAME.to_string(),
        data_filename: SIMPLE_CONFIG_FILENAME.to_string(),
        column_headers: SIMPLE_CONFIG_HEADERS.iter().map(|&s| s.to_string()).collect(),
        column_types: SIMPLE_CONFIG_COLUMN_TYPES.to_vec(),
        column_filters: vec![None; num_cols], // Initialize filters
    }
}

// --- Constant for convenience (optional) ---
// Note: Creating these at compile time requires `const fn` features or lazy_static.
// It's often simpler to call the creation functions in your setup code.
// Example:
// pub static ref EXAMPLE_ITEMS_METADATA: SheetMetadata = create_example_items_metadata(); // Requires lazy_static