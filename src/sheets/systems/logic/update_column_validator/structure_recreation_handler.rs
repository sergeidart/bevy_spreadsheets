// src/sheets/systems/logic/update_column_validator/structure_recreation_handler.rs
// Handler for structure table recreation with user-selected strategy

use crate::sheets::{
    definitions::ColumnValidator,
    events::{
        RequestStructureTableRecreation, RequestSheetRevalidation, SheetDataModifiedInRegistryEvent, SheetOperationFeedback,
        StructureRecreationStrategy,
    },
    resources::SheetRegistry,
    database::daemon_resource::SharedDaemonClient,
};
use bevy::prelude::*;

use super::content_copy::copy_parent_content_to_structure_table;

/// Handles structure table recreation requests based on user-selected strategy.
pub fn handle_structure_table_recreation(
    mut events: EventReader<RequestStructureTableRecreation>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut revalidation_writer: EventWriter<RequestSheetRevalidation>,
    daemon_client: Res<SharedDaemonClient>,
) {
    for event in events.read() {
        info!(
            "Processing structure table recreation: '{:?}/{}' with strategy {:?}",
            event.category, event.structure_sheet_name, event.strategy
        );

        match event.strategy {
            StructureRecreationStrategy::Cancel => {
                feedback_writer.write(SheetOperationFeedback {
                    message: format!(
                        "Cancelled structure table creation for '{}'",
                        event.structure_sheet_name
                    ),
                    is_error: false,
                });
                continue;
            }
            StructureRecreationStrategy::CleanStart => {
                info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                info!("🧹 CLEAN START: Step 1/3 - Deleting existing table '{}'", event.structure_sheet_name);
                info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                
                // Remove from registry
                if let Err(e) = registry.delete_sheet(&event.category, &event.structure_sheet_name) {
                    error!("Failed to delete sheet from registry: {}", e);
                }
                
                // Drop physical table if DB-backed - use daemon for proper synchronization
                if let Some(cat_str) = &event.category {
                    let db_name = Some(format!("{}.db", cat_str));
                    
                    // Drop main table using daemon
                    if let Err(e) = crate::sheets::database::schema::writer::drop_table(
                        &event.structure_sheet_name,
                        daemon_client.client(),
                        db_name.as_deref(),
                    ) {
                        error!("Failed to drop table '{}': {}", event.structure_sheet_name, e);
                    }
                    
                    // Drop metadata table using daemon
                    let meta_table_name = format!("{}_Metadata", event.structure_sheet_name);
                    if let Err(e) = crate::sheets::database::schema::writer::drop_table(
                        &meta_table_name,
                        daemon_client.client(),
                        db_name.as_deref(),
                    ) {
                        error!("Failed to drop metadata table '{}': {}", meta_table_name, e);
                    }
                    
                    // CRITICAL: Delete from global _Metadata registry to prevent orphan detection
                    // ARCHITECTURE: Use daemon for write operation to ensure WAL consistency
                    use crate::sheets::database::daemon_client::Statement;
                    let stmt = Statement {
                        sql: "DELETE FROM _Metadata WHERE table_name = ?".to_string(),
                        params: vec![serde_json::json!(event.structure_sheet_name)],
                    };
                    if let Err(e) = daemon_client.client().exec_batch(vec![stmt], db_name.as_deref()) {
                        error!("Failed to delete '{}' from _Metadata: {}", event.structure_sheet_name, e);
                    } else {
                        info!("Deleted '{}' from _Metadata registry via daemon", event.structure_sheet_name);
                    }
                    
                    info!("Dropped tables '{}' and '{}' via daemon", event.structure_sheet_name, meta_table_name);
                    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    info!("✅ CLEAN START: Step 1/3 Complete - Old table deleted");
                    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                }
                
                feedback_writer.write(SheetOperationFeedback {
                    message: format!("Deleted existing table '{}'", event.structure_sheet_name),
                    is_error: false,
                });
            }
            StructureRecreationStrategy::CarefulRecreation => {
                info!("Careful recreation: Updating schema for '{}'", event.structure_sheet_name);
                
                // For now, just update metadata without modifying data
                // In the future, could implement schema migration logic here
                if let Some(sheet_data) = registry.get_sheet_mut(&event.category, &event.structure_sheet_name) {
                    if let Some(ref mut metadata) = sheet_data.metadata {
                        // Update column definitions to match new schema
                        metadata.columns = event.structure_columns.clone();
                        
                        feedback_writer.write(SheetOperationFeedback {
                            message: format!(
                                "Updated schema for '{}' (data preserved)",
                                event.structure_sheet_name
                            ),
                            is_error: false,
                        });
                        
                        data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                            category: event.category.clone(),
                            sheet_name: event.structure_sheet_name.clone(),
                        });
                        
                        // CRITICAL: Persist Structure validator for parent column
                        // Even though we're not recreating the table, we need to persist the validator
                        // so it survives cache reloads and app restarts
                        info!("📝 Persisting Structure validator for parent column '{}' (Careful Recreation)", event.parent_col_def.header);
                        
                        if let Some(cat_str) = event.category.as_deref() {
                            if let Err(e) = crate::sheets::database::persist_column_validator_by_name(
                                cat_str,
                                &event.parent_sheet_name,
                                &event.parent_col_def.header,
                                event.parent_col_def.data_type,
                                &Some(ColumnValidator::Structure),
                                event.parent_col_def.ai_include_in_send,
                                event.parent_col_def.ai_enable_row_generation,
                                daemon_client.client(),
                            ) {
                                error!("❌ Failed to persist Structure validator for parent column '{}': {}", event.parent_col_def.header, e);
                            } else {
                                info!("✅ Successfully persisted Structure validator for parent column '{}'", event.parent_col_def.header);
                            }
                        }
                        
                        continue;
                    }
                }
            }
        }

        // After Cancel: do nothing
        // After CleanStart: table dropped, fall through to creation
        // After CarefulRecreation: if we got here, table doesn't exist in registry (shouldn't happen), fall through
        
        if event.strategy != StructureRecreationStrategy::Cancel {
            if event.strategy == StructureRecreationStrategy::CleanStart {
                info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                info!("🔨 CLEAN START: Step 2/3 - Creating new table and copying data");
                info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            }
            // Create the structure table
            create_structure_table_impl(
                &mut registry,
                &event.category,
                &event.structure_sheet_name,
                &event.parent_sheet_name,
                &event.parent_col_def,
                &event.structure_columns,
                &mut feedback_writer,
                &mut data_modified_writer,
                &mut revalidation_writer,
                &daemon_client,
            );
        }
    }
}

/// Internal implementation for creating structure table (shared by both strategies).
fn create_structure_table_impl(
    registry: &mut SheetRegistry,
    category: &Option<String>,
    struct_sheet_name: &str,
    parent_sheet_name: &str,
    parent_col_def: &crate::sheets::definitions::ColumnDefinition,
    struct_columns: &[crate::sheets::definitions::ColumnDefinition],
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
    data_modified_writer: &mut EventWriter<SheetDataModifiedInRegistryEvent>,
    revalidation_writer: &mut EventWriter<RequestSheetRevalidation>,
    daemon_client: &SharedDaemonClient,
) {
    info!("Creating structure sheet: {:?}/{}", category, struct_sheet_name);

    let data_filename = format!("{}.json", struct_sheet_name);
    let structure_metadata = crate::sheets::definitions::SheetMetadata::create_generic(
        struct_sheet_name.to_string(),
        data_filename,
        struct_columns.len(),
        category.clone(),
    );
    let structure_metadata = crate::sheets::definitions::SheetMetadata {
        columns: struct_columns.to_vec(),
        hidden: true,
        ..structure_metadata
    };

    let structure_sheet_data = crate::sheets::definitions::SheetGridData {
        metadata: Some(structure_metadata.clone()),
        grid: Vec::new(),
        row_indices: Vec::new(),
    };

    registry.add_or_replace_sheet(
        category.clone(),
        struct_sheet_name.to_string(),
        structure_sheet_data,
    );

    // Save the structure sheet (JSON) or create DB table (DB-backed)
    if structure_metadata.category.is_none() {
        crate::sheets::systems::io::save::save_single_sheet(registry, &structure_metadata);
    } else if let Some(cat_str) = category {
        info!("Creating DB table for structure sheet: {}", struct_sheet_name);
        let base_path = crate::sheets::systems::io::get_default_data_base_path();
        let db_path = base_path.join(format!("{}.db", cat_str));

        if db_path.exists() {
            match rusqlite::Connection::open(&db_path) {
                Ok(conn) => {
                    if let Err(e) = crate::sheets::database::schema::create_structure_table(
                        &conn,
                        parent_sheet_name,
                        parent_col_def,
                        Some(struct_columns),
                        daemon_client.client(),
                        db_path.file_name().and_then(|n| n.to_str()),
                    ) {
                        error!("Failed to create structure table '{}': {}", struct_sheet_name, e);
                    } else {
                        info!("Successfully created DB structure table: {}", struct_sheet_name);

                        // Create metadata table
                        if let Err(e) = crate::sheets::database::schema::create_metadata_table(
                            struct_sheet_name,
                            &structure_metadata,
                            daemon_client.client(),
                            db_path.file_name().and_then(|n| n.to_str()),
                        ) {
                            warn!(
                                "Failed to create metadata table for structure '{}': {}",
                                struct_sheet_name, e
                            );
                        }

                        // Drop parent's physical column if it exists
                        let _ = crate::sheets::database::writer::DbWriter::drop_physical_column_if_exists(
                            &conn,
                            parent_sheet_name,
                            &parent_col_def.header,
                            db_path.file_name().and_then(|n| n.to_str()),
                            daemon_client.client(),
                        );

                        // Copy content from parent to structure table
                        if let Err(e) = copy_parent_content_to_structure_table(
                            &conn,
                            parent_sheet_name,
                            struct_sheet_name,
                            parent_col_def,
                            struct_columns,
                            &db_path,
                            daemon_client.client(),
                        ) {
                            error!("Failed to copy content to structure table '{}': {}", struct_sheet_name, e);
                        } else {
                            info!("Successfully copied content to structure table: {}", struct_sheet_name);
                            
                            // CRITICAL: Reload structure table from DB to show the copied data
                            info!("🔄 Reloading structure table '{}' from database after copying data", struct_sheet_name);
                            match crate::sheets::database::reader::DbReader::read_sheet(
                                &conn,
                                struct_sheet_name,
                                daemon_client.client(),
                                db_path.file_name().and_then(|n| n.to_str()),
                            ) {
                                Ok(reloaded_structure) => {
                                    registry.add_or_replace_sheet(
                                        category.clone(),
                                        struct_sheet_name.to_string(),
                                        reloaded_structure,
                                    );
                                    info!("✅ Successfully reloaded structure table '{}' - data now visible in UI", struct_sheet_name);
                                    
                                    // Emit data modified event so UI refreshes
                                    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                                        category: category.clone(),
                                        sheet_name: struct_sheet_name.to_string(),
                                    });
                                }
                                Err(e) => {
                                    error!("❌ Failed to reload structure table '{}': {}", struct_sheet_name, e);
                                }
                            }
                        }

                        // Update parent metadata to Structure validator
                        // Use persist_column_validator_by_name to avoid index mismatch issues
                        info!("📝 Persisting Structure validator for parent column '{}'", parent_col_def.header);
                        if let Err(e) = crate::sheets::database::persist_column_validator_by_name(
                            cat_str,
                            parent_sheet_name,
                            &parent_col_def.header,
                            parent_col_def.data_type,
                            &Some(ColumnValidator::Structure),
                            parent_col_def.ai_include_in_send,
                            parent_col_def.ai_enable_row_generation,
                            daemon_client.client(),
                        ) {
                            error!("❌ Failed to persist Structure validator for parent column '{}': {}", parent_col_def.header, e);
                        } else {
                            info!("✅ Successfully persisted Structure validator for parent column '{}'", parent_col_def.header);
                        }

                        // CRITICAL: Reload parent sheet from DB to sync in-memory state with database
                        // After creating structure table and updating metadata, the in-memory registry
                        // still has the old state. We need to reload to ensure UI displays current data.
                        // The structure_schema is now populated automatically from child tables during read_sheet().
                        info!("🔄 Reloading parent sheet '{}' from database after structure table creation", parent_sheet_name);
                        match crate::sheets::database::reader::DbReader::read_sheet(
                            &conn,
                            parent_sheet_name,
                            daemon_client.client(),
                            db_path.file_name().and_then(|n| n.to_str()),
                        ) {
                            Ok(reloaded_parent) => {
                                // Verify structure_schema was populated from DB
                                if let Some(ref meta) = reloaded_parent.metadata {
                                    if let Some(col) = meta.columns.iter().find(|c| c.header == parent_col_def.header) {
                                        info!("✅ Structure schema populated from DB for '{}': schema_len={}, order_len={}, key_parent_idx={:?}",
                                            parent_col_def.header,
                                            col.structure_schema.as_ref().map(|s| s.len()).unwrap_or(0),
                                            col.structure_column_order.as_ref().map(|o| o.len()).unwrap_or(0),
                                            col.structure_key_parent_column_index
                                        );
                                    } else {
                                        error!("❌ Could not find column '{}' in reloaded parent metadata!", parent_col_def.header);
                                        info!("Available columns: {:?}", meta.columns.iter().map(|c| &c.header).collect::<Vec<_>>());
                                    }
                                }
                                
                                registry.add_or_replace_sheet(
                                    category.clone(),
                                    parent_sheet_name.to_string(),
                                    reloaded_parent,
                                );
                                info!("✅ Successfully reloaded parent sheet '{}' - in-memory state now matches database", parent_sheet_name);
                                
                                // Emit data modified event for parent sheet so UI refreshes
                                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                                    category: category.clone(),
                                    sheet_name: parent_sheet_name.to_string(),
                                });
                                
                                // CRITICAL: Request revalidation to rebuild render cache with the reloaded data
                                // This ensures Structure column buttons are rendered correctly
                                revalidation_writer.write(RequestSheetRevalidation {
                                    category: category.clone(),
                                    sheet_name: parent_sheet_name.to_string(),
                                });
                                info!("🔄 Requested render cache rebuild for parent sheet '{}'", parent_sheet_name);
                                
                                // Check if this was a Clean Start operation
                                info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                                info!("✅ CLEAN START: Step 2/3 Complete - Table created and data copied");
                                info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                            }
                            Err(e) => {
                                error!("❌ Failed to reload parent sheet '{}' from database: {}", parent_sheet_name, e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to open database: {}", e);
                }
            }
        }
    }

    feedback_writer.write(SheetOperationFeedback {
        message: format!("Created structure table '{}'", struct_sheet_name),
        is_error: false,
    });

    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
        category: category.clone(),
        sheet_name: struct_sheet_name.to_string(),
    });
}
