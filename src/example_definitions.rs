// src/example_definitions.rs
use crate::sheets::definitions::{
    ColumnDataType, ColumnDefinition, ColumnValidator, SheetMetadata, // Use ColumnDefinition
};

// --- Example Struct ---
// (This struct remains unchanged as it's just for conceptual mapping)
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
const EXAMPLE_ITEMS_FILENAME: &str = "ExampleItems.json"; // Filename only
const EXAMPLE_ITEMS_COLUMN_TYPES: &[ColumnDataType] = &[
    ColumnDataType::String,
    ColumnDataType::OptionString,
    ColumnDataType::I32,
    ColumnDataType::OptionF32,
    ColumnDataType::Bool,
];
const EXAMPLE_ITEMS_HEADERS: &[&str] = &[
    "ID",
    "Optional Name",
    "Value (i32)",
    "Optional Cost (f32)",
    "Enabled (bool)",
];

pub fn create_example_items_metadata() -> SheetMetadata {
    let num_cols = EXAMPLE_ITEMS_HEADERS.len();
    assert_eq!(
        num_cols,
        EXAMPLE_ITEMS_COLUMN_TYPES.len(),
        "ExampleItems headers/types length mismatch!"
    );

    // --- CORRECTED: Create Vec<ColumnDefinition> ---
    let columns: Vec<ColumnDefinition> = EXAMPLE_ITEMS_HEADERS
        .iter()
        .zip(EXAMPLE_ITEMS_COLUMN_TYPES.iter())
        .map(|(&header, &data_type)| {
            // Create ColumnDefinition using the helper
            ColumnDefinition::new_basic(header.to_string(), data_type)
        })
        .collect();

    SheetMetadata {
        sheet_name: EXAMPLE_ITEMS_SHEET_NAME.to_string(),
        category: None, // Default sheets are in the root category
        data_filename: EXAMPLE_ITEMS_FILENAME.to_string(), // Filename only
        columns, // Use the new 'columns' field
        ai_general_rule: None, // Initialize new AI field
    }
}

// --- Metadata for Another Example Sheet ---
const SIMPLE_CONFIG_SHEET_NAME: &str = "SimpleConfig";
const SIMPLE_CONFIG_FILENAME: &str = "SimpleConfig.json"; // Filename only
const SIMPLE_CONFIG_COLUMN_TYPES: &[ColumnDataType] = &[
    ColumnDataType::String,
    ColumnDataType::String,
    ColumnDataType::OptionU16,
];
const SIMPLE_CONFIG_HEADERS: &[&str] =
    &["Setting Key", "Setting Value", "Priority (Optional u16)"];

pub fn create_simple_config_metadata() -> SheetMetadata {
    let num_cols = SIMPLE_CONFIG_HEADERS.len();
    assert_eq!(
        num_cols,
        SIMPLE_CONFIG_COLUMN_TYPES.len(),
        "SimpleConfig headers/types length mismatch!"
    );

    // --- CORRECTED: Create Vec<ColumnDefinition> ---
    let columns: Vec<ColumnDefinition> = SIMPLE_CONFIG_HEADERS
        .iter()
        .zip(SIMPLE_CONFIG_COLUMN_TYPES.iter())
        .map(|(&header, &data_type)| {
            ColumnDefinition::new_basic(header.to_string(), data_type)
        })
        .collect();

    SheetMetadata {
        sheet_name: SIMPLE_CONFIG_SHEET_NAME.to_string(),
        category: None, // Default sheets are in the root category
        data_filename: SIMPLE_CONFIG_FILENAME.to_string(), // Filename only
        columns, // Use the new 'columns' field
        ai_general_rule: None, // Initialize new AI field
    }
}