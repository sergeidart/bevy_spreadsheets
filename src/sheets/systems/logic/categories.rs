// src/sheets/systems/logic/categories.rs
use bevy::prelude::*;
use crate::sheets::{
    events::{RequestCreateCategory, RequestDeleteCategory, SheetOperationFeedback, RequestDeleteSheetFile, RequestCreateCategoryDirectory, RequestRenameCategory, RequestRenameCategoryDirectory},
    resources::{SheetRegistry},
    definitions::SheetMetadata,
};
use std::path::PathBuf;

/// Handles creating a new empty category (folder)
pub fn handle_create_category_request(
    mut events: EventReader<RequestCreateCategory>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut mkdir_writer: EventWriter<RequestCreateCategoryDirectory>,
) {
    for ev in events.read() {
        let name = ev.name.trim();
        if name.is_empty() {
            feedback.write(SheetOperationFeedback { message: "Category name cannot be empty".to_string(), is_error: true });
            continue;
        }
        // Basic validation: disallow path separators and reserved chars
        if name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            feedback.write(SheetOperationFeedback { message: format!("Invalid category name: '{}'", name), is_error: true });
            continue;
        }
        match registry.create_category(name.to_string()) {
            Ok(_) => {
                // Create directory on disk so the category persists across restarts
                mkdir_writer.write(RequestCreateCategoryDirectory { name: name.to_string() });
                feedback.write(SheetOperationFeedback { message: format!("Category '{}' created.", name), is_error: false });
            }
            Err(e) => {
                feedback.write(SheetOperationFeedback { message: e, is_error: true });
            }
        }
    }
}

/// Handles deleting a category and all of its sheets (and requesting file deletions)
pub fn handle_delete_category_request(
    mut events: EventReader<RequestDeleteCategory>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut file_delete_writer: EventWriter<RequestDeleteSheetFile>,
) {
    for ev in events.read() {
        let name = ev.name.trim();
        if name.is_empty() { continue; }
        // Capture metadata before deleting to know file paths
        let sheets_to_delete: Vec<(Option<String>, String, Option<SheetMetadata>)> = registry
            .iter_sheets()
            .filter(|(cat, _, _)| cat.as_deref() == Some(name))
            .map(|(cat, sheet, data)| (cat.clone(), sheet.clone(), data.metadata.clone()))
            .collect();

        let result = registry.delete_category(name);
        match result {
            Ok(_list) => {
                // Request file deletions based on captured metadata
                for (_cat, _sheet_name, meta_opt) in sheets_to_delete {
                    if let Some(meta) = meta_opt {
                        // base not needed here; deletion handler joins base path
                        // data file
                        let mut rel = PathBuf::new();
                        if let Some(cat) = &meta.category { rel.push(cat); }
                        rel.push(&meta.data_filename);
                        file_delete_writer.write(RequestDeleteSheetFile { relative_path: rel });
                        // meta file
                        let mut relm = PathBuf::new();
                        if let Some(cat) = &meta.category { relm.push(cat); }
                        relm.push(format!("{}.meta.json", meta.sheet_name));
                        file_delete_writer.write(RequestDeleteSheetFile { relative_path: relm });
                    }
                }
                feedback.write(SheetOperationFeedback { message: format!("Category '{}' deleted.", name), is_error: false });
            }
            Err(e) => {
                feedback.write(SheetOperationFeedback { message: e, is_error: true });
            }
        }
    }
}

/// Handles renaming a category (folder): registry update + directory rename request
pub fn handle_rename_category_request(
    mut events: EventReader<RequestRenameCategory>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut dir_rename: EventWriter<RequestRenameCategoryDirectory>,
) {
    for ev in events.read() {
        let old_name = ev.old_name.trim();
        let new_name = ev.new_name.trim();
        if old_name.is_empty() || new_name.is_empty() { continue; }
        if new_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            feedback.write(SheetOperationFeedback { message: format!("Invalid category name: '{}'", new_name), is_error: true });
            continue;
        }
        match registry.rename_category(old_name, new_name) {
            Ok(_) => {
                dir_rename.write(RequestRenameCategoryDirectory { old_name: old_name.to_string(), new_name: new_name.to_string() });
                feedback.write(SheetOperationFeedback { message: format!("Category '{}' renamed to '{}'.", old_name, new_name), is_error: false });
            }
            Err(e) => { feedback.write(SheetOperationFeedback { message: e, is_error: true }); },
        }
    }
}
