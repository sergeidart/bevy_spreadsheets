// src/example_definitions.rs
use crate::sheets::definitions::{
    ColumnDataType, ColumnDefinition, SheetMetadata,
    // --- MODIFIED: Import new default function for model ID ---
    default_ai_model_id,
    // --- END MODIFIED ---
    default_temperature, default_top_k, default_top_p, // Keep these
};

#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct ExampleItem {
    pub id: String,
    pub name: Option<String>,
    pub value: i32,
    pub cost: Option<f32>,
    pub enabled: bool,
}

const EXAMPLE_ITEMS_SHEET_NAME: &str = "ExampleItems";
const EXAMPLE_ITEMS_FILENAME: &str = "ExampleItems.json";
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

    let columns: Vec<ColumnDefinition> = EXAMPLE_ITEMS_HEADERS
        .iter()
        .zip(EXAMPLE_ITEMS_COLUMN_TYPES.iter())
        .map(|(&header, &data_type)| {
            ColumnDefinition::new_basic(header.to_string(), data_type)
        })
        .collect();

    SheetMetadata {
        sheet_name: EXAMPLE_ITEMS_SHEET_NAME.to_string(),
        category: None,
        data_filename: EXAMPLE_ITEMS_FILENAME.to_string(),
        columns,
        ai_general_rule: None,
        // --- MODIFIED: Initialize new AI model ID field and keep existing params ---
        ai_model_id: default_ai_model_id(),
        ai_temperature: default_temperature(),
        ai_top_k: default_top_k(),
        ai_top_p: default_top_p(),
        requested_grounding_with_google_search: Default::default(),
        // --- END MODIFIED ---
    }
}

const SIMPLE_CONFIG_SHEET_NAME: &str = "SimpleConfig";
const SIMPLE_CONFIG_FILENAME: &str = "SimpleConfig.json";
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

    let columns: Vec<ColumnDefinition> = SIMPLE_CONFIG_HEADERS
        .iter()
        .zip(SIMPLE_CONFIG_COLUMN_TYPES.iter())
        .map(|(&header, &data_type)| {
            ColumnDefinition::new_basic(header.to_string(), data_type)
        })
        .collect();

    SheetMetadata {
        sheet_name: SIMPLE_CONFIG_SHEET_NAME.to_string(),
        category: None,
        data_filename: SIMPLE_CONFIG_FILENAME.to_string(),
        columns,
        ai_general_rule: None,
        // --- MODIFIED: Initialize new AI model ID field and keep existing params ---
        ai_model_id: default_ai_model_id(),
        ai_temperature: default_temperature(),
        ai_top_k: default_top_k(),
        ai_top_p: default_top_p(),
        requested_grounding_with_google_search: Default::default(),
        // --- END MODIFIED ---
    }
}