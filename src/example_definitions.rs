// src/example_definitions.rs
use crate::sheets::definitions::{
    default_ai_model_id, ColumnDataType, ColumnDefinition, SheetMetadata,
};

const EXAMPLE_ITEMS_SHEET_NAME: &str = "ExampleItems";
const EXAMPLE_ITEMS_FILENAME: &str = "ExampleItems.json";
const EXAMPLE_ITEMS_COLUMN_TYPES: &[ColumnDataType] = &[
    ColumnDataType::String,
    ColumnDataType::String,
    ColumnDataType::I64,
    ColumnDataType::F64,
    ColumnDataType::Bool,
];
const EXAMPLE_ITEMS_HEADERS: &[&str] = &[
    "ID",
    "Name",
    "Value (int)",
    "Cost (float)",
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
        .map(|(&header, &data_type)| ColumnDefinition::new_basic(header.to_string(), data_type))
        .collect();

    SheetMetadata {
        sheet_name: EXAMPLE_ITEMS_SHEET_NAME.to_string(),
        category: None,
        data_filename: EXAMPLE_ITEMS_FILENAME.to_string(),
        columns,
        ai_general_rule: None,
        ai_model_id: default_ai_model_id(),
        ai_temperature: None,
        requested_grounding_with_google_search: Default::default(),
        ai_enable_row_generation: false,
        ai_schema_groups: Vec::new(),
        ai_active_schema_group: None,
        random_picker: None,
        structure_parent: None,
        hidden: false,
    }
}

const SIMPLE_CONFIG_SHEET_NAME: &str = "SimpleConfig";
const SIMPLE_CONFIG_FILENAME: &str = "SimpleConfig.json";
const SIMPLE_CONFIG_COLUMN_TYPES: &[ColumnDataType] = &[
    ColumnDataType::String,
    ColumnDataType::String,
    ColumnDataType::I64,
];
const SIMPLE_CONFIG_HEADERS: &[&str] = &["Setting Key", "Setting Value", "Priority (int)"];

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
        .map(|(&header, &data_type)| ColumnDefinition::new_basic(header.to_string(), data_type))
        .collect();

    SheetMetadata {
        sheet_name: SIMPLE_CONFIG_SHEET_NAME.to_string(),
        category: None,
        data_filename: SIMPLE_CONFIG_FILENAME.to_string(),
        columns,
        ai_general_rule: None,
        ai_model_id: default_ai_model_id(),
        ai_temperature: None,
        requested_grounding_with_google_search: Default::default(),
        ai_enable_row_generation: false,
        ai_schema_groups: Vec::new(),
        ai_active_schema_group: None,
        random_picker: None,
        structure_parent: None,
        hidden: false,
    }
}
