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
        }

        if self.sheets.insert(name.clone(), data).is_none() {
            // If it was a new insertion, update the sorted list
            self.sheet_names_sorted.push(name);
            self.sheet_names_sorted.sort_unstable();
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
}