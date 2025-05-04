// src/sheets/definitions.rs
use serde::{Deserialize, Serialize};

/// Defines the type of data expected in a specific column of a sheet grid.
/// Used for parsing, validation, and UI generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColumnDataType {
    String, OptionString, Bool, OptionBool, U8, OptionU8, U16, OptionU16,
    U32, OptionU32, U64, OptionU64, I8, OptionI8, I16, OptionI16,
    I32, OptionI32, I64, OptionI64, F32, OptionF32, F64, OptionF64,
}

/// Holds the metadata defining the structure of a specific sheet.
/// Now uses owned types for dynamic creation.
#[derive(Debug, Clone, Serialize, Deserialize)] // Can now be serialized if needed
pub struct SheetMetadata {
    pub sheet_name: String,       // Owned String
    pub data_filename: String,    // Owned String
    pub column_headers: Vec<String>, // Owned Vec<String>
    pub column_types: Vec<ColumnDataType>, // Owned Vec<ColumnDataType>
}

/// Represents the actual grid data along with its metadata.
/// Stored within the SheetRegistry resource.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SheetGridData {
    #[serde(skip)] // Still skip metadata during grid data serialization
    pub metadata: Option<SheetMetadata>, // Holds the owned SheetMetadata
    // The actual data grid, loaded from `data_filename` specified in metadata
    pub grid: Vec<Vec<String>>,
}

// --- Helper for creating generic metadata ---
impl SheetMetadata {
    pub fn create_generic(name: String, filename: String, num_cols: usize) -> Self {
        SheetMetadata {
            sheet_name: name,
            data_filename: filename,
            column_headers: (0..num_cols).map(|i| format!("Column {}", i + 1)).collect(),
            // Default all columns to String type for generic uploads
            column_types: vec![ColumnDataType::String; num_cols],
        }
    }
}