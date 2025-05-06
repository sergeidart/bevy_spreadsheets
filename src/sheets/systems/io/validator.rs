// src/sheets/systems/io/validator.rs
use crate::sheets::{
     definitions::{SheetGridData, SheetMetadata}, // Keep SheetMetadata import
     events::SheetOperationFeedback,               // Use the specific event
     resources::SheetRegistry,
 };
 use bevy::prelude::*;
 use std::path::Path;
 
 /// Validates if a file path exists and is a file.
 pub fn validate_file_exists(path: &Path) -> Result<(), String> {
     if !path.exists() {
         Err(format!("File not found: '{}'", path.display()))
     } else if !path.is_file() {
         Err(format!("Path is not a file: '{}'", path.display()))
     } else {
         Ok(())
     }
 }
 
 /// Validates a sheet name against the registry for potential collisions or emptiness
 /// during an upload/creation process. Checks for existence across all categories.
 /// Sends warnings via EventWriter.
 pub fn validate_sheet_name_for_upload(
     name: &str,
     registry: &SheetRegistry, // Should be Res<SheetRegistry> typically
     feedback_writer: &mut EventWriter<SheetOperationFeedback>, // For warnings
 ) -> Result<(), String> {
     let trimmed_name = name.trim();
     if trimmed_name.is_empty() {
         return Err("Sheet name cannot be empty or just whitespace.".to_string());
     }
     // Basic check for potentially problematic names derived from filenames
     if trimmed_name == ".meta"
         || trimmed_name.starts_with('.') // Check for leading dot
         || trimmed_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
     {
         return Err(format!(
             "Invalid sheet name derived (contains invalid characters, starts with '.', or is reserved): '{}'",
             name
         ));
     }
 
     if registry.does_sheet_exist(trimmed_name) {
         // It's not necessarily an error to overwrite via upload (upload replaces),
         // but warn the user that a sheet with this name exists somewhere.
         let msg = format!("Sheet name '{}' already exists (possibly in another category). Upload will overwrite/update if it's in the Root category, or fail if the name exists elsewhere.", trimmed_name);
         warn!("{}", msg); // Log internally
                          // Make this a warning, not an error, as upload logic might handle replacement.
         feedback_writer.send(SheetOperationFeedback {
             message: msg,
             is_error: false, // It's a warning about potential overwrite
         });
     }
     Ok(())
 }
 
 /// Validates a sheet name derived during startup scan.
 /// Checks for emptiness, reserved names, and invalid characters.
 pub fn validate_derived_sheet_name(name: &str) -> Result<(), String> {
     let trimmed_name = name.trim();
     if trimmed_name.is_empty() {
         return Err("Derived sheet name is empty or just whitespace.".to_string());
     }
     if trimmed_name == ".meta"
         || trimmed_name.starts_with('.') // Check for leading dot
         || trimmed_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
     {
         return Err(format!(
             "Invalid derived sheet name (contains invalid characters, starts with '.', or is reserved): '{}'",
             name
         ));
     }
     Ok(())
 }
 
 /// Validates a category name derived during startup scan.
 /// Checks for emptiness and invalid starting characters.
 pub fn validate_derived_category_name(category_name: &str) -> Result<(), String> {
     let trimmed_name = category_name.trim();
     if trimmed_name.is_empty() {
         // This case might not occur if derived from PathBuf, but check anyway
         return Err("Derived category name is empty.".to_string());
     }
     if trimmed_name.starts_with('.') {
         return Err(format!(
             "Invalid derived category name (starts with '.'): '{}'",
             category_name
         ));
     }
     // Add other invalid category name checks if needed (e.g., specific characters)
     Ok(())
 }
 
 /// Validates the consistency of loaded metadata against expected values.
 /// Returns Ok or an error string containing all validation failures.
 /// Optionally corrects the metadata in place if warnings_only is true and returns Ok.
 /// Note: This function primarily validates name and filename now. Length consistency
 /// is better handled by ensure_column_consistency after loading.
 pub fn validate_or_correct_loaded_metadata(
     metadata: &mut SheetMetadata, // Mutable to allow correction
     expected_sheet_name: &str,    // e.g., the registry key or derived name
     expected_data_filename: &str, // e.g., the grid file name it was loaded alongside
     warnings_only: bool, // If true, log warnings and correct; if false, return Err on mismatch
 ) -> Result<(), String> {
     let mut issues = Vec::new();
     let mut corrections_made = Vec::new();
 
     // Check Sheet Name
     if metadata.sheet_name != expected_sheet_name {
         let issue = format!(
             "Metadata name mismatch: Expected '{}', found '{}'",
             expected_sheet_name, metadata.sheet_name
         );
         if warnings_only {
             warn!("Correcting {}", issue);
             corrections_made.push(format!(
                 "Corrected sheet_name to '{}'",
                 expected_sheet_name
             ));
             metadata.sheet_name = expected_sheet_name.to_string();
         } else {
             issues.push(issue);
         }
     }
 
     // Check Data Filename (ensure it's just filename part)
     let expected_filename_only = std::path::Path::new(expected_data_filename)
         .file_name()
         .map(|os| os.to_string_lossy().into_owned())
         .unwrap_or_else(|| expected_data_filename.to_string()); // Fallback
 
     if metadata.data_filename != expected_filename_only {
         let issue = format!(
             "Metadata filename mismatch: Expected '{}', found '{}'",
             expected_filename_only, metadata.data_filename
         );
         if warnings_only {
             warn!("Correcting {}", issue);
             corrections_made.push(format!(
                 "Corrected data_filename to '{}'",
                 expected_filename_only
             ));
             metadata.data_filename = expected_filename_only.to_string();
         } else {
             issues.push(issue);
         }
     }
 
     // --- REMOVED internal length consistency checks ---
     // These checks are now implicitly handled by the SheetMetadata::ensure_column_consistency
     // method, which should be called *after* loading and potential corrections here.
     /*
     if metadata.column_headers.len() != metadata.column_types.len() { ... }
     if metadata.column_headers.len() != metadata.column_validators.len() { ... }
     if metadata.column_headers.len() != metadata.column_filters.len() { ... }
     */
 
     // --- Final Result ---
     if issues.is_empty() {
         if !corrections_made.is_empty() {
             info!(
                 "Metadata for '{}' validated with corrections: {}",
                 expected_sheet_name,
                 corrections_made.join("; ")
             );
         }
         Ok(())
     } else if warnings_only {
         // If we are only warning, but there were uncorrectable issues
         error!(
             "Metadata validation for '{}' found uncorrectable issues: {}",
             expected_sheet_name,
             issues.join("; ")
         );
         // Return Ok because we are in warnings_only mode, but logged errors
         // Let ensure_column_consistency try to fix length issues if possible later
         Ok(())
     } else {
         // Errors found and we are not in warnings_only mode
         Err(issues.join("; "))
     }
 }
 
 /// Validates the grid structure (row lengths) against metadata's column count.
 pub fn validate_grid_structure(
     grid: &Vec<Vec<String>>,
     metadata: &SheetMetadata,
     sheet_name: &str, // For logging context
 ) -> Result<(), String> {
     if grid.is_empty() {
         return Ok(()); // Nothing to validate in an empty grid
     }
     // --- CORRECTED: Get expected columns from columns.len() ---
     let expected_cols = metadata.columns.len();
 
     // --- REMOVED internal metadata consistency check ---
     // Assume metadata passed here is reasonably consistent or checked elsewhere.
     /*
     if expected_cols != metadata.column_types.len() || ... { ... }
     */
 
     // Validate row lengths against expected_cols
     for (i, row) in grid.iter().enumerate() {
         if row.len() != expected_cols {
             return Err(format!(
                 "Sheet '{:?}/{}' row {} column count mismatch: Expected {}, found {}. Grid invalid.",
                 metadata.category, sheet_name, i, expected_cols, row.len()
             ));
         }
     }
     Ok(())
 }
 