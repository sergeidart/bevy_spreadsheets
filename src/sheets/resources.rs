// src/sheets/resources.rs
use bevy::prelude::*;
use std::collections::{HashMap, BTreeMap}; // Use BTreeMap for sorted categories

use super::definitions::{SheetGridData, SheetMetadata};

/// Core Resource holding all registered sheet data, categorized by folder structure.
#[derive(Resource, Default, Debug)]
pub struct SheetRegistry {
    // Key: Category Name (None for root), Value: Map of SheetName -> SheetData
    categorized_sheets: BTreeMap<Option<String>, HashMap<String, SheetGridData>>,
    // Store category names separately for UI ordering (BTreeMap keys are already sorted)
}

impl SheetRegistry {
    /// Gets the path relative to the data directory for a given sheet.
    /// Returns PathBuf("CategoryName/FileName.json") or PathBuf("FileName.json").
    fn get_relative_path(metadata: &SheetMetadata) -> std::path::PathBuf {
        let mut path = std::path::PathBuf::new();
        if let Some(cat) = &metadata.category {
            path.push(cat);
        }
        path.push(&metadata.data_filename);
        path
    }

    /// Registers sheet metadata under its category.
    /// This is typically used for pre-defined sheets at startup.
    pub fn register(&mut self, mut metadata: SheetMetadata) -> bool {
        let name = metadata.sheet_name.clone();
        let category = metadata.category.clone(); // Can be None

        // Ensure data_filename is just the filename part if not already
        if let Some(filename_only) = std::path::Path::new(&metadata.data_filename).file_name() {
            metadata.data_filename = filename_only.to_string_lossy().into_owned();
        } else {
             warn!("Could not extract filename from '{}' for sheet '{}'. Using full path.", metadata.data_filename, name);
        }


        let category_map = self.categorized_sheets.entry(category.clone()).or_default();

        if !category_map.contains_key(&name) {
            let mut data = SheetGridData::default();
            data.metadata = Some(metadata); // Store the owned metadata
            category_map.insert(name.clone(), data);
            true
        } else {
            warn!("Sheet '{}' in category '{:?}' already registered. Registration skipped.", name, category);
            false
        }
    }

    /// Adds or replaces a sheet in the specified category.
    pub fn add_or_replace_sheet(
        &mut self,
        category: Option<String>,
        name: String,
        mut data: SheetGridData,
    ) {
        // Ensure metadata exists and is consistent
        if data.metadata.is_none() {
            let num_cols = data.grid.first().map_or(0, |row| row.len());
            // Use a filename derived from the sheet name
            let filename = format!("{}.json", name);
            data.metadata = Some(SheetMetadata::create_generic(
                name.clone(),
                filename,
                num_cols,
                category.clone(), // Pass category
            ));
        } else if let Some(meta) = &mut data.metadata {
            // Ensure metadata name matches the key
            if meta.sheet_name != name {
                warn!("Correcting metadata sheet_name ('{}') to match registry key ('{}').", meta.sheet_name, name);
                meta.sheet_name = name.clone();
            }
            // Ensure category matches
            if meta.category != category {
                 warn!("Correcting metadata category ('{:?}') to match registry category ('{:?}').", meta.category, category);
                 meta.category = category.clone();
            }
            // Ensure filename is consistent if not set properly or contains path separators
             let filename_only = std::path::Path::new(&meta.data_filename)
                .file_name()
                .map(|os| os.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("{}.json", name));

            if meta.data_filename != filename_only {
                 warn!("Correcting data_filename for sheet '{}' from '{}' to '{}'.", name, meta.data_filename, filename_only);
                 meta.data_filename = filename_only;
            }
        }

        // Get or create the category map and insert/replace the sheet
        self.categorized_sheets
            .entry(category)
            .or_default()
            .insert(name, data);
    }

     /// Returns a sorted list of category names (including None for root).
     pub fn get_categories(&self) -> Vec<Option<String>> {
         self.categorized_sheets.keys().cloned().collect()
     }

     /// Returns a sorted list of sheet names within a specific category.
     pub fn get_sheet_names_in_category(&self, category: &Option<String>) -> Vec<String> {
         let mut names = Vec::new();
         if let Some(category_map) = self.categorized_sheets.get(category) {
             names = category_map.keys().cloned().collect();
             names.sort_unstable();
         }
         names
     }

     /// Checks if a sheet exists anywhere across all categories.
     pub fn does_sheet_exist(&self, sheet_name: &str) -> bool {
         self.categorized_sheets.values().any(|category_map| category_map.contains_key(sheet_name))
     }

    /// Gets mutable access to the SheetGridData for a given sheet name within a specific category.
    pub fn get_sheet_mut(
        &mut self,
        category: &Option<String>,
        sheet_name: &str,
    ) -> Option<&mut SheetGridData> {
        self.categorized_sheets
            .get_mut(category)
            .and_then(|category_map| category_map.get_mut(sheet_name))
    }

