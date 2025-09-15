// src/sheets/systems/logic/move_sheet.rs
use bevy::prelude::*;
use std::path::PathBuf;
use crate::sheets::{
    events::{RequestMoveSheetToCategory, RequestRenameSheetFile, SheetOperationFeedback, RequestSheetRevalidation},
    resources::{SheetRegistry, SheetRenderCache},
};

// Contract:
// - Input: RequestMoveSheetToCategory { from_category, to_category, sheet_name }
// - Effect: Move sheet in registry from -> to, update metadata.category, save, and rename both grid/meta files accordingly.
// - Error modes: sheet not found, moving to same place, name conflict in destination, invalid names.

pub fn handle_move_sheet_to_category_request(
    mut events: EventReader<RequestMoveSheetToCategory>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut file_rename: EventWriter<RequestRenameSheetFile>,
    mut revalidate: EventWriter<RequestSheetRevalidation>,
    mut render_cache: ResMut<SheetRenderCache>,
) {
    for ev in events.read() {
        let from = &ev.from_category;
        let to = &ev.to_category;
        let name = &ev.sheet_name;

        if from == to {
            feedback.write(SheetOperationFeedback { message: "Sheet is already in this category.".to_string(), is_error: true });
            continue;
        }

        // Pre-check destination name conflict within destination category
        if registry.get_sheet(to, name).is_some() {
            feedback.write(SheetOperationFeedback { message: format!("A sheet named '{}' already exists in the destination.", name), is_error: true });
            continue;
        }

        // Extract existing data
        let data = match registry.delete_sheet(from, name) {
            Ok(d) => d,
            Err(e) => { feedback.write(SheetOperationFeedback { message: e, is_error: true }); continue; }
        };

        // Update metadata to destination category
        let mut moved = data;
        if let Some(meta) = &mut moved.metadata {
            meta.category = to.clone();
        } else {
            feedback.write(SheetOperationFeedback { message: format!("Internal error: '{}' missing metadata during move.", name), is_error: true });
            // Try to reinsert back to original location to avoid loss
            registry.add_or_replace_sheet(from.clone(), name.clone(), moved);
            continue;
        }

    // Insert into destination
        registry.add_or_replace_sheet(to.clone(), name.clone(), moved.clone());

    // Clear render cache entries for old and new locations and request a rebuild
    render_cache.clear_sheet_render_data(from, name);
    render_cache.clear_sheet_render_data(to, name);
    revalidate.write(RequestSheetRevalidation { category: to.clone(), sheet_name: name.clone() });

        // Compute file renames for grid and meta
        if let Some(meta) = &moved.metadata {
            let new_grid_fn = meta.data_filename.clone();
            let old_meta_fn = format!("{}.meta.json", name);
            let new_meta_fn = format!("{}.meta.json", name);

            let mut old_grid_rel = PathBuf::new();
            let mut new_grid_rel = PathBuf::new();
            let mut old_meta_rel = PathBuf::new();
            let mut new_meta_rel = PathBuf::new();

            if let Some(ref from_cat) = from { old_grid_rel.push(from_cat); old_meta_rel.push(from_cat); }
            if let Some(ref to_cat) = to { new_grid_rel.push(to_cat); new_meta_rel.push(to_cat); }

            // Old grid filename comes from old metadata; we didn't keep a copy. Assume filename same as current but path differs.
            // Since rename within same name only changes directory, use current data_filename as old as well.
            old_grid_rel.push(new_grid_fn.clone());
            new_grid_rel.push(new_grid_fn.clone());

            old_meta_rel.push(old_meta_fn.clone());
            new_meta_rel.push(new_meta_fn.clone());

            if old_grid_rel != new_grid_rel {
                file_rename.write(RequestRenameSheetFile { old_relative_path: old_grid_rel, new_relative_path: new_grid_rel });
            }
            if old_meta_rel != new_meta_rel {
                file_rename.write(RequestRenameSheetFile { old_relative_path: old_meta_rel, new_relative_path: new_meta_rel });
            }
        }

        feedback.write(SheetOperationFeedback { message: format!("Moved sheet '{}' to {:?}.", name, to.clone().unwrap_or_else(|| "root".into())), is_error: false });
    }
}
