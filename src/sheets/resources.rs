// src/sheets/resources.rs
use bevy::prelude::*;
use std::collections::HashMap;

use super::definitions::{SheetGridData, SheetMetadata};

/// Core Resource holding all registered sheet data.
/// Now uses String keys and stores owned metadata.
#[derive(Resource, Default, Debug)]
pub struct SheetRegistry {
    sheets: HashMap<String, SheetGridData>, // Key is String
    sheet_names_sorted: Vec<String>,      // Stores String
}

impl SheetRegistry {
    /// Registers sheet metadata. Now takes owned SheetMetadata.
    /// Returns true if registration was successful (new sheet), false otherwise.
    pub fn register(&mut self, metadata: SheetMetadata) -> bool {
        let name = metadata.sheet_name.clone(); // Clone name for separate use
        if !self.sheets.contains_key(&name) {
            let mut data = SheetGridData::default();
            // Store the owned metadata directly
            data.metadata = Some(metadata);
            self.sheets.insert(name.clone(), data); // Insert cloned name
            self.sheet_names_sorted.push(name); // Push cloned name
            self.sheet_names_sorted.sort_unstable(); // Sort owned strings
            true
        } else {
            warn!("Sheet '{}' already registered. Registration skipped.", name);
            false
        }
    }

    /// Adds or replaces a sheet directly using SheetGridData (e.g., from upload).
    /// Ensures metadata exists if grid data is present.
    pub fn add_or_replace_sheet(&mut self, name: String, mut data: SheetGridData) {
         // Ensure metadata is present if grid isn't empty
        if data.metadata.is_none() && !data.grid.is_empty() {
            let num_cols = data.grid.first().map_or(0, |row| row.len());
             // Use a filename derived from the sheet name for saving consistency
            let filename = format!("{}.json", name);
            data.metadata = Some(SheetMetadata::create_generic(name.clone(), filename, num_cols));
        } else if let Some(meta) = &mut data.metadata {
             // Ensure metadata name matches the key (important for rename/replace logic)
             if meta.sheet_name != name {
                  warn!("Correcting metadata sheet_name ('{}') to match registry key ('{}').", meta.sheet_name, name);
                  meta.sheet_name = name.clone();
             }
             // Ensure filename is consistent if not set properly
             if meta.data_filename.is_empty() {
                  meta.data_filename = format!("{}.json", name);
                  warn!("Generated missing data_filename for sheet '{}': {}", name, meta.data_filename);
             }
        }


        if self.sheets.insert(name.clone(), data).is_none() {
            // If it was a new insertion, update the sorted list
            if !self.sheet_names_sorted.contains(&name) {
                self.sheet_names_sorted.push(name);
                self.sheet_names_sorted.sort_unstable();
            }
        }
    }

    /// Returns a sorted list of registered sheet names.
    pub fn get_sheet_names(&self) -> &Vec<String> { // Returns &Vec<String>
        &self.sheet_names_sorted
    }

    /// Gets mutable access to the SheetGridData for a given sheet name.
    pub fn get_sheet_mut(&mut self, sheet_name: &str) -> Option<&mut SheetGridData> {
        self.sheets.get_mut(sheet_name)
    }

    /// Gets immutable access to the SheetGridData for a given sheet name.
    pub fn get_sheet(&self, sheet_name: &str) -> Option<&SheetGridData> {
        self.sheets.get(sheet_name)
    }

    /// Provides an iterator over all registered sheets (name, data).
    pub fn iter_sheets_mut(&mut self) -> impl Iterator<Item = (&String, &mut SheetGridData)> {
        self.sheets.iter_mut() // HashMap iter returns (&K, &mut V)
    }

    /// Provides an immutable iterator over all registered sheets (name, data).
    pub fn iter_sheets(&self) -> impl Iterator<Item = (&String, &SheetGridData)> {
        self.sheets.iter() // HashMap iter returns (&K, &V)
    }