    /// Gets immutable access to the SheetGridData for a given sheet name within a specific category.
    pub fn get_sheet(&self, category: &Option<String>, sheet_name: &str) -> Option<&SheetGridData> {
        self.categorized_sheets
            .get(category)
            .and_then(|category_map| category_map.get(sheet_name))
    }

    /// Provides an iterator over all sheets: (CategoryNameOpt, SheetName, SheetData).
    pub fn iter_sheets(&self) -> impl Iterator<Item = (&Option<String>, &String, &SheetGridData)> {
        self.categorized_sheets
            .iter()
            .flat_map(|(category, sheets_map)| {
                sheets_map.iter().map(move |(sheet_name, sheet_data)| {
                    (category, sheet_name, sheet_data)
                })
            })
    }

    /// Provides a mutable iterator over all sheets: (CategoryNameOpt, SheetName, SheetDataMut).
     pub fn iter_sheets_mut(&mut self) -> impl Iterator<Item = (&Option<String>, &String, &mut SheetGridData)> {
         self.categorized_sheets
             .iter_mut()
             .flat_map(|(category, sheets_map)| {
                 sheets_map.iter_mut().map(move |(sheet_name, sheet_data)| {
                     // Need to re-borrow category immutably here, which is tricky.
                     // Let's simplify the mutable iteration API for now or return owned data.
                     // A simpler approach might be separate iterators for categories and sheets within.
                     // For now, let's just return the sheet name and mutable data.
                     // Caller needs to know the category separately if needed.
                     // TODO: Revisit mutable iteration API if needed.
                     (category, sheet_name, sheet_data) // This might have lifetime issues, requires careful use.
                 })
             })
     }

    /// Renames a sheet *within its current category*.
    /// Updates the HashMap key and the `sheet_name` and `data_filename` within the `SheetMetadata`.
    /// Does NOT handle moving between categories.
    /// Returns the old `SheetGridData` (with updated metadata) if successful, or an error string.
    pub fn rename_sheet(
        &mut self,
        category: &Option<String>,
        old_name: &str,
        new_name: String,
    ) -> Result<SheetGridData, String> {
        if old_name == new_name {
            return Err("New name cannot be the same as the old name.".to_string());
        }
        if self.does_sheet_exist(&new_name) { // Check across all categories
            return Err(format!("A sheet named '{}' already exists (possibly in another category).", new_name));
        }
        if new_name.trim().is_empty() {
            return Err("New sheet name cannot be empty or just whitespace.".to_string());
        }
        // Basic filename character check
        if new_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            return Err("New sheet name contains invalid characters for filenames.".to_string());
        }

        // 1. Get mutable access to the specific category map
        let category_map = match self.categorized_sheets.get_mut(category) {
            Some(map) => map,
            None => return Err(format!("Category '{:?}' not found.", category)),
        };

        // 2. Remove from the category's HashMap
        if let Some(mut data) = category_map.remove(old_name) {
            // 3. Update metadata name and filename
            if let Some(meta) = &mut data.metadata {
                meta.sheet_name = new_name.clone();
                // Update data_filename (just the name part)
                meta.data_filename = format!("{}.json", new_name);
                info!(
                    "Updated metadata: sheet_name='{}', data_filename='{}' in category '{:?}'",
                    meta.sheet_name, meta.data_filename, category
                );
            } else {
                error!("Sheet '{}' found in category '{:?}' but is missing metadata during rename!", old_name, category);
                // Re-insert to avoid data loss and return error
                category_map.insert(old_name.to_string(), data);
                return Err(format!("Internal error: Metadata missing for sheet '{}' during rename.", old_name));
            }

            // Capture the modified data before moving ownership
            let updated_data_for_return = data.clone();

            // 4. Insert back into the *same category's* HashMap with the new name
            category_map.insert(new_name.clone(), data); // data now has updated metadata

            Ok(updated_data_for_return)
        } else {
            Err(format!("Sheet '{}' not found in category '{:?}' for renaming.", old_name, category))
        }
    }

    /// Deletes a sheet from its category in the registry.
    /// Returns the removed `SheetGridData` if successful, or an error string.
    pub fn delete_sheet(
        &mut self,
        category: &Option<String>,
        sheet_name: &str,
    ) -> Result<SheetGridData, String> {
        // 1. Get mutable access to the category map
        if let Some(category_map) = self.categorized_sheets.get_mut(category) {
            // 2. Remove from the category's HashMap
            if let Some(data) = category_map.remove(sheet_name) {
                // 3. If category map becomes empty, remove the category itself
                if category_map.is_empty() {
                    self.categorized_sheets.remove(category);
                }
                Ok(data) // Return the removed data
            } else {
                Err(format!("Sheet '{}' not found in category '{:?}' for deletion.", sheet_name, category))
            }
        } else {
            Err(format!("Category '{:?}' not found for deletion.", category))
        }
    }
}