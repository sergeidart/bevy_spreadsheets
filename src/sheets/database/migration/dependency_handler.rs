// src/sheets/database/migration/dependency_handler.rs

use std::collections::{HashMap, HashSet};

use crate::sheets::definitions::{ColumnValidator, SheetMetadata};

use super::io_helpers::JsonSheetPair;

pub struct DependencyHandler;

impl DependencyHandler {
    /// Find all linked sheets referenced in metadata
    pub fn find_linked_sheets(metadata: &SheetMetadata) -> Vec<String> {
        let mut linked = HashSet::new();

        for col in &metadata.columns {
            if let Some(ColumnValidator::Linked {
                target_sheet_name, ..
            }) = &col.validator
            {
                linked.insert(target_sheet_name.clone());
            }
        }

        linked.into_iter().collect()
    }

    /// Order sheets so dependencies are migrated first
    pub fn order_sheets_by_dependency(sheets: &HashMap<String, JsonSheetPair>) -> Vec<String> {
        let mut ordered = Vec::new();
        let mut visited = HashSet::new();

        fn visit(
            name: &str,
            sheets: &HashMap<String, JsonSheetPair>,
            visited: &mut HashSet<String>,
            ordered: &mut Vec<String>,
        ) {
            if visited.contains(name) {
                return;
            }

            visited.insert(name.to_string());

            if let Some(pair) = sheets.get(name) {
                // Visit dependencies first
                for dep in &pair.dependencies {
                    if sheets.contains_key(dep) {
                        visit(dep, sheets, visited, ordered);
                    }
                }
            }

            ordered.push(name.to_string());
        }

        for name in sheets.keys() {
            visit(name, sheets, &mut visited, &mut ordered);
        }

        ordered
    }
}
