// src/sheets/resources.rs
use bevy::prelude::*;
use std::collections::{HashMap, BTreeMap};
// ADDED: Import ValidationState from ui::validation
use crate::ui::validation::ValidationState;


use super::definitions::{SheetGridData, SheetMetadata};

// --- NEW: RenderableCellData ---
/// Holds pre-processed data for rendering a single cell.
#[derive(Clone, Debug, Default)]
pub struct RenderableCellData {
    /// The string to actually display in the UI widget.
    /// This might be the raw string or a formatted version (e.g., "true" for bool).
    pub display_text: String,
    /// The calculated validation state for this cell.
    pub validation_state: ValidationState,
    // pub background_color: egui::Color32, // Example: Could be added if not derived in widget
    // pub text_color: egui::Color32,     // Example
}

// --- NEW: SheetRenderCache Resource ---
/// Resource holding the pre-calculated renderable data for each cell of each sheet.
#[derive(Resource, Default, Debug)]
pub struct SheetRenderCache {
    // Key: (Category Name Opt, Sheet Name)
    // Value: Grid of renderable cell data matching the data grid structure
    pub(crate) states: HashMap<(Option<String>, String), Vec<Vec<RenderableCellData>>>,
}

impl SheetRenderCache {
    /// Gets the renderable data for a specific cell, if it exists.
    pub fn get_cell_data(
        &self,
        category: &Option<String>,
        sheet_name: &str,
        row: usize,
        col: usize,
    ) -> Option<&RenderableCellData> {
        self.states
            .get(&(category.clone(), sheet_name.to_string()))
            .and_then(|grid_state| grid_state.get(row))
            .and_then(|row_state| row_state.get(col))
    }

    /// Updates or inserts the entire renderable grid data for a sheet.
    #[allow(dead_code)] // Might be used internally or for full rebuilds
    pub(crate) fn update_sheet_grid_data(
        &mut self,
        category: Option<String>,
        sheet_name: String,
        new_grid_render_data: Vec<Vec<RenderableCellData>>,
    ) {
        self.states
            .insert((category, sheet_name), new_grid_render_data);
    }

    /// Clears the render cache for a specific sheet (e.g., when deleted).
    pub(crate) fn clear_sheet_render_data(&mut self, category: &Option<String>, sheet_name: &str) {
        let key = (category.clone(), sheet_name.to_string());
        if self.states.remove(&key).is_some() {
            trace!("Cleared render cache for sheet '{:?}/{}'.", category, sheet_name);
        }
    }

    /// Renames the render cache entry for a sheet.
    pub(crate) fn rename_sheet_render_data(
        &mut self,
        category: &Option<String>,
        old_name: &str,
        new_name: &str,
    ) {
        let old_key = (category.clone(), old_name.to_string());
        if let Some(state_grid) = self.states.remove(&old_key) {
            let new_key = (category.clone(), new_name.to_string());
            self.states.insert(new_key, state_grid);
             trace!("Renamed render cache for sheet '{:?}/{}' to '{:?}/{}'.", category, old_name, category, new_name);
        } else {
            trace!("No render cache found to rename for sheet '{:?}/{}'.", category, old_name);
        }
    }

    /// Ensures the render cache for a sheet has the correct dimensions,
    /// adding default `RenderableCellData` if rows/columns are missing.
    /// Returns a mutable reference to the cached grid.
    pub(crate) fn ensure_and_get_sheet_cache_mut(
        &mut self,
        category: &Option<String>,
        sheet_name: &String,
        num_rows: usize,
        num_cols: usize,
    ) -> &mut Vec<Vec<RenderableCellData>> {
        let key = (category.clone(), sheet_name.clone());
        let sheet_cache = self.states.entry(key).or_insert_with(Vec::new);

        // Ensure correct number of rows
        if sheet_cache.len() < num_rows {
            for _ in sheet_cache.len()..num_rows {
                sheet_cache.push(vec![RenderableCellData::default(); num_cols]);
            }
        } else if sheet_cache.len() > num_rows {
            sheet_cache.truncate(num_rows);
        }

        // Ensure correct number of columns for each row
        for row_cache in sheet_cache.iter_mut() {
            if row_cache.len() < num_cols {
                row_cache.resize_with(num_cols, RenderableCellData::default);
            } else if row_cache.len() > num_cols {
                row_cache.truncate(num_cols);
            }
        }
        sheet_cache
    }
}


// --- SheetRegistry definition (remains the same) ---
#[derive(Resource, Default, Debug)]
pub struct SheetRegistry {
    categorized_sheets: BTreeMap<Option<String>, HashMap<String, SheetGridData>>,
}

// --- SheetRegistry impl (remains the same) ---
impl SheetRegistry {
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
             names = category_map.keys()
                 .filter(|k| !k.starts_with("__virtual__"))
                 .cloned()
                 .collect();
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