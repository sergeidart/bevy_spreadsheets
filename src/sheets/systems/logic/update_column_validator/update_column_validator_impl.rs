// src/sheets/systems/logic/update_column_validator/update_column_validator_impl.rs
// Main implementation of column validator update logic

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator, SheetMetadata},
    events::{
        RequestSheetRevalidation, RequestUpdateColumnValidator, SheetDataModifiedInRegistryEvent,
        SheetOperationFeedback,
    },
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use std::collections::HashMap;

// Import from the new modular structure
use super::cell_population::{
    ensure_structure_cells_not_empty, handle_structure_conversion_from, populate_structure_rows,
};
use super::content_copy::copy_parent_content_to_structure_table;
use super::hierarchy::calculate_hierarchy_depth;
use super::structure_conversion::handle_structure_conversion_to;

/// Handles requests to update the validator (and derived base data type) for a specific column.
/// Supports the new Structure validator which snapshots selected source columns into a JSON object
/// stored directly in the target column cells as a string.
pub fn handle_update_column_validator(
    mut events: EventReader<RequestUpdateColumnValidator>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut revalidation_writer: EventWriter<RequestSheetRevalidation>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut editor_state: Option<ResMut<crate::ui::elements::editor::state::EditorWindowState>>,
) {
    // Track sheets whose metadata changed so we can save after loop with immutable borrow
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();
    // Track structure sheets that need to be created
    // Format: (category, structure_sheet_name, parent_sheet_name, parent_col_def, structure_columns)
    let mut structure_sheets_to_create: Vec<(
        Option<String>,
        String,
        String,
        crate::sheets::definitions::ColumnDefinition,
        Vec<crate::sheets::definitions::ColumnDefinition>,
    )> = Vec::new();

    for event in events.read() {
        let category = &event.category;
        let sheet_name = &event.sheet_name;
        let col_index = event.column_index;
        let new_validator_opt = &event.new_validator;

        info!(
            "RequestUpdateColumnValidator EVENT: col={}, validator={:?}, structure_source_columns={:?}, key_parent_column_index={:?}",
            col_index, new_validator_opt, event.structure_source_columns, event.key_parent_column_index
        );

        // --- Phase 1: Validation (immutable) ---
        let validation_result: Result<(), String> = (|| {
            let sheet_data = registry
                .get_sheet(category, sheet_name)
                .ok_or_else(|| format!("Sheet '{:?}/{}' not found.", category, sheet_name))?;
            let metadata = sheet_data
                .metadata
                .as_ref()
                .ok_or_else(|| "Metadata missing.".to_string())?;
            if col_index >= metadata.columns.len() {
                return Err(format!(
                    "Column index {} out of bounds ({} columns).",
                    col_index,
                    metadata.columns.len()
                ));
            }
            if let Some(v) = new_validator_opt {
                match v {
                    ColumnValidator::Basic(_) => {}
                    ColumnValidator::Linked {
                        target_sheet_name,
                        target_column_index,
                    } => {
                        // Look for target sheet anywhere (category-agnostic) for convenience
                        let mut found_sheet_meta = None;
                        for (_cat, name, data) in registry.iter_sheets() {
                            if name == target_sheet_name {
                                found_sheet_meta = data.metadata.as_ref();
                                break;
                            }
                        }
                        let target_meta = found_sheet_meta.ok_or_else(|| {
                            format!("Target sheet '{}' not found.", target_sheet_name)
                        })?;
                        if *target_column_index >= target_meta.columns.len() {
                            return Err(format!(
                                "Target column index {} out of bounds for sheet '{}' ({} columns).",
                                target_column_index,
                                target_sheet_name,
                                target_meta.columns.len()
                            ));
                        }
                        // Prevent linking to itself (same category, same sheet, same column)
                        if target_sheet_name == sheet_name
                            && *target_column_index == col_index
                            && target_meta.category == *category
                        {
                            return Err("Cannot link column to itself.".to_string());
                        }
                    }
                    ColumnValidator::Structure => { /* Schema validated separately when schema provided */
                    }
                }
            }
            Ok(())
        })();

        if let Err(err_msg) = validation_result {
            let msg = format!(
                "Failed validator update for col {} of sheet '{:?}/{}': {}",
                col_index + 1,
                category,
                sheet_name,
                err_msg
            );
            error!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
            continue;
        }

        // --- Phase 2: Apply (mutable) ---
        // Calculate hierarchy depth BEFORE getting mutable borrow to avoid borrow checker issues
        let parent_depth = calculate_hierarchy_depth(&registry, category, sheet_name);
        
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("HIERARCHY DEPTH CALCULATED: parent_depth = {}", parent_depth);
        info!("  Parent table: '{:?}/{}'", category, sheet_name);
        info!("  Child structure table will be at depth: {}", parent_depth + 1);
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        
        // (Structure schema handled elsewhere; no indices-based sources needed)
        let sheet_data_mut = match registry.get_sheet_mut(category, sheet_name) {
            Some(data) => data,
            None => {
                error!("Sheet '{:?}/{}' disappeared before apply phase.", category, sheet_name);
                continue;
            }
        };
        
        let meta_mut = match &mut sheet_data_mut.metadata {
            Some(meta) => meta,
            None => {
                error!("Metadata missing during apply phase for sheet '{:?}/{}'.", category, sheet_name);
                continue;
            }
        };
        
        if col_index >= meta_mut.columns.len() {
            // Should not happen after validation
            error!("Column index out of bounds during apply phase.");
            continue;
        }

        // Snapshot old column definition & cell values before mutating (needed if self is included as source)
                let old_col_def_snapshot = meta_mut.columns[col_index].clone();
                let old_validator = old_col_def_snapshot.validator.clone();
                let old_was_structure = matches!(old_validator, Some(ColumnValidator::Structure));
                let old_self_cells: Vec<String> = sheet_data_mut
                    .grid
                    .iter()
                    .map(|row| row.get(col_index).cloned().unwrap_or_default())
                    .collect();
                meta_mut.columns[col_index].validator = new_validator_opt.clone();
                // Derive data type
                let derived_type = match &meta_mut.columns[col_index].validator {
                    Some(ColumnValidator::Basic(t)) => *t,
                    Some(ColumnValidator::Linked { .. }) => ColumnDataType::String,
                    Some(ColumnValidator::Structure) => ColumnDataType::String,
                    None => ColumnDataType::String,
                };
                meta_mut.columns[col_index].data_type = derived_type;
                if matches!(
                    meta_mut.columns[col_index].validator,
                    Some(ColumnValidator::Structure)
                ) {
                    meta_mut.columns[col_index].ai_include_in_send = Some(false);
                }
                // New Structure variant: structure schema is stored in col_def.structure_schema (not handled here yet)

                // Feedback message (primary)
                let new_validator_ref = &meta_mut.columns[col_index].validator;
                let change_msg = match (&old_validator, new_validator_ref) {
                    (Some(o), Some(n)) => format!("Changed validator {:?} -> {:?}.", o, n),
                    (Some(o), None) => format!("Cleared validator {:?}.", o),
                    (None, Some(n)) => format!("Set validator {:?}.", n),
                    (None, None) => "Validator unchanged.".to_string(),
                };
                let base_msg = format!(
                    "Updated validator for column {} ('{}') in sheet '{:?}/{}': {} Base type now {:?}.",
                    col_index + 1,
                    meta_mut.columns[col_index].header.clone(),
                    category,
                    sheet_name,
                    change_msg,
                    meta_mut.columns[col_index].data_type
                );
                info!("{}", base_msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: base_msg,
                    is_error: false,
                });

                // Row population (if any) happens below; we'll clone metadata for saving after modifications.

                // If new validator is Structure: populate each row cell with JSON object of selected source columns
                if matches!(
                    meta_mut.columns[col_index].validator,
                    Some(ColumnValidator::Structure)
                ) {
                    info!(
                        "Structure validator detected for column {}. structure_schema.is_none()={}, structure_key_parent_column_index={:?}",
                        col_index,
                        meta_mut.columns[col_index].structure_schema.is_none(),
                        meta_mut.columns[col_index].structure_key_parent_column_index
                    );
                    
                    if meta_mut.columns[col_index].structure_schema.is_none() {
                        // Use pre-calculated parent_depth (calculated before mutable borrow)
                        if let Some((collected_defs, value_sources, structure_columns)) =
                            handle_structure_conversion_to(
                                &event,
                                col_index,
                                &old_col_def_snapshot,
                                &meta_mut.columns.clone(),
                                sheet_name,
                                &meta_mut.columns[col_index].header,
                                parent_depth,
                            )
                        {
                            meta_mut.columns[col_index].structure_schema =
                                Some(collected_defs.clone());
                            meta_mut.columns[col_index].structure_column_order =
                                Some((0..collected_defs.len()).collect());
                            if meta_mut.columns[col_index]
                                .structure_key_parent_column_index
                                .is_none()
                            {
                                if let Some(k) = event.key_parent_column_index {
                                    meta_mut.columns[col_index].structure_key_parent_column_index =
                                        Some(k);
                                }
                            }

                            // Mark structure sheet for creation
                            let structure_sheet_name =
                                format!("{}_{}", sheet_name, meta_mut.columns[col_index].header);

                            structure_sheets_to_create.push((
                                category.clone(),
                                structure_sheet_name,
                                sheet_name.clone(),
                                meta_mut.columns[col_index].clone(),
                                structure_columns,
                            ));

                            populate_structure_rows(
                                &mut sheet_data_mut.grid,
                                col_index,
                                &value_sources,
                                &old_self_cells,
                            );
                        } else {
                            warn!("handle_structure_conversion_to returned None - structure conversion failed or was skipped");
                            ensure_structure_cells_not_empty(&mut sheet_data_mut.grid, col_index);
                        }
                    } else {
                        info!(
                            "Skipping structure schema creation - schema already exists with {} fields",
                            meta_mut.columns[col_index].structure_schema.as_ref().map(|s| s.len()).unwrap_or(0)
                        );
                        // Ensure existing cells not empty
                        ensure_structure_cells_not_empty(&mut sheet_data_mut.grid, col_index);
                    }
                } else if old_was_structure
                    && !matches!(
                        meta_mut.columns[col_index].validator,
                        Some(ColumnValidator::Structure)
                    )
                {
                    handle_structure_conversion_from(
                        &mut sheet_data_mut.grid,
                        col_index,
                        &meta_mut.columns[col_index].header,
                        &mut feedback_writer,
                    );
                }
                // After any potential row mutations, record metadata clone for save
                // Emit data modified event so downstream systems (structure sync) run.
                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: sheet_name.clone(),
                });
                sheets_to_save.insert((category.clone(), sheet_name.clone()), meta_mut.clone());

                // Persist to DB if this sheet belongs to a database category
                if let Some(cat_str) = &meta_mut.category {
                    // Persist by column name to avoid index mismatch when metadata contains technical columns
                    let column_name = meta_mut.columns[col_index].header.clone();
                    
                    // Skip technical columns (row_index, parent_key) - they're not in metadata table
                    if column_name.eq_ignore_ascii_case("row_index") || column_name.eq_ignore_ascii_case("parent_key") {
                        // These are runtime-only columns, not persisted in metadata table
                        continue;
                    }
                    
                    let _ = crate::sheets::database::persist_column_validator_by_name(
                        cat_str,
                        sheet_name,
                        &column_name,
                        meta_mut.columns[col_index].data_type,
                        &meta_mut.columns[col_index].validator,
                        meta_mut.columns[col_index].ai_include_in_send,
                        meta_mut.columns[col_index].ai_enable_row_generation,
                    )
                    .map_err(|e| error!("Persist column validator failed: {}", e));
                }

        // Request revalidation (render cache rebuild etc.)
        revalidation_writer.write(RequestSheetRevalidation {
            category: category.clone(),
            sheet_name: sheet_name.clone(),
        });
        
        // Clear linked column caches to force rebuild with new validator
        if let Some(ref mut state) = editor_state {
            // Clear both regular and normalized caches for the affected sheet
            state.linked_column_cache.retain(|(sheet, _), _| {
                sheet != sheet_name
            });
            state.linked_column_cache_normalized.retain(|(sheet, _), _| {
                sheet != sheet_name
            });
            info!(
                "Cleared linked column cache for sheet '{}' after validator update",
                sheet_name
            );
        }
    }

    // --- Phase 2.5: Create structure sheets ---
    for (cat, struct_sheet_name, parent_sheet_name, parent_col_def, struct_columns) in structure_sheets_to_create {
        // Check if sheet already exists
        if registry.get_sheet(&cat, &struct_sheet_name).is_none() {
            info!("Creating structure sheet: {:?}/{}", cat, struct_sheet_name);

            let data_filename = format!("{}.json", struct_sheet_name);
            let structure_metadata = SheetMetadata::create_generic(
                struct_sheet_name.clone(),
                data_filename,
                struct_columns.len(),
                cat.clone(),
            );
            let structure_metadata = SheetMetadata {
                columns: struct_columns.clone(),
                hidden: true,
                ..structure_metadata
            };

            let structure_sheet_data = crate::sheets::definitions::SheetGridData {
                metadata: Some(structure_metadata.clone()),
                grid: Vec::new(), // Empty initially
                row_indices: Vec::new(), // JSON-backed sheets don't track row_indices
            };

            registry.add_or_replace_sheet(
                cat.clone(),
                struct_sheet_name.clone(),
                structure_sheet_data,
            );

            // Save the structure sheet (JSON) or create DB table (DB-backed)
            let registry_immut_temp = registry.as_ref();
            if structure_metadata.category.is_none() {
                // JSON-backed: save to file
                save_single_sheet(registry_immut_temp, &structure_metadata);
            } else if let Some(cat_str) = &cat {
                // DB-backed: create table
                info!("Creating DB table for structure sheet: {}", struct_sheet_name);
                let base_path = crate::sheets::systems::io::get_default_data_base_path();
                let db_path = base_path.join(format!("{}.db", cat_str));
                
                if db_path.exists() {
                    match rusqlite::Connection::open(&db_path) {
                        Ok(conn) => {
                            // Log what we're passing
                            info!(
                                "About to create structure table with parent_col_def: header='{}', structure_schema.len()={}, structure_key_parent_column_index={:?}, struct_columns.len()={}",
                                parent_col_def.header,
                                parent_col_def.structure_schema.as_ref().map(|s| s.len()).unwrap_or(0),
                                parent_col_def.structure_key_parent_column_index,
                                struct_columns.len()
                            );
                            
                            // Create the structure table with multi-level support, passing full column list
                            if let Err(e) = crate::sheets::database::schema::create_structure_table(
                                &conn,
                                &parent_sheet_name,
                                &parent_col_def,
                                Some(&struct_columns),
                            ) {
                                error!("Failed to create structure table '{}': {}", struct_sheet_name, e);
                            } else {
                                info!("Successfully created DB structure table: {}", struct_sheet_name);
                                // Ensure per-table metadata exists for the structure sheet so later renames find it
                                if let Err(e) = crate::sheets::database::schema::create_metadata_table(
                                    &conn,
                                    &struct_sheet_name,
                                    &structure_metadata,
                                ) {
                                    warn!(
                                        "Failed to create metadata table for structure '{}': {} (will continue)",
                                        struct_sheet_name,
                                        e
                                    );
                                }

                                // Best-effort cleanup: drop the parent table's physical column if it existed.
                                // Structure columns should not remain as physical columns on the parent table,
                                // otherwise orphan recovery will resurrect them on reload.
                                let _ = crate::sheets::database::writer::DbWriter::drop_physical_column_if_exists(
                                    &conn,
                                    &parent_sheet_name,
                                    &parent_col_def.header,
                                );
                                
                                // Copy content from parent table to structure table
                                if let Err(e) = copy_parent_content_to_structure_table(
                                    &conn,
                                    &parent_sheet_name,
                                    &struct_sheet_name,
                                    &parent_col_def,
                                    &struct_columns,
                                ) {
                                    error!("Failed to copy content to structure table '{}': {}", struct_sheet_name, e);
                                } else {
                                    info!("Successfully copied content to structure table: {}", struct_sheet_name);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to open database for structure table creation: {}", e);
                        }
                    }
                } else {
                    warn!("Database file not found for structure table creation: {:?}", db_path);
                }
            }

            data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                category: cat.clone(),
                sheet_name: struct_sheet_name.clone(),
            });
        }
    }

    // --- Phase 3: Saving (immutable borrow) ---
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!(
                "Validator updated for '{:?}/{}', triggering save.",
                cat, name
            );
            if metadata.category.is_none() {
                save_single_sheet(registry_immut, &metadata);
            }
        }
    }
}
