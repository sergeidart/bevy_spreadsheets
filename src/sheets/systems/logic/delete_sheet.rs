// src/sheets/systems/logic/delete_sheet.rs
use crate::sheets::{
    definitions::{ColumnValidator, SheetMetadata}, // Needed for path generation
    events::{RequestDeleteSheet, RequestDeleteSheetFile, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::get_default_data_base_path,
};
use bevy::prelude::*;
use std::path::PathBuf; // Added for relative path

/// Handles requests to delete a sheet from the registry and requests deletion of associated files.
pub fn handle_delete_request(
    mut events: EventReader<RequestDeleteSheet>,
    mut registry: ResMut<SheetRegistry>,
    mut file_delete_writer: EventWriter<RequestDeleteSheetFile>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<crate::sheets::events::SheetDataModifiedInRegistryEvent>,
    daemon_client: Res<crate::sheets::database::daemon_resource::SharedDaemonClient>,
) {
    for event in events.read() {
        let category = &event.category; // <<< Get category
        let sheet_name = &event.sheet_name;
        info!(
            "Handling delete request for sheet: '{:?}/{}'",
            category, sheet_name
        );

        // --- Get metadata BEFORE attempting delete ---
        // Need immutable borrow first to clone metadata if sheet exists
        let metadata_opt: Option<SheetMetadata> = {
            let registry_immut = registry.as_ref();
            registry_immut
                .get_sheet(category, sheet_name)
                .and_then(|d| d.metadata.clone()) // Clone metadata if present
        };

        // Check if sheet exists before attempting delete (using immutable borrow again)
        if metadata_opt.is_none() {
            let msg = format!(
                "Delete failed: Sheet '{:?}/{}' not found or missing metadata.",
                category, sheet_name
            );
            error!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
            continue; // Skip to next event
        }

        // --- Perform Delete in Registry (Mutable Borrow) ---
        // Use the category from the event
        match registry.delete_sheet(category, sheet_name) {
            Ok(removed_data) => {
                // Registry deletion returns the removed data
                let msg = format!(
                    "Successfully deleted sheet '{:?}/{}' from registry.",
                    category, sheet_name
                );
                info!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: false,
                });

                // Notify that sheet data changed so UI and caches can respond (clears transient UI feedback)
                data_modified_writer.write(
                    crate::sheets::events::SheetDataModifiedInRegistryEvent {
                        category: category.clone(),
                        sheet_name: sheet_name.clone(),
                    },
                );

                // --- Cascade delete: remove any child structure sheets belonging to this parent ---
                // We discover children in two ways:
                // 1) Column-derived naming convention: Parent_StructureColumn
                // 2) Metadata links:
                //    - DB mode: _Metadata.parent_table = sheet_name
                //    - JSON mode: SheetMetadata.structure_parent matches (category, sheet_name)
                let mut cascade_stack: Vec<(Option<String>, String)> = Vec::new();

                // 1) Column-derived immediate children
                if let Some(parent_meta) = &removed_data.metadata {
                    for col_def in parent_meta.columns.iter() {
                        if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                            let child_name = format!("{}_{}", sheet_name, col_def.header);
                            cascade_stack.push((category.clone(), child_name));
                        }
                    }
                }

                // 2) Metadata-linked children
                if let Some(ref db_name) = category {
                    // DB mode: query _Metadata for structures whose parent_table == sheet_name
                    let base_path = get_default_data_base_path();
                    let db_path = base_path.join(format!("{}.db", db_name));
                    if db_path.exists() {
                        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                            if let Ok(mut stmt) = conn.prepare(
                                "SELECT table_name FROM _Metadata WHERE table_type = 'structure' AND parent_table = ?"
                            ) {
                                if let Ok(iter) = stmt.query_map([sheet_name.as_str()], |row| row.get::<_, String>(0)) {
                                    for res in iter {
                                        if let Ok(child) = res { cascade_stack.push((category.clone(), child)); }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // JSON mode: scan registry for sheets with structure_parent matching this parent
                    for (cat_ref, name_ref, data_ref) in registry.iter_sheets() {
                        if let Some(meta) = &data_ref.metadata {
                            if let Some(parent_link) = &meta.structure_parent {
                                if parent_link.parent_sheet == *sheet_name
                                    && parent_link.parent_category == *category
                                {
                                    cascade_stack.push((cat_ref.clone(), name_ref.clone()));
                                }
                            }
                        }
                    }
                }

                while let Some((child_cat, child_name)) = cascade_stack.pop() {
                    // Capture child's metadata prior to deletion so we can cascade further
                    let child_meta_opt: Option<SheetMetadata> = registry
                        .get_sheet(&child_cat, &child_name)
                        .and_then(|d| d.metadata.clone());

                    // Remove child from registry
                    if let Ok(child_removed) = registry.delete_sheet(&child_cat, &child_name) {
                        info!(
                            "Cascade: removed child structure sheet '{:?}/{}' from registry.",
                            child_cat, child_name
                        );

                        // Notify so render cache clears entries for this sheet
                        data_modified_writer.write(
                            crate::sheets::events::SheetDataModifiedInRegistryEvent {
                                category: child_cat.clone(),
                                sheet_name: child_name.clone(),
                            },
                        );

                        // Database-backed: drop tables and clean metadata
                        if let Some(ref db_name) = child_cat {
                            let base_path = get_default_data_base_path();
                            let db_path = base_path.join(format!("{}.db", db_name));
                            if db_path.exists() {
                                match rusqlite::Connection::open(&db_path) {
                                    Ok(_conn) => {
                                        let meta_table = format!("{}_Metadata", child_name);
                                        let ai_groups_table = format!("{}_AIGroups", child_name);
                                        
                                        let statements = vec![
                                            crate::sheets::database::daemon_client::Statement {
                                                sql: format!("DROP TABLE IF EXISTS \"{}\"", child_name),
                                                params: vec![],
                                            },
                                            crate::sheets::database::daemon_client::Statement {
                                                sql: format!("DROP TABLE IF EXISTS \"{}\"", meta_table),
                                                params: vec![],
                                            },
                                            crate::sheets::database::daemon_client::Statement {
                                                sql: format!("DROP TABLE IF EXISTS \"{}\"", ai_groups_table),
                                                params: vec![],
                                            },
                                            crate::sheets::database::daemon_client::Statement {
                                                sql: "DELETE FROM _Metadata WHERE table_name = ?".to_string(),
                                                params: vec![serde_json::json!(child_name)],
                                            },
                                            crate::sheets::database::daemon_client::Statement {
                                                sql: "VACUUM".to_string(),
                                                params: vec![],
                                            },
                                        ];
                                        
                                        match daemon_client.client().exec_batch(statements) {
                                            Ok(response) => {
                                                if response.error.is_some() {
                                                    warn!("Daemon error cascade deleting child '{:?}/{}': {:?}", 
                                                          child_cat, child_name, response.error);
                                                } else {
                                                    info!("Cascade: cleaned DB objects and vacuumed for child '{:?}/{}'", 
                                                          child_cat, child_name);
                                                }
                                            }
                                            Err(e) => {
                                                warn!("Failed to execute cascade delete batch via daemon for '{:?}/{}': {:?}", 
                                                      child_cat, child_name, e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!(
                                            "Cascade: failed to open DB '{}' to drop child '{}': {}",
                                            db_name, child_name, e
                                        );
                                        feedback_writer.write(SheetOperationFeedback {
                                            message: format!(
                                                "Child sheet removed from memory but failed DB cleanup: {}",
                                                e
                                            ),
                                            is_error: true,
                                        });
                                    }
                                }
                            }
                        } else {
                            // Root-level: request JSON/meta deletion
                            if let Some(child_meta) = child_removed.metadata {
                                let mut json_rel = PathBuf::new();
                                if let Some(cat) = &child_meta.category {
                                    json_rel.push(cat);
                                }
                                json_rel.push(format!("{}.json", child_meta.sheet_name));

                                let mut meta_rel = PathBuf::new();
                                if let Some(cat) = &child_meta.category {
                                    meta_rel.push(cat);
                                }
                                meta_rel.push(format!("{}.meta.json", child_meta.sheet_name));

                                file_delete_writer.write(RequestDeleteSheetFile {
                                    relative_path: json_rel,
                                });
                                file_delete_writer.write(RequestDeleteSheetFile {
                                    relative_path: meta_rel,
                                });
                            }
                        }

                        feedback_writer.write(SheetOperationFeedback {
                            message: format!(
                                "Cascade deleted structure sheet '{:?}/{}'.",
                                child_cat, child_name
                            ),
                            is_error: false,
                        });

                        // If the child had its own structure columns, cascade further
                        if let Some(child_meta) = child_meta_opt {
                            for col_def in child_meta.columns.iter() {
                                if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                                    let grandchild_name =
                                        format!("{}_{}", child_name, col_def.header);
                                    cascade_stack.push((child_cat.clone(), grandchild_name));
                                }
                            }
                        }
                    } else {
                        // Not found in registry; skip
                        warn!(
                            "Cascade: attempted to delete missing child structure sheet '{:?}/{}'.",
                            child_cat, child_name
                        );
                    }
                }

                // --- Delete from database if in a category (database) ---
                if let Some(ref db_name) = category {
                    // This sheet is in a database - drop the table
                    let base_path = get_default_data_base_path();
                    let db_path = base_path.join(format!("{}.db", db_name));

                    if db_path.exists() {
                        match rusqlite::Connection::open(&db_path) {
                            Ok(_conn) => {
                                let meta_table = format!("{}_Metadata", sheet_name);
                                let ai_groups_table = format!("{}_AIGroups", sheet_name);
                                
                                let statements = vec![
                                    crate::sheets::database::daemon_client::Statement {
                                        sql: format!("DROP TABLE IF EXISTS \"{}\"", sheet_name),
                                        params: vec![],
                                    },
                                    crate::sheets::database::daemon_client::Statement {
                                        sql: format!("DROP TABLE IF EXISTS \"{}\"", meta_table),
                                        params: vec![],
                                    },
                                    crate::sheets::database::daemon_client::Statement {
                                        sql: format!("DROP TABLE IF EXISTS \"{}\"", ai_groups_table),
                                        params: vec![],
                                    },
                                    crate::sheets::database::daemon_client::Statement {
                                        sql: "DELETE FROM _Metadata WHERE table_name = ?".to_string(),
                                        params: vec![serde_json::json!(sheet_name)],
                                    },
                                    crate::sheets::database::daemon_client::Statement {
                                        sql: "VACUUM".to_string(),
                                        params: vec![],
                                    },
                                ];
                                
                                match daemon_client.client().exec_batch(statements) {
                                    Ok(response) => {
                                        if response.error.is_some() {
                                            error!("Daemon error deleting table '{}' from database '{}': {:?}", 
                                                   sheet_name, db_name, response.error);
                                        } else {
                                            info!("Successfully dropped table '{}', cleaned metadata, and vacuumed database '{}'", 
                                                  sheet_name, db_name);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to execute delete batch via daemon for table '{}': {:?}", 
                                               sheet_name, e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to open database '{}' for deletion: {}", db_name, e);
                                feedback_writer.write(SheetOperationFeedback {
                                    message: format!(
                                        "Sheet removed from memory but failed to open database: {}",
                                        e
                                    ),
                                    is_error: true,
                                });
                            }
                        }
                    } else {
                        warn!("Database file not found: {}", db_path.display());
                    }
                } else {
                    // Root-level sheet - delete JSON files (legacy support)
                    if let Some(metadata) = removed_data.metadata {
                        // Use metadata from the returned data
                        // Construct relative paths
                        let mut grid_relative_path = PathBuf::new();
                        if let Some(cat) = &metadata.category {
                            grid_relative_path.push(cat);
                        }
                        grid_relative_path.push(&metadata.data_filename);

                        let mut meta_relative_path = PathBuf::new();
                        if let Some(cat) = &metadata.category {
                            meta_relative_path.push(cat);
                        }
                        meta_relative_path.push(format!("{}.meta.json", metadata.sheet_name));

                        if !metadata.data_filename.is_empty() {
                            info!(
                                "Requesting grid file deletion: '{}'",
                                grid_relative_path.display()
                            );
                            file_delete_writer.write(RequestDeleteSheetFile {
                                relative_path: grid_relative_path,
                            });
                        } else {
                            warn!(
                                "No grid filename found in metadata for deleted sheet '{:?}/{}'.",
                                category, metadata.sheet_name
                            );
                        }
                        info!(
                            "Requesting meta file deletion: '{}'",
                            meta_relative_path.display()
                        );
                        file_delete_writer.write(RequestDeleteSheetFile {
                            relative_path: meta_relative_path,
                        });
                    } else {
                        // This case should ideally not happen if metadata_opt check passed
                        warn!("Cannot request file deletion for '{:?}/{}': Metadata was missing in removed data.", category, sheet_name);
                    }
                }
            }
            Err(e) => {
                // Error from registry.delete_sheet
                let msg = format!(
                    "Failed to delete sheet '{:?}/{}' from registry: {}",
                    category, sheet_name, e
                );
                error!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        }
    }
}
