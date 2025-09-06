// src/sheets/systems/io/startup/load_registered.rs
use crate::sheets::{
    definitions::{SheetMetadata, ColumnValidator, StructureFieldDefinition, ColumnDataType},
    resources::SheetRegistry,
    events::RequestSheetRevalidation,
    systems::io::{
        get_default_data_base_path, get_full_metadata_path, get_full_sheet_path,
        parsers::read_and_parse_metadata_file,
        save::save_single_sheet,
        validator::{self, validate_file_exists},
        startup::grid_load,
    },
};
use bevy::prelude::*;
use std::path::Path;
use serde_json::{Value, Map};

pub fn load_data_for_registered_sheets(
    mut registry: ResMut<SheetRegistry>,
    mut revalidate_writer: EventWriter<RequestSheetRevalidation>,
) {
    info!("Startup Load: Loading data for registered sheets...");
    let base_path = get_default_data_base_path();

    if !base_path.exists() {
        info!("Startup Load: Data directory '{:?}' does not exist yet. Skipping load.", base_path);
        return;
    }

    let sheet_identifiers: Vec<(Option<String>, String)> = {
        let registry_immut = &*registry;
        registry_immut
            .iter_sheets()
            .map(|(cat, name, _)| (cat.clone(), name.clone()))
            .collect()
    };

    if sheet_identifiers.is_empty() {
        info!("Startup Load: No pre-registered sheets found to load.");
        return;
    } else {
        trace!("Startup Load: Found registered sheets: {:?}", sheet_identifiers);
    }

    let mut sheets_corrected_and_need_save = Vec::new();
    let mut sheets_loaded = Vec::new();

    for (category, sheet_name) in &sheet_identifiers {
        // Pass sheet_name (which is &String) directly here
        let (needs_correction_save, load_successful) = load_and_update_single_sheet_entry(
            category,
            sheet_name, // Pass &String here -> will be coerced to &str in the function call
            &mut registry,
            &base_path,
        );

        if load_successful {
            sheets_loaded.push((category.clone(), sheet_name.clone()));
            if needs_correction_save {
                let registry_immut_read = &*registry;
                if let Some(data) = registry_immut_read.get_sheet(category, sheet_name) {
                    if let Some(meta) = &data.metadata {
                        sheets_corrected_and_need_save.push(meta.clone());
                    }
                }
            }
        }
    }

    if !sheets_corrected_and_need_save.is_empty() {
        info!("Startup Load: Saving sheets that required metadata correction...");
        let registry_immut_save = &*registry;
        for metadata_to_save in sheets_corrected_and_need_save {
            save_single_sheet(registry_immut_save, &metadata_to_save);
        }
    }

    for (cat, name) in sheets_loaded {
        revalidate_writer.write(RequestSheetRevalidation {
            category: cat,
            sheet_name: name.to_owned(),
        });
    }

    info!("Startup Load: Finished loading data for registered sheets.");
}


