// src/sheets/systems/logic/update_column_name.rs
use crate::sheets::{
    database::daemon_resource::SharedDaemonClient,
    definitions::SheetMetadata, // Need metadata for saving
    events::{RequestUpdateColumnName, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use std::collections::HashMap; // Keep HashMap

/// Handles requests to update the name of a specific column in a sheet's metadata.
pub fn handle_update_column_name(
    mut events: EventReader<RequestUpdateColumnName>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    daemon_client: Res<SharedDaemonClient>,
) {
    // Use map to track sheets needing save
    let mut changed_sheets: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let category = &event.category; // Get category
        let sheet_name = &event.sheet_name;
        let col_index = event.column_index;
        let new_name = event.new_name.trim(); // Trim whitespace

        let mut success = false; // Track if update was successful for this event
        let mut metadata_cache: Option<SheetMetadata> = None; // Cache metadata for saving

        // --- Validation ---
        if new_name.is_empty() {
            feedback_writer.write(SheetOperationFeedback {
                message: format!(
                    "Failed column rename in '{:?}/{}': New name cannot be empty.",
                    category, sheet_name
                ),
                is_error: true,
            });
            continue; // Skip to next event
        }
        // Note: We allow any characters in display name. Physical/DB-safe
        // sanitization applies only to logical headers enforced by DB path.

        // --- Resolve current metadata snapshot immutably (to avoid borrow conflicts) ---
        let (meta_snapshot, old_header, old_ui, is_structure, header_conflict, is_reserved, display_duplicate, db_backed) =
            if let Some(sheet_ro) = registry.get_sheet(category, sheet_name) {
                if let Some(meta_ro) = &sheet_ro.metadata {
                    if col_index >= meta_ro.columns.len() {
                        feedback_writer.write(SheetOperationFeedback {
                            message: format!(
                                "Failed column rename in '{:?}/{}': Index {} out of bounds ({} columns).",
                                category,
                                sheet_name,
                                col_index,
                                meta_ro.columns.len()
                            ),
                            is_error: true,
                        });
                        continue;
                    }
                    let col_ro = &meta_ro.columns[col_index];
                    let old_header = col_ro.header.clone();
                    let old_ui = col_ro
                        .display_header
                        .clone()
                        .unwrap_or_else(|| col_ro.header.clone());
                    let is_structure = matches!(
                        col_ro.validator,
                        Some(crate::sheets::definitions::ColumnValidator::Structure)
                    );
                    let header_conflict = meta_ro
                        .columns
                        .iter()
                        .enumerate()
                        .any(|(idx, c)| idx != col_index && !c.deleted && c.header.eq_ignore_ascii_case(new_name));
                    let is_reserved = new_name.eq_ignore_ascii_case("row_index")
                        || new_name.eq_ignore_ascii_case("parent_key");
                    let display_duplicate = meta_ro
                        .columns
                        .iter()
                        .enumerate()
                        .any(|(idx, c)| {
                            if idx == col_index || c.deleted {
                                return false;
                            }
                            let comp = c
                                .display_header
                                .as_ref()
                                .map(|s| s.as_str())
                                .unwrap_or(c.header.as_str());
                            comp.eq_ignore_ascii_case(new_name)
                        });
                    (
                        Some(meta_ro.clone()),
                        old_header,
                        old_ui,
                        is_structure,
                        header_conflict,
                        is_reserved,
                        display_duplicate,
                        meta_ro.category.is_some(),
                    )
                } else {
                    feedback_writer.write(SheetOperationFeedback {
                        message: format!(
                            "Failed column rename in '{:?}/{}': Metadata missing.",
                            category, sheet_name
                        ),
                        is_error: true,
                    });
                    continue;
                }
            } else {
                feedback_writer.write(SheetOperationFeedback {
                    message: format!(
                        "Failed column rename: Sheet '{:?}/{}' not found.",
                        category, sheet_name
                    ),
                    is_error: true,
                });
                continue;
            };

        // Block duplicate display names within the sheet
        if display_duplicate {
            feedback_writer.write(SheetOperationFeedback {
                message: format!(
                    "Failed column rename in '{:?}/{}': Name '{}' already exists.",
                    category, sheet_name, new_name
                ),
                is_error: true,
            });
            continue;
        }

        // Determine whether a real header rename is intended (vs display-only)
        let header_change_intended = !old_header.eq_ignore_ascii_case(new_name) && !header_conflict && !is_reserved;

        // --- DB-first path (when DB-backed) ---
        if db_backed {
            // Open the DB
            let cat = meta_snapshot.and_then(|m| m.category.clone()).unwrap();
            let base = crate::sheets::systems::io::get_default_data_base_path();
            let db_path = base.join(format!("{}.db", &cat));
            let conn = match crate::sheets::database::connection::DbConnection::open_existing(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    feedback_writer.write(SheetOperationFeedback {
                        message: format!(
                            "Cannot open database for category '{}': {}",
                            cat, e
                        ),
                        is_error: true,
                    });
                    continue;
                }
            };

            // If changing real header, attempt DB rename first; else update display name in DB first
            let db_filename = db_path.file_name().and_then(|n| n.to_str());
            let db_result = if header_change_intended {
                if is_structure {
                    crate::sheets::database::writer::DbWriter::rename_structure_and_parent_metadata_atomic(
                        &conn,
                        sheet_name,
                        &old_header,
                        new_name,
                        col_index,
                        db_filename,
                        daemon_client.client(),
                    )
                } else {
                    crate::sheets::database::writer::DbWriter::rename_data_column(
                        &conn,
                        sheet_name,
                        &old_header,
                        new_name,
                        db_filename,
                        daemon_client.client(),
                    )
                }
            } else {
                crate::sheets::database::writer::DbWriter::update_column_display_name(
                    &conn,
                    sheet_name,
                    col_index,
                    new_name,
                    db_filename,
                    daemon_client.client(),
                )
            };

            if let Err(e) = db_result {
                feedback_writer.write(SheetOperationFeedback {
                    message: format!(
                        "Rename failed in DB for '{}' -> '{}': {}. No changes applied in memory.",
                        old_header, new_name, e
                    ),
                    is_error: true,
                });
                continue;
            }

            // If a real rename was done, also update display_name in DB to match new_name (best-effort)
            if header_change_intended {
                let _ = crate::sheets::database::writer::DbWriter::update_column_display_name(
                    &conn,
                    sheet_name,
                    col_index,
                    new_name,
                    db_filename,
                    daemon_client.client(),
                );
            }
        }

        // --- In-memory mutation after DB success (or JSON mode) ---
        // Plan follow-up child registry fixes; fill only if we actually perform header change
        let mut planned_child_op: Option<(String, String, Option<usize>)> = None;

        if let Some(sheet_mut) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(metadata) = &mut sheet_mut.metadata {
                // Update display name in memory
                metadata.columns[col_index].display_header = Some(new_name.to_string());

                if header_change_intended {
                    // Update real header in memory
                    metadata.columns[col_index].header = new_name.to_string();

                    // If Structure: plan registry child rename (old -> new)
                    if is_structure {
                        let old_struct = format!("{}_{}", sheet_name, old_header);
                        let new_struct = format!("{}_{}", sheet_name, new_name);
                        planned_child_op = Some((
                            old_struct,
                            new_struct,
                            metadata.columns[col_index].structure_key_parent_column_index,
                        ));
                    }
                }

                // Feedback and events
                let msg = if header_change_intended {
                    format!(
                        "Renamed column {} in '{:?}/{}' from '{}' to '{}' in DB and memory.",
                        col_index + 1,
                        category,
                        sheet_name,
                        old_header,
                        new_name
                    )
                } else {
                    format!(
                        "Updated display name for column {} in '{:?}/{}' from '{}' to '{}'.",
                        col_index + 1,
                        category,
                        sheet_name,
                        old_ui,
                        new_name
                    )
                };
                info!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: false,
                });

                success = true;
                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: sheet_name.clone(),
                });
                metadata_cache = Some(metadata.clone());
            }
        }

        // After potential borrows end, apply planned child registry adjustments (only when header changed)
        if success {
            if let Some((child_old, child_new, parent_key_idx)) = planned_child_op.take() {
                // Prefer in-place rename when old exists and new does not
                let old_exists = registry.get_sheet(category, &child_old).is_some();
                let new_exists = registry.get_sheet(category, &child_new).is_some();
                if old_exists && !new_exists {
                    if let Err(e) = registry.rename_sheet(category, &child_old, child_new.clone()) {
                        warn!(
                            "Failed to rename child structure sheet in registry: {} -> {}: {}",
                            child_old, child_new, e
                        );
                    } else {
                        info!(
                            "Renamed child structure sheet in registry: {} -> {}",
                            child_old, child_new
                        );
                    }
                } else if old_exists && new_exists {
                    // Remove stale old entry to avoid duplicates
                    if let Err(e) = registry.delete_sheet(category, &child_old) {
                        warn!(
                            "Failed to delete stale child sheet '{}' after rename: {}",
                            child_old, e
                        );
                    } else {
                        info!("Deleted stale child sheet '{}' after rename", child_old);
                    }
                } else if !old_exists && !new_exists {
                    // Neither present in registry; try to read from DB and register the child
                    if let Some(cat_name) = category {
                        let base = crate::sheets::systems::io::get_default_data_base_path();
                        let db_path = base.join(format!("{}.db", cat_name));
                        if db_path.exists() {
                            match crate::sheets::database::connection::DbConnection::open_existing(&db_path) {
                                Ok(conn2) => match crate::sheets::database::reader::DbReader::read_sheet(&conn2, &child_new, daemon_client.client(), Some(cat_name)) {
                                    Ok(child_data) => {
                                        registry.add_or_replace_sheet(category.clone(), child_new.clone(), child_data);
                                        info!(
                                            "Loaded child structure sheet '{}' from DB into registry after rename",
                                            child_new
                                        );
                                    }
                                    Err(e) => warn!(
                                        "Failed to load child sheet '{}' from DB after rename: {:?}",
                                        child_new, e
                                    ),
                                },
                                Err(e) => warn!("Failed to open DB to load child sheet '{}': {}", child_new, e),
                            }
                        }
                    }
                }

                // Propagate parent's configured key mapping into child's parent_key metadata if available
                if let Some(child_sheet) = registry.get_sheet_mut(category, &child_new) {
                    if let Some(child_meta) = &mut child_sheet.metadata {
                        if let Some(pk_col) = child_meta
                            .columns
                            .iter_mut()
                            .find(|c| c.header.eq_ignore_ascii_case("parent_key"))
                        {
                            pk_col.structure_key_parent_column_index = parent_key_idx;
                        }
                    }
                }

                // Trigger cache update for the child sheet after registry changes
                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: child_new,
                });
            }
        }

        // If successful, mark sheet for saving
        if success {
            if let Some(meta) = metadata_cache {
                let key = (category.clone(), sheet_name.clone());
                changed_sheets.insert(key, meta);
            }
        }
    } // End event loop

    // --- Trigger Saves (Immutable Borrow) ---
    if !changed_sheets.is_empty() {
        let registry_immut = registry.as_ref(); // Get immutable borrow for saving
        for ((cat, name), metadata) in changed_sheets {
            info!(
                "Column name updated for '{:?}/{}', triggering save.",
                cat, name
            );
            save_single_sheet(registry_immut, &metadata); // Pass metadata
        }
    }
}



