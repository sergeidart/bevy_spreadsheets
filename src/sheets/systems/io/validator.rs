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
/// during an upload/creation process. Sends warnings via EventWriter.
pub fn validate_sheet_name_for_upload(
    name: &str,
    registry: &SheetRegistry,
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
    if registry.get_sheet(trimmed_name).is_some() {
        // It's not necessarily an error to overwrite, but send feedback
        let msg = format!("Sheet name '{}' already exists. Upload will overwrite.", trimmed_name);
         warn!("{}", msg); // Log internally
         feedback_writer.send(SheetOperationFeedback { message: msg, is_error: false }); // Send to UI
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

      // Check Data Filename
       if metadata.data_filename != expected_data_filename {
            let issue = format!(
                 "Metadata filename mismatch: Expected '{}', found '{}'",
                 expected_data_filename, metadata.data_filename
            );
            if warnings_only {
                 warn!("Correcting {}", issue);
                 corrections_made.push(format!("Corrected data_filename to '{}'", expected_data_filename));
                 metadata.data_filename = expected_data_filename.to_string();
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
           // Correction here is complex, usually better to error out
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
     // Check if metadata itself is inconsistent
     if expected_cols != metadata.column_types.len() {
          return Err(format!(
               "Metadata inconsistency for sheet '{}': {} headers, {} types. Cannot validate grid.",
               sheet_name, expected_cols, metadata.column_types.len()
          ));
     }

     for (i, row) in grid.iter().enumerate() {
          if row.len() != expected_cols {
               return Err(format!(
                    "Sheet '{}' row {} column count mismatch: Expected {}, found {}. Grid invalid.",
                    sheet_name, i, expected_cols, row.len()
               ));
          }
     }
     Ok(())
}

// Add other specific validation functions as needed...