fn load_and_update_single_sheet_entry(
    category: &Option<String>,
    sheet_name: &str, // Keep signature as &str
    registry: &mut SheetRegistry,
    base_path: &Path,
) -> (bool, bool) { // Returns (needs_save, load_successful)
    trace!("Startup Load: Processing registered sheet entry '{:?}/{}'", category, sheet_name);
    let mut loaded_metadata_opt: Option<SheetMetadata> = None;
    let mut final_grid_filename: Option<String>;
    let mut needs_save_after_correction = false;
    let load_successful;

    // ... (path finding logic remains the same) ...
    let (expected_meta_path_opt, _expected_grid_path_opt, initial_grid_filename_opt) = {
        let registry_immut = &*registry;
        registry_immut.get_sheet(category, sheet_name)
            .and_then(|sheet_data| sheet_data.metadata.as_ref())
            .map(|meta| {
                let meta_p = get_full_metadata_path(base_path, meta);
                let grid_p = get_full_sheet_path(base_path, meta);
                (Some(meta_p), Some(grid_p), Some(meta.data_filename.clone()))
            })
            .unwrap_or_else(|| {
                 warn!(
                     "Startup Load: Metadata missing in registry for '{:?}/{}' before loading.",
                     category, sheet_name
                 );
                 let mut meta_p = base_path.to_path_buf();
                 let mut grid_p = base_path.to_path_buf();
                 if let Some(cat) = category { meta_p.push(cat); grid_p.push(cat); }
                 let filename = format!("{}.json", sheet_name);
                 meta_p.push(format!("{}.meta.json", sheet_name));
                 grid_p.push(&filename);
                 (Some(meta_p), Some(grid_p), Some(filename))
            })
    };
    // Assign here after tuple destructure
    final_grid_filename = initial_grid_filename_opt;


     if let Some(meta_path) = &expected_meta_path_opt {
        if validate_file_exists(meta_path).is_ok() {
            match read_and_parse_metadata_file(meta_path) {
                Ok(mut loaded_meta) => {
                    let expected_grid_fn_str = final_grid_filename.as_deref().unwrap_or("");

                    // --- CORRECTED CALL SITE with .to_owned() ---
                    // Create an owned String from the &str sheet_name
                    let sheet_name_owned = sheet_name.to_owned();
                    // Pass the owned String to the validator function
                    match validator::validate_or_correct_loaded_metadata(
                        &mut loaded_meta,
                        &sheet_name_owned, // Pass the owned String
                        expected_grid_fn_str,
                        true,
                    ) {
                        Ok(()) => {
                            // Category check
                            if loaded_meta.category != *category {
                                warn!("Correcting category from '{:?}' to '{:?}' for '{}'", loaded_meta.category, category, sheet_name);
                                loaded_meta.category = category.clone();
                                needs_save_after_correction = true;
                            }
                            // Consistency check
                            if loaded_meta.ensure_column_consistency() {
                                info!("Corrected column consistency for loaded metadata of '{:?}/{}'.", category, sheet_name);
                                needs_save_after_correction = true;
                            }
                            final_grid_filename = Some(loaded_meta.data_filename.clone());
                            loaded_metadata_opt = Some(loaded_meta);
                        }
                        Err(e) => {
                            error!("Uncorrectable errors during metadata validation for '{:?}/{}': {}. Using registry metadata.", category, sheet_name, e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read/parse metadata file '{}': {}. Using registry metadata.", meta_path.display(), e);
                }
            }
        } else {
            trace!("Metadata file not found: {}", meta_path.display());
        }
    }

    let loaded_grid_data_opt: Option<Vec<Vec<String>>>;
    if let Some(grid_filename) = &final_grid_filename {
        if !grid_filename.is_empty() {
            let mut full_grid_path = base_path.to_path_buf();
            if let Some(cat_name) = category { full_grid_path.push(cat_name); }
            full_grid_path.push(grid_filename);
            match grid_load::load_grid_data_file(&full_grid_path) {
                Ok(grid_opt) => { loaded_grid_data_opt = grid_opt; }
                Err(e) => { error!("Failed to load grid file '{}': {}", full_grid_path.display(), e); return (needs_save_after_correction, false); }
            }
        } else { warn!("Skipping grid load for '{:?}/{}': Filename in metadata is empty.", category, sheet_name); loaded_grid_data_opt = Some(Vec::new()); }
    } else { warn!("Cannot load grid for '{:?}/{}': No data filename identified in metadata.", category, sheet_name); return (needs_save_after_correction, false); }


    // --- Update Registry Entry ---
    if let Some(sheet_entry) = registry.get_sheet_mut(category, sheet_name) {
         // Update metadata in registry if loaded_metadata_opt is Some
         if let Some(loaded_meta) = loaded_metadata_opt {
            sheet_entry.metadata = Some(loaded_meta);
            load_successful = true;
         } else if let Some(meta) = &mut sheet_entry.metadata {
             // If file meta wasn't loaded/valid, ensure existing registry meta is consistent
            if meta.ensure_column_consistency() { info!("Corrected column consistency for existing registry metadata for '{:?}/{}'.", category, sheet_name); needs_save_after_correction = true; }
             if meta.category != *category { warn!("Correcting category in existing registry metadata for '{}' from '{:?}' to '{:?}'.", sheet_name, meta.category, category); meta.category = category.clone(); needs_save_after_correction = true; }
             if Some(meta.data_filename.clone()) != final_grid_filename { warn!("Correcting filename in existing registry metadata for '{}' from '{}' to match determined filename ('{:?}').", sheet_name, meta.data_filename, final_grid_filename); meta.data_filename = final_grid_filename.clone().unwrap_or_default(); needs_save_after_correction = true; }
            load_successful = true;
         } else {
             // Generate default ONLY if registry entry has None metadata
             warn!("Generating default metadata for registered sheet '{:?}/{}' as none was found in registry.", category, sheet_name);
             let default_filename = final_grid_filename.clone().unwrap_or_else(|| format!("{}.json", sheet_name));
             let num_cols = loaded_grid_data_opt.as_ref().and_then(|g| g.first()).map_or(0, |r| r.len());
             sheet_entry.metadata = Some(SheetMetadata::create_generic( sheet_name.to_string(), default_filename, num_cols, category.clone(), ));
             needs_save_after_correction = true;
             load_successful = true;
         }

        // ... (Grid validation and update logic remains the same) ...
        let grid_validation_passed;
        if let (Some(grid), Some(meta)) = (&loaded_grid_data_opt, sheet_entry.metadata.as_ref()) {
             if meta.columns.is_empty() && !grid.is_empty() && grid.iter().any(|r| !r.is_empty()) { warn!("Grid structure validation skipped for '{:?}/{}': Metadata has no columns, but grid data exists.", category, sheet_name); grid_validation_passed = true; }
             else if !meta.columns.is_empty() {
                 match validator::validate_grid_structure(grid, meta, sheet_name) {
                     Ok(()) => { grid_validation_passed = true; }
                     Err(e) => { warn!("Grid structure validation failed for '{:?}/{}': {}. Allowing load.", category, sheet_name, e); grid_validation_passed = true; }
                 }
             } else { grid_validation_passed = true; }
        } else if loaded_grid_data_opt.is_some() { warn!("Cannot validate grid structure for '{:?}/{}': Metadata unavailable.", category, sheet_name); grid_validation_passed = true; }
        else { grid_validation_passed = true; }

        if grid_validation_passed {
            if let Some(grid_data) = loaded_grid_data_opt { sheet_entry.grid = grid_data; trace!("Updated grid data for '{:?}/{}'.", category, sheet_name); }
            else if final_grid_filename.is_some() { sheet_entry.grid = Vec::new(); trace!("Set empty grid data for '{:?}/{}' as file was empty.", category, sheet_name); }
        } else if loaded_grid_data_opt.is_some() { warn!("Skipping grid update for '{:?}/{}' due to structure validation failure.", category, sheet_name); }

        // Reconcile inline structure schema (legacy structures_meta removed)
        if let Some(meta) = &mut sheet_entry.metadata {
            let mut changed_local = false;
            for (col_index, col) in meta.columns.iter_mut().enumerate() {
                if matches!(col.validator, Some(ColumnValidator::Structure)) && col.structure_schema.is_none() {
                    // Derive basic schema from first non-empty cell (headers not inferable reliably otherwise)
                    if let Some(first_row) = sheet_entry.grid.iter().find(|r| r.get(col_index).map(|c| !c.trim().is_empty()).unwrap_or(false)) {
                        let cell = first_row.get(col_index).unwrap();
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(cell) {
                            if let serde_json::Value::Array(arr) = val {
                                if let Some(serde_json::Value::Array(inner)) = arr.first() {
                                    col.structure_schema = Some(inner.iter().enumerate().map(|(i, _)| StructureFieldDefinition {
                                        header: format!("Field{}", i+1),
                                        validator: Some(ColumnValidator::Basic(ColumnDataType::String)),
                                        data_type: ColumnDataType::String,
                                        filter: None,
                                        ai_context: None,
                                        width: None,
                                        structure_schema: None,
                                        structure_column_order: None,
                                        structure_key_parent_column_index: None,
                                        structure_ancestor_key_parent_column_indices: None,
                                    }).collect());
                                    col.structure_column_order = col.structure_schema.as_ref().map(|s| (0..s.len()).collect());
                                    changed_local = true;
                                }
                            }
                        }
                    }
                }
            }
            if changed_local { needs_save_after_correction = true; }
        }

        // Final trace log (after reconciliation)
        let final_metadata_status = match &sheet_entry.metadata {
            Some(meta) => format!("Metadata Some(cols: {}, file: '{}')", meta.columns.len(), meta.data_filename),
            None => "Metadata None".to_string(),
        };
        trace!( "Startup Load: Final state for '{:?}/{}': {}, Grid Rows: {}", category, sheet_name, final_metadata_status, sheet_entry.grid.len() );

    } else {
        error!("Sheet '{:?}/{}' not found in registry during mutable update.", category, sheet_name);
        load_successful = false;
    }

    (needs_save_after_correction, load_successful)
}

/// Reconcile structure column metadata & normalize structure cell JSON representations.
/// Returns true if any modifications were applied that should trigger a save.
// Legacy reconciliation function removed (structures_meta obsolete)

/// Derive a simple schema from existing cell values in a column.
fn derive_schema_from_cells(grid: &Vec<Vec<String>>, col_index: usize) -> Option<Vec<StructureFieldDefinition>> {
    for row in grid.iter() {
        if row.len() <= col_index { continue; }
        let cell = row[col_index].trim();
        if cell.is_empty() { continue; }
        if let Ok(val) = serde_json::from_str::<Value>(cell) {
            let mut headers: Vec<String> = Vec::new();
            match val {
                Value::Array(arr) => {
                    if let Some(Value::Object(first_obj)) = arr.first() { headers = first_obj.keys().cloned().collect(); }
                }
                Value::Object(obj) => { headers = obj.keys().cloned().collect(); }
                _ => {}
            }
            if headers.is_empty() { continue; }
            headers.sort();
            let fields: Vec<StructureFieldDefinition> = headers.into_iter().map(|h| StructureFieldDefinition {
                header: h,
                validator: Some(ColumnValidator::Basic(ColumnDataType::String)),
                data_type: ColumnDataType::String,
                filter: None,
                ai_context: None,
                width: Some(140.0),
                structure_schema: None,
                structure_column_order: None,
                structure_key_parent_column_index: None,
                structure_ancestor_key_parent_column_indices: None,
            }).collect();
            if !fields.is_empty() { return Some(fields); }
        }
    }
    None
}