    // --- NEW Methods ---

    /// Renames a sheet within the registry.
    /// Updates the internal HashMap key, the sorted name list,
    /// and the `sheet_name` and `data_filename` within the `SheetMetadata`. // MODIFIED DOC
    /// Returns the old `SheetGridData` (with updated metadata) if successful, or an error string. // MODIFIED DOC
    pub fn rename_sheet(&mut self, old_name: &str, new_name: String) -> Result<SheetGridData, String> {
        if old_name == new_name {
            return Err("New name cannot be the same as the old name.".to_string());
        }
        if self.sheets.contains_key(&new_name) {
            return Err(format!("Sheet name '{}' already exists.", new_name));
        }
        if new_name.trim().is_empty() {
            return Err("New sheet name cannot be empty or just whitespace.".to_string());
        }
        // Make sure filename characters are reasonable (basic check)
        if new_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
             return Err("New sheet name contains invalid characters for filenames.".to_string());
        }


        // 1. Remove from HashMap
        if let Some(mut data) = self.sheets.remove(old_name) {
             // 2. Update metadata name and filename (should always exist if sheet exists)
             if let Some(meta) = &mut data.metadata {
                 // Store old filename *before* updating metadata, maybe needed by caller?
                 // Let's actually do this in the calling system (handle_rename_request)
                 // as it's cleaner separation of concerns.

                 // Update internal metadata
                 meta.sheet_name = new_name.clone();
                 // *** FIX: Update data_filename to match new sheet name ***
                 meta.data_filename = format!("{}.json", new_name);
                 info!("Updated metadata: sheet_name='{}', data_filename='{}'", meta.sheet_name, meta.data_filename); // Add log

             } else {
                 // This case shouldn't happen if registry invariants are maintained
                 error!("Sheet '{}' found in map but is missing metadata during rename!", old_name);
                 // Re-insert to avoid data loss and return error
                 self.sheets.insert(old_name.to_string(), data);
                 return Err(format!("Internal error: Metadata missing for sheet '{}' during rename.", old_name));
             }

            // 3. Remove old name from sorted list
            if let Some(index) = self.sheet_names_sorted.iter().position(|n| n == old_name) {
                self.sheet_names_sorted.remove(index);
            } else {
                 warn!("Old name '{}' not found in sorted list during rename.", old_name);
            }

            // Capture the modified data before moving ownership
            let updated_data_for_return = data.clone();

            // 4. Insert back into HashMap with new name
            self.sheets.insert(new_name.clone(), data); // data now has updated metadata

            // 5. Insert new name into sorted list and re-sort
            self.sheet_names_sorted.push(new_name); // new_name was already cloned
            self.sheet_names_sorted.sort_unstable();

            // Return the data with updated metadata
            Ok(updated_data_for_return)
        } else {
            Err(format!("Sheet '{}' not found for renaming.", old_name))
        }
        // Note: The decision to update the filename here simplifies the calling code.
        // The caller (handle_rename_request) is still responsible for *triggering*
        // the actual file system rename operation.
    }


    /// Deletes a sheet from the registry.
    /// Removes from the internal HashMap and the sorted name list.
    /// Returns the removed `SheetGridData` if successful, or an error string.
    pub fn delete_sheet(&mut self, sheet_name: &str) -> Result<SheetGridData, String> {
         // 1. Remove from HashMap
         if let Some(data) = self.sheets.remove(sheet_name) {
            // 2. Remove from sorted list
            if let Some(index) = self.sheet_names_sorted.iter().position(|n| n == sheet_name) {
                self.sheet_names_sorted.remove(index);
                // No need to re-sort after removal
            } else {
                 warn!("Deleted sheet name '{}' was not found in sorted list.", sheet_name);
            }
            Ok(data) // Return the removed data
         } else {
            Err(format!("Sheet '{}' not found for deletion.", sheet_name))
         }
    }
}