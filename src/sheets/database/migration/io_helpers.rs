// src/sheets/database/migration/io_helpers.rs

use bevy::prelude::*;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::super::error::{DbError, DbResult};
use super::super::reader::DbReader;
use super::dependency_handler::DependencyHandler;
use crate::sheets::definitions::SheetMetadata;

#[derive(Debug, Clone)]
pub struct JsonSheetPair {
    pub name: String,
    pub data_path: PathBuf,
    pub meta_path: PathBuf,
    pub dependencies: Vec<String>,
    pub category: Option<String>,
}

pub struct IoHelpers;

impl IoHelpers {
    /// Scan folder for JSON pairs and their dependencies
    pub fn scan_json_folder(folder_path: &Path) -> DbResult<HashMap<String, JsonSheetPair>> {
        let mut sheets = HashMap::new();

        if !folder_path.exists() || !folder_path.is_dir() {
            return Err(DbError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Folder not found",
            )));
        }

        // Find all .json files (not .meta.json)
        for entry in std::fs::read_dir(folder_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".meta.json") {
                        continue;
                    }

                    if name.ends_with(".json") {
                        let sheet_name = name.trim_end_matches(".json").to_string();
                        let meta_path = path.with_file_name(format!("{}.meta.json", sheet_name));

                        if meta_path.exists() {
                            // Read metadata to find dependencies
                            let meta_content = std::fs::read_to_string(&meta_path)?;
                            let metadata: SheetMetadata = serde_json::from_str(&meta_content)
                                .map_err(|e| DbError::InvalidMetadata(e.to_string()))?;

                            let dependencies = DependencyHandler::find_linked_sheets(&metadata);

                            sheets.insert(
                                sheet_name.clone(),
                                JsonSheetPair {
                                    name: sheet_name,
                                    data_path: path,
                                    meta_path,
                                    dependencies,
                                    category: metadata.category.clone(),
                                },
                            );
                        }
                    }
                }
            }
        }

        Ok(sheets)
    }

    /// Export sheet from database to JSON
    pub fn export_sheet_to_json(
        conn: &Connection,
        table_name: &str,
        output_folder: &Path,
    ) -> DbResult<()> {
        let sheet_data = DbReader::read_sheet(conn, table_name)?;

        let metadata = sheet_data
            .metadata
            .ok_or_else(|| DbError::InvalidMetadata("No metadata found".into()))?;

        // Write data file
        let data_path = output_folder.join(format!("{}.json", table_name));
        let data_json = serde_json::to_string_pretty(&sheet_data.grid)?;
        std::fs::write(data_path, data_json)?;

        // Write metadata file
        let meta_path = output_folder.join(format!("{}.meta.json", table_name));
        let meta_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(meta_path, meta_json)?;

        info!("Exported '{}' to JSON", table_name);
        Ok(())
    }

    /// Load JSON metadata from a file
    pub fn load_metadata(meta_path: &Path) -> DbResult<SheetMetadata> {
        let meta_content = std::fs::read_to_string(meta_path)?;
        let metadata: SheetMetadata = serde_json::from_str(&meta_content)?;
        Ok(metadata)
    }

    /// Load JSON grid data from a file
    pub fn load_grid_data(data_path: &Path) -> DbResult<Vec<Vec<String>>> {
        let data_content = std::fs::read_to_string(data_path)?;
        let grid: Vec<Vec<String>> = serde_json::from_str(&data_content)?;
        Ok(grid)
    }
}
