// src/sheets/systems/io/validator.rs
use bevy::prelude::*;
use std::path::Path;
use crate::sheets::{
    definitions::{SheetMetadata, SheetGridData},
    resources::SheetRegistry,
    events::SheetOperationFeedback, // Use the specific event
};

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
    feedback_writer: &mut EventWriter<SheetOperationFeedback> // For warnings
) -> Result<(), String> {
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err("Sheet name cannot be empty or just whitespace.".to_string());
    }
    // Basic check for potentially problematic names derived from filenames
    if trimmed_name == ".meta" || trimmed_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
         return Err(format!("Invalid sheet name derived (contains invalid characters or is reserved): '{}'", name));
    }

    // <<< --- FIX: Use does_sheet_exist to check globally --- >>>
    if registry.does_sheet_exist(trimmed_name) {
        // It's not necessarily an error to overwrite via upload (upload replaces),
        // but warn the user that a sheet with this name exists somewhere.
        let msg = format!("Sheet name '{}' already exists (possibly in another category). Upload will overwrite/update if it's in the Root category, or fail if the name exists elsewhere.", trimmed_name);
         warn!("{}", msg); // Log internally
         // Make this a warning, not an error, as upload logic might handle replacement.
         feedback_writer.send(SheetOperationFeedback { message: msg, is_error: false });
    }
    Ok(())
}

 /// Validates the consistency of loaded metadata against expected values.
 /// Returns Ok or an error string containing all validation failures.
 /// Optionally corrects the metadata in place if warnings_only is true and returns Ok.
 pub fn validate_or_correct_loaded_metadata(
     metadata: &mut SheetMetadata, // Mutable to allow correction
     expected_sheet_name: &str, // e.g., the registry key or derived name
     expected_data_filename: &str, // e.g., the grid file name it was loaded alongside
     // Removed expected_category, category correction handled elsewhere (startup_load/scan)
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
                corrections_made.push(format!("Corrected sheet_name to '{}'", expected_sheet_name));
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
                 corrections_made.push(format!("Corrected data_filename to '{}'", expected_filename_only));
                 metadata.data_filename = expected_filename_only.to_string();
            } else {
                 issues.push(issue);
            }
       }

      // Add more checks as needed (e.g., non-empty headers if types exist)
      if metadata.column_headers.len() != metadata.column_types.len() {
           let issue = format!(
                "Metadata structure inconsistency: {} headers, {} types",
                metadata.column_headers.len(), metadata.column_types.len()
           );
           // Correction here is complex, usually better to error out or rely on ensure_validator_consistency
           issues.push(issue);
      }
       if metadata.column_headers.len() != metadata.column_validators.len() {
            // This check might be redundant if ensure_validator_consistency runs, but good sanity check
            let issue = format!(
                 "Metadata structure inconsistency: {} headers, {} validators",
                 metadata.column_headers.len(), metadata.column_validators.len()
            );
            issues.push(issue);
       }
        if metadata.column_headers.len() != metadata.column_filters.len() {
             let issue = format!(
                  "Metadata structure inconsistency: {} headers, {} filters",
                  metadata.column_headers.len(), metadata.column_filters.len()
             );
             issues.push(issue);
        }


      // --- Final Result ---
      if issues.is_empty() {
           if !corrections_made.is_empty() {
                info!("Metadata for '{}' validated with corrections: {}", expected_sheet_name, corrections_made.join("; "));
           }
           Ok(())
      } else if warnings_only {
          // If we are only warning, but there were uncorrectable issues
          error!("Metadata validation for '{}' found uncorrectable issues: {}", expected_sheet_name, issues.join("; "));
          // Return Ok because we are in warnings_only mode, but logged errors
          // Let ensure_validator_consistency try to fix length issues if possible
          Ok(())
      } else {
           // Errors found and we are not in warnings_only mode
           Err(issues.join("; "))
      }
 }

/// Validates the grid structure against metadata (e.g., column count consistency).
pub fn validate_grid_structure(
     grid: &Vec<Vec<String>>,
     metadata: &SheetMetadata,
     sheet_name: &str, // For logging context
) -> Result<(), String> {
     if grid.is_empty() {
          return Ok(()); // Nothing to validate in an empty grid
     }
     let expected_cols = metadata.column_headers.len();
     // Check if metadata itself is inconsistent (should be caught by ensure_validator_consistency ideally)
     if expected_cols != metadata.column_types.len() || expected_cols != metadata.column_validators.len() || expected_cols != metadata.column_filters.len() {
          return Err(format!(
               "Metadata inconsistency for sheet '{:?}/{}': Headers ({}), Types ({}), Validators ({}), Filters ({}). Cannot validate grid.",
               metadata.category, sheet_name, expected_cols, metadata.column_types.len(), metadata.column_validators.len(), metadata.column_filters.len()
          ));
     }

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