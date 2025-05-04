// src/sheets/definitions.rs
use serde::{Deserialize, Serialize};

/// Defines the type of data expected in a specific column of a sheet grid.
/// Used for parsing, validation, and UI generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)] // Added Hash for BTreeMap key in UI state
pub enum ColumnDataType {
    String, OptionString, Bool, OptionBool, U8, OptionU8, U16, OptionU16,
    U32, OptionU32, U64, OptionU64, I8, OptionI8, I16, OptionI16,
    I32, OptionI32, I64, OptionI64, F32, OptionF32, F64, OptionF64,
}

/// Holds the metadata defining the structure of a specific sheet.
/// Instances are typically defined statically (e.g., in `example_definitions.rs`).
#[derive(Debug, Clone)]
pub struct SheetMetadata {
    pub sheet_name: &'static str,
    pub data_filename: &'static str,
    pub column_headers: &'static [&'static str],
    pub column_types: &'static [ColumnDataType],
}

/// Represents the actual grid data (usually loaded from JSON) along with its metadata.
/// Stored within the SheetRegistry resource.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SheetGridData {
    #[serde(skip)] // Don't serialize/deserialize metadata, it's linked at runtime
    pub metadata: Option<SheetMetadata>,
    // The actual data grid, loaded from `data_filename` specified in metadata
    pub grid: Vec<Vec<String>>,
}