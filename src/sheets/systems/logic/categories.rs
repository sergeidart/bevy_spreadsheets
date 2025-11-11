// src/sheets/systems/logic/categories.rs
use crate::sheets::{
    database::connection::DbConnection,
    database::daemon_resource::SharedDaemonClient,
    events::{
        RequestCreateCategory, RequestDeleteCategory,
        RequestRenameCategory, SheetOperationFeedback,
    },
    resources::SheetRegistry,
    systems::io::get_default_data_base_path,
};
use bevy::prelude::*;

/// Handles creating a new empty category (database file)
pub fn handle_create_category_request(
    mut events: EventReader<RequestCreateCategory>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    daemon_client: Res<SharedDaemonClient>,
) {
    for ev in events.read() {
        let name = ev.name.trim();
        if name.is_empty() {
            feedback.write(SheetOperationFeedback {
                message: "Database name cannot be empty".to_string(),
                is_error: true,
            });
            continue;
        }
        // Basic validation: disallow path separators and reserved chars
        if name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            feedback.write(SheetOperationFeedback {
                message: format!("Invalid database name: '{}'", name),
                is_error: true,
            });
            continue;
        }

        // Create database file on disk
        let base_path = get_default_data_base_path();
        let db_filename = format!("{}.db", name);
        let db_path = base_path.join(&db_filename);

        if db_path.exists() {
            feedback.write(SheetOperationFeedback {
                message: format!("Database '{}' already exists", name),
                is_error: true,
            });
            continue;
        }

        // Ensure base directory exists
        if let Err(e) = std::fs::create_dir_all(&base_path) {
            error!("Failed to create base directory: {}", e);
            feedback.write(SheetOperationFeedback {
                message: format!("Failed to create base directory: {}", e),
                is_error: true,
            });
            continue;
        }

        // Create empty database with initialized schema
        match DbConnection::create_new(&db_path, daemon_client.client()) {
            Ok(_conn) => {
                info!("Created new database file: {}", db_path.display());

                // Register in memory
                match registry.create_category(name.to_string()) {
                    Ok(_) => {
                        feedback.write(SheetOperationFeedback {
                            message: format!("Database '{}' created successfully", name),
                            is_error: false,
                        });
                    }
                    Err(e) => {
                        // Database file created but registry failed - clean up
                        let _ = std::fs::remove_file(&db_path);
                        feedback.write(SheetOperationFeedback {
                            message: format!("Failed to register database: {}", e),
                            is_error: true,
                        });
                    }
                }
            }
            Err(e) => {
                error!(
                    "Failed to create database file '{}': {}",
                    db_path.display(),
                    e
                );
                feedback.write(SheetOperationFeedback {
                    message: format!("Failed to create database: {}", e),
                    is_error: true,
                });
            }
        }
    }
}

/// Handles deleting a category (database file) and all of its sheets
pub fn handle_delete_category_request(
    mut events: EventReader<RequestDeleteCategory>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
) {
    for ev in events.read() {
        let name = ev.name.trim();
        if name.is_empty() {
            continue;
        }

        // Delete from registry first
        let result = registry.delete_category(name);
        match result {
            Ok(_list) => {
                // Delete database file from disk
                let base_path = get_default_data_base_path();
                let db_filename = format!("{}.db", name);
                let db_path = base_path.join(&db_filename);

                if db_path.exists() {
                    match std::fs::remove_file(&db_path) {
                        Ok(_) => {
                            info!("Deleted database file: {}", db_path.display());
                            feedback.write(SheetOperationFeedback {
                                message: format!("Database '{}' deleted successfully", name),
                                is_error: false,
                            });
                        }
                        Err(e) => {
                            error!(
                                "Failed to delete database file '{}': {}",
                                db_path.display(),
                                e
                            );
                            feedback.write(SheetOperationFeedback {
                                message: format!(
                                    "Database deleted from memory but file removal failed: {}",
                                    e
                                ),
                                is_error: true,
                            });
                        }
                    }
                } else {
                    warn!("Database file not found: {}", db_path.display());
                    feedback.write(SheetOperationFeedback {
                        message: format!(
                            "Database '{}' removed from memory (file not found on disk)",
                            name
                        ),
                        is_error: false,
                    });
                }
            }
            Err(e) => {
                feedback.write(SheetOperationFeedback {
                    message: e,
                    is_error: true,
                });
            }
        }
    }
}

/// Handles renaming a category (database file): registry update + file rename
pub fn handle_rename_category_request(
    mut events: EventReader<RequestRenameCategory>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    daemon_client: Res<SharedDaemonClient>,
) {
    for ev in events.read() {
        let old_name = ev.old_name.trim();
        let new_name = ev.new_name.trim();
        if old_name.is_empty() || new_name.is_empty() {
            continue;
        }
        if new_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            feedback.write(SheetOperationFeedback {
                message: format!("Invalid database name: '{}'", new_name),
                is_error: true,
            });
            continue;
        }

        // Check if new database file already exists
        let base_path = get_default_data_base_path();
        let new_db_path = base_path.join(format!("{}.db", new_name));

        if new_db_path.exists() {
            feedback.write(SheetOperationFeedback {
                message: format!("Database '{}' already exists", new_name),
                is_error: true,
            });
            continue;
        }

        match registry.rename_category(old_name, new_name) {
            Ok(_) => {
                // Rename database file on disk with proper daemon coordination
                let old_db_path = base_path.join(format!("{}.db", old_name));
                let old_db_filename = format!("{}.db", old_name);
                let new_db_filename = format!("{}.db", new_name);

                if old_db_path.exists() {
                    // Use daemon's safe file operation helper
                    let client = daemon_client.client();
                    let rename_result = client.with_safe_file_operation(
                        Some(&old_db_filename),
                        || std::fs::rename(&old_db_path, &new_db_path),
                        Some(&new_db_filename)
                    );
                    
                    match rename_result {
                        Ok(_) => {
                            info!(
                                "Renamed database file from '{}' to '{}'",
                                old_db_path.display(),
                                new_db_path.display()
                            );
                            
                            feedback.write(SheetOperationFeedback {
                                message: format!(
                                    "Database '{}' renamed to '{}'",
                                    old_name, new_name
                                ),
                                is_error: false,
                            });
                        }
                        Err(e) => {
                            error!("Failed to rename database file: {}", e);
                            
                            // Rollback registry change
                            let _ = registry.rename_category(new_name, old_name);
                            feedback.write(SheetOperationFeedback {
                                message: format!("Failed to rename database file: {}", e),
                                is_error: true,
                            });
                        }
                    }
                } else {
                    warn!("Old database file not found: {}", old_db_path.display());
                    feedback.write(SheetOperationFeedback {
                        message: format!("Database renamed in memory but file not found on disk"),
                        is_error: true,
                    });
                }
            }
            Err(e) => {
                feedback.write(SheetOperationFeedback {
                    message: e,
                    is_error: true,
                });
            }
        }
    }
}
