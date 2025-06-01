// src/sheets/systems/io/parsers.rs
use bevy::prelude::{warn}; // Only need warn for now
use std::fs;
use std::path::Path;
use crate::sheets::definitions::SheetMetadata; // Use definition

/// Reads and parses a JSON file expected to contain Vec<Vec<String>> (Grid Data).
/// Assumes basic file validation (existence) has already occurred.
/// Returns Result<(GridData, OriginalFilename), ErrorMsg>
pub fn read_and_parse_json_sheet(path: &Path) -> Result<(Vec<Vec<String>>, String), String> {
     let file_content = fs::read_to_string(path)
         .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;

     let trimmed_content = file_content.trim_start_matches('\u{FEFF}');

     let filename = path.file_name()
         .map(|s| s.to_string_lossy().into_owned())
         .unwrap_or_else(|| {
              warn!("Could not derive filename from path '{}', using 'unknown.json'.", path.display());
              "unknown.json".to_string()
         });

     if trimmed_content.is_empty() {
         warn!("File '{}' is empty. Parsing as empty sheet.", path.display());
         return Ok((Vec::new(), filename));
     }

     let grid: Vec<Vec<String>> = serde_json::from_str(trimmed_content)
         .map_err(|e| format!("Failed to parse JSON grid data from '{}': {}", path.display(), e))?;

     Ok((grid, filename))
 }

 /// Reads and parses a SheetMetadata JSON file.
 /// Assumes basic file validation (existence) has already occurred.
 pub fn read_and_parse_metadata_file(path: &Path) -> Result<SheetMetadata, String> {
     let file_content = fs::read_to_string(path)
         .map_err(|e| format!("Failed to read metadata file '{}': {}", path.display(), e))?;
     let trimmed_content = file_content.trim_start_matches('\u{FEFF}');
     if trimmed_content.is_empty() {
         return Err(format!("Metadata file '{}' is empty.", path.display()));
     }
     serde_json::from_str(trimmed_content)
         .map_err(|e| format!("Failed to parse metadata JSON from '{}': {}", path.display(), e))
 }