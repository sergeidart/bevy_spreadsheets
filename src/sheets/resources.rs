// src/sheets/resources.rs
use bevy::prelude::*;
use std::collections::HashMap;

use super::definitions::{SheetGridData, SheetMetadata};

/// Core Resource holding all registered sheet data (definitions metadata + grid data).
/// Managed by systems within the `sheets` module.
/// Read by visual systems (e.g., editor UI).
#[derive(Resource, Default, Debug)]
pub struct SheetRegistry {
    sheets: HashMap<&'static str, SheetGridData>,
    sheet_names_sorted: Vec<&'static str>,
}

// Implementation only contains methods to access/modify the data. No other logic.
impl SheetRegistry {
    /// Registers sheet metadata. Should be called before loading data.
    /// Returns true if registration was successful (new sheet), false otherwise.
    pub fn register(&mut self, metadata: SheetMetadata) -> bool {
        let name = metadata.sheet_name;
        if !self.sheets.contains_key(name) {
            let mut data = SheetGridData::default();
            data.metadata = Some(metadata);
            self.sheets.insert(name, data);
            self.sheet_names_sorted.push(name);
            self.sheet_names_sorted.sort_unstable();
            true
        } else {
            false
        }
    }

    /// Returns a sorted list of registered sheet names.
    pub fn get_sheet_names(&self) -> &Vec<&'static str> {
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
    pub fn iter_sheets_mut(&mut self) -> impl Iterator<Item = (&'static str, &mut SheetGridData)> {
        self.sheets.iter_mut().map(|(name, data)| (*name, data))
    }

    /// Provides an immutable iterator over all registered sheets (name, data).
    pub fn iter_sheets(&self) -> impl Iterator<Item = (&'static str, &SheetGridData)> {
        self.sheets.iter().map(|(name, data)| (*name, data))
    }
}