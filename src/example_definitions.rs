// src/example_definitions.rs
use crate::sheets::{ColumnDataType, SheetMetadata};

// --- Example Struct (Optional but good practice) ---
#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct ExampleItem {
    pub id: String,
    pub name: Option<String>,
    pub value: i32,
    pub cost: Option<f32>,
    pub enabled: bool,
}

// --- Metadata for the Example Sheet ---
const EXAMPLE_ITEMS_COLUMN_TYPES: &[ColumnDataType] = &[
    ColumnDataType::String,     // id
    ColumnDataType::OptionString, // name
    ColumnDataType::I32,        // value
    ColumnDataType::OptionF32,  // cost
    ColumnDataType::Bool,       // enabled
];
const EXAMPLE_ITEMS_HEADERS: &[&str] = &[
    "ID", "Optional Name", "Value (i32)", "Optional Cost (f32)", "Enabled (bool)",
];
pub const EXAMPLE_ITEMS_METADATA: SheetMetadata = SheetMetadata {
    sheet_name: "ExampleItems",
    data_filename: "ExampleItems.json", // File it will save/load
    column_headers: EXAMPLE_ITEMS_HEADERS,
    column_types: EXAMPLE_ITEMS_COLUMN_TYPES,
};
// Basic check
const _: () = assert!(
    EXAMPLE_ITEMS_HEADERS.len() == EXAMPLE_ITEMS_COLUMN_TYPES.len(),
    "ExampleItems headers/types length mismatch!"
);

// --- Metadata for Another Example Sheet ---
const SIMPLE_CONFIG_COLUMN_TYPES: &[ColumnDataType] = &[
    ColumnDataType::String,     // Key
    ColumnDataType::String,     // Value
    ColumnDataType::OptionU16, // Priority
];
const SIMPLE_CONFIG_HEADERS: &[&str] = &["Setting Key", "Setting Value", "Priority (Optional u16)"];
pub const SIMPLE_CONFIG_METADATA: SheetMetadata = SheetMetadata {
    sheet_name: "SimpleConfig",
    data_filename: "SimpleConfig.json",
    column_headers: SIMPLE_CONFIG_HEADERS,
    column_types: SIMPLE_CONFIG_COLUMN_TYPES,
};
const _: () = assert!(
    SIMPLE_CONFIG_HEADERS.len() == SIMPLE_CONFIG_COLUMN_TYPES.len(),
    "SimpleConfig headers/types length mismatch!"
);
