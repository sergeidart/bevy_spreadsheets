// src/sheets/definitions.rs
use serde::{Deserialize, Serialize};
use bevy::prelude::warn; // Import warn for default function logging

/// Defines the type of data expected in a specific column of a sheet grid.
/// Used for parsing, validation, and UI generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)] // Added Default
pub enum ColumnDataType {
    #[default] // Specify String as the default type
    String,
    OptionString, Bool, OptionBool, U8, OptionU8, U16, OptionU16,
    U32, OptionU32, U64, OptionU64, I8, OptionI8, I16, OptionI16,
    I32, OptionI32, I64, OptionI64, F32, OptionF32, F64, OptionF64,
}

// --- NEW: Validator Definition ---
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ColumnValidator {
    /// Allows any value compatible with the basic ColumnDataType.
    Basic(ColumnDataType),
    /// Restricts values to those present in another column.
    Linked {
        target_sheet_name: String,
        target_column_index: usize,
        // Add flags? e.g., allow_empty, case_sensitive? For now, keep simple.
    },
    // Future validators (e.g., Regex, Range) could go here
}


/// Holds the metadata defining the structure of a specific sheet.
/// Now uses owned types for dynamic creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetMetadata {
    pub sheet_name: String,       // Owned String
    #[serde(default)] // Handle loading older metadata without category
    pub category: Option<String>, // <<< --- ADDED CATEGORY FIELD --- >>>
    pub data_filename: String,    // Owned String (relative to category folder)
    pub column_headers: Vec<String>, // Owned Vec<String>
    // Consider if `column_types` is still the primary source of type info
    // or if it should be derived purely from `column_validators`.
    // For now, keep both and ensure they are consistent.
    pub column_types: Vec<ColumnDataType>, // Owned Vec<ColumnDataType>
    #[serde(default)] // To handle loading older metadata without filters
    pub column_filters: Vec<Option<String>>, // Existing field for filters
    // --- NEW: Field for Validators ---
    #[serde(default = "default_column_validators")] // Default for loading old data
    pub column_validators: Vec<Option<ColumnValidator>>,
}

/// Function to provide a default value for `column_validators` during deserialization.
/// Crucially, this returns an empty Vec. The actual initialization logic
/// (e.g., basing validators on types) needs to happen *after* loading,
/// likely during the validation/correction phase (e.g., in startup_load).
fn default_column_validators() -> Vec<Option<ColumnValidator>> {
    // This function is only called by serde when the field is MISSING in the JSON.
    // It does NOT initialize the content if the field exists but is empty/null.
    // Consistency checks after loading handle initialization/resizing.
    Vec::new()
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
    // Updated to initialize validators based on default types and accept category
    pub fn create_generic(
        name: String,
        filename: String, // Should be just the filename (e.g., "Sheet1.json")
        num_cols: usize,
        category: Option<String>, // <<< --- ADDED CATEGORY PARAMETER --- >>>
    ) -> Self {
        let default_types = vec![ColumnDataType::String; num_cols];
        SheetMetadata {
            sheet_name: name,
            category, // <<< --- ASSIGN CATEGORY --- >>>
            data_filename: filename,
            column_headers: (0..num_cols).map(|i| format!("Column {}", i + 1)).collect(),
            // Initialize validators based on the default types
            column_validators: default_types.iter().map(|&t| Some(ColumnValidator::Basic(t))).collect(),
            column_types: default_types, // Keep types consistent
            column_filters: vec![None; num_cols], // Initialize filters with None
        }
    }

    // --- NEW: Helper to get basic type from validator ---
    // Useful for UI logic that still relies on the basic type (like ui_for_cell parsing)
    pub fn get_validator_basic_type(&self, col_index: usize) -> Option<ColumnDataType> {
        self.column_validators.get(col_index)
            .and_then(|opt_validator| opt_validator.as_ref())
            .and_then(|validator| match validator {
                ColumnValidator::Basic(data_type) => Some(*data_type),
                // For Linked columns, the base type for UI interaction is String (dropdown)
                ColumnValidator::Linked { .. } => Some(ColumnDataType::String),
            })
    }

    // --- NEW: Helper to ensure consistency after loading/modification ---
    // Call this after potential modifications (loading old data, adding/removing cols)
    pub fn ensure_validator_consistency(&mut self) -> bool {
        let num_headers = self.column_headers.len();
        let mut changed = false;

        // Sync column_types length (using validator's basic type or default String)
        if self.column_types.len() != num_headers {
            warn!("Correcting column_types length mismatch for sheet '{}'. Resizing from {} to {}.",
                  self.sheet_name, self.column_types.len(), num_headers);
            self.column_types.resize(num_headers, ColumnDataType::String); // Default resize first
            // Then try to fill based on validators (if available and Basic) or keep String default
            for i in 0..num_headers {
                if let Some(typ) = self.get_validator_basic_type(i) { // Use helper
                     self.column_types[i] = typ;
                }
                // If validator is missing or Linked, type remains String (or whatever default was)
            }
            changed = true;
        }

        // Sync column_filters length
        if self.column_filters.len() != num_headers {
             warn!("Correcting column_filters length mismatch for sheet '{}'. Resizing from {} to {}.",
                   self.sheet_name, self.column_filters.len(), num_headers);
             self.column_filters.resize(num_headers, None);
             changed = true;
        }

        // Sync column_validators length and content
        if self.column_validators.len() != num_headers {
            warn!("Correcting column_validators length mismatch for sheet '{}'. Resizing from {} to {}.",
                  self.sheet_name, self.column_validators.len(), num_headers);
            // Resize first, filling with None temporarily
            self.column_validators.resize(num_headers, None);
            // Now try to fill based on column_types
            for i in 0..num_headers {
                 if self.column_validators[i].is_none() { // Only fill if None (missing)
                     let basic_type = self.column_types.get(i).copied().unwrap_or_default(); // Use type or default
                     self.column_validators[i] = Some(ColumnValidator::Basic(basic_type));
                     warn!("Initialized missing validator for column {} of '{}' based on type {:?}.",
                           i + 1, self.sheet_name, basic_type);
                     changed = true; // Mark changed because we initialized a validator
                 }
            }
            // No need for changed = true here, already handled by resize check
        }

        // Now ensure consistency between types and *existing* Basic validators
        // This must run even if lengths initially matched
        for i in 0..num_headers {
            let basic_type = self.column_types.get(i).copied().unwrap_or_default();
            if let Some(Some(ColumnValidator::Basic(validator_type))) = self.column_validators.get_mut(i) {
                if *validator_type != basic_type {
                    warn!("Correcting validator type mismatch for column {} of '{}'. Validator had {:?}, type is {:?}. Updating validator.",
                          i + 1, self.sheet_name, *validator_type, basic_type);
                    *validator_type = basic_type;
                    changed = true;
                }
            } else if self.column_validators.get(i).map_or(true, |v| v.is_none()) {
                // This case handles `default_column_validators` if the field existed but was null/empty in JSON,
                // or if a column was added without a validator.
                self.column_validators[i] = Some(ColumnValidator::Basic(basic_type));
                warn!("Initialized 'None' validator for column {} of '{}' based on type {:?}.",
                      i + 1, self.sheet_name, basic_type);
                changed = true;
            }
            // We don't force Linked validators to change based on column_types
        }


        changed // Return true if any correction was made
    }
}