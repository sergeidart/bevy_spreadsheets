// src/sheets/definitions.rs
use bevy::prelude::warn; // Import warn for default function logging
use serde::{Deserialize, Serialize};

/// Defines the type of data expected in a specific column of a sheet grid.
/// Used for parsing, validation, and UI generation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default,
)] // Added Default
pub enum ColumnDataType {
    #[default] // Specify String as the default type
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

/// Defines the validation rule for a column.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ColumnValidator {
    /// Allows any value compatible with the basic ColumnDataType.
    Basic(ColumnDataType),
    /// Restricts values to those present in another column.
    Linked {
        target_sheet_name: String,
        target_column_index: usize,
        // Consider adding target_sheet_category: Option<String> in future?
    },
    // Future validators (e.g., Regex, Range) could go here
}

// --- NEW: Column Definition Struct ---
/// Holds all metadata pertaining to a single column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    pub header: String, // Column name/title
    pub validator: Option<ColumnValidator>, // Validation rule (includes basic type)
    pub data_type: ColumnDataType, // Derived basic type (for UI/parsing)
    pub filter: Option<String>, // Current filter text for this column
    #[serde(default)] // For backward compatibility
    pub ai_context: Option<String>, // NEW: Context hint for AI
    // --- ADDED ---
    #[serde(default)] // For backward compatibility with older files
    pub width: Option<f32>, // Column width persistence
    // -------------
}

impl ColumnDefinition {
    /// Creates a default column definition with a basic validator.
    pub fn new_basic(header: String, data_type: ColumnDataType) -> Self {
        ColumnDefinition {
            header,
            validator: Some(ColumnValidator::Basic(data_type)),
            data_type, // Store the derived type
            filter: None,
            ai_context: None,
            width: None, // <-- Initialize width to None
        }
    }

    /// Ensures the internal `data_type` field is consistent with the `validator`.
    /// Call this after modifying the validator. Returns true if changed.
    fn ensure_type_consistency(&mut self) -> bool {
        let expected_type = match &self.validator {
            Some(ColumnValidator::Basic(t)) => *t,
            Some(ColumnValidator::Linked { .. }) => ColumnDataType::String, // Linked uses String UI
            None => ColumnDataType::String, // Default if no validator
        };
        if self.data_type != expected_type {
            self.data_type = expected_type;
            true
        } else {
            false
        }
    }
}

/// Holds the metadata defining the structure and rules of a specific sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetMetadata {
    pub sheet_name: String,
    #[serde(default)]
    pub category: Option<String>,
    pub data_filename: String, // Relative to category folder

    // --- Refactored Column Data ---
    #[serde(default)] // Handle loading older metadata without columns field
    pub columns: Vec<ColumnDefinition>,

    // --- NEW: AI Rule ---
    #[serde(default)] // For backward compatibility
    pub ai_general_rule: Option<String>,
}

impl SheetMetadata {
    /// Creates generic metadata for a new sheet.
    pub fn create_generic(
        name: String,
        filename: String, // Should be just the filename (e.g., "Sheet1.json")
        num_cols: usize,
        category: Option<String>,
    ) -> Self {
        let columns = (0..num_cols)
            .map(|i| {
                // new_basic now initializes width to None implicitly
                ColumnDefinition::new_basic(
                    format!("Column {}", i + 1),
                    ColumnDataType::String, // Default type
                )
            })
            .collect();

        SheetMetadata {
            sheet_name: name,
            category,
            data_filename: filename,
            columns,
            ai_general_rule: None,
        }
    }

    /// Ensures column definitions are consistent (e.g., validator matches type).
    /// This is less about length syncing now, more about internal consistency per column.
    pub fn ensure_column_consistency(&mut self) -> bool {
        let mut changed = false;
        for column in self.columns.iter_mut() {
            // If validator is None, initialize it based on data_type (important for loading old data)
            if column.validator.is_none() {
                warn!(
                    "Initializing missing validator for column '{}' in sheet '{}' based on type {:?}.",
                    column.header, self.sheet_name, column.data_type
                );
                column.validator = Some(ColumnValidator::Basic(column.data_type));
                changed = true;
            }
            // Ensure data_type matches validator
            if column.ensure_type_consistency() {
                warn!(
                    "Corrected data type inconsistency for column '{}' in sheet '{}'.",
                    column.header, self.sheet_name
                );
                changed = true;
            }
            // NOTE: No width consistency check needed here unless rules are added
        }
        changed
    }

    // Helper to get just headers (useful for UI)
    pub fn get_headers(&self) -> Vec<String> {
        self.columns.iter().map(|c| c.header.clone()).collect()
    }

    // Helper to get just filters (useful for UI)
    pub fn get_filters(&self) -> Vec<Option<String>> {
        self.columns.iter().map(|c| c.filter.clone()).collect()
    }
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