// src/sheets/systems/logic/create_sheet.rs
use crate::{
    sheets::{
        definitions::{SheetGridData, SheetMetadata},
        events::{RequestCreateNewSheet, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
        resources::SheetRegistry,
        systems::io::{
            save::save_single_sheet,
            validator, // For name validation
        },
    },
    ui::elements::editor::state::EditorWindowState, // To potentially set as selected
};
use bevy::prelude::*;
use rusqlite::Connection;

pub fn handle_create_new_sheet_request(
    _commands: Commands,
    mut events: EventReader<RequestCreateNewSheet>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut editor_state_opt: Option<ResMut<EditorWindowState>>, // Make optional if direct state change isn't critical path
) {
    for event in events.read() {
        let category = &event.category;
        let desired_name = event.desired_name.trim();

        // Validate name (using existing validator logic if possible, or a new one)
        // For now, using a simple check similar to startup scan validation
        if let Err(e) = validator::validate_derived_sheet_name(desired_name) {
            let msg = format!(
                "Failed to create sheet: Invalid name '{}'. {}",
                desired_name, e
            );
            error!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
            continue;
        }

        // Check if sheet already exists in this category
        if registry.get_sheet(category, desired_name).is_some() {
            let msg = format!(
                "Failed to create sheet: Name '{}' already exists in category '{:?}'.",
                desired_name, category
            );
            warn!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
            continue;
        }

        // Create dummy metadata (0 columns, 0 rows)
        let data_filename = format!("{}.json", desired_name);
        let mut new_metadata = SheetMetadata::create_generic(
            desired_name.to_string(),
            data_filename,
            0, // 0 columns for a dummy sheet
            category.clone(),
        );
        // Regular tables default to shown
        new_metadata.hidden = false;

        let new_sheet_data = SheetGridData {
            metadata: Some(new_metadata.clone()),
            grid: Vec::new(), // 0 rows
            row_indices: Vec::new(),
        };

        // Add to registry
        registry.add_or_replace_sheet(category.clone(), desired_name.to_string(), new_sheet_data);

        info!(
            "Successfully created new sheet '{:?}/{}' in registry.",
            category, desired_name
        );

        // Persist the new sheet based on storage mode
        // 1) JSON (no category)
        // 2) DB-backed (has category): create tables and register in _Metadata immediately
        let registry_immut = registry.as_ref();
        if new_metadata.category.is_none() {
            // Legacy JSON persistence
            save_single_sheet(registry_immut, &new_metadata);
            info!("Saved new sheet '{:?}/{}' to JSON.", category, desired_name);
        } else if let Some(cat) = &new_metadata.category {
            // DB-backed: create SQLite artifacts
            let base = crate::sheets::systems::io::get_default_data_base_path();
            let db_path = base.join(format!("{}.db", cat));
            match Connection::open(&db_path) {
                Ok(conn) => {
                    // Ensure global metadata exists
                    if let Err(e) =
                        crate::sheets::database::schema::ensure_global_metadata_table(&conn)
                    {
                        error!(
                            "Failed to ensure _Metadata in DB '{}': {}",
                            db_path.display(),
                            e
                        );
                    }

                    // Compute display order (append at end)
                    let next_order: Option<i32> = conn
                        .query_row(
                            "SELECT COALESCE(MAX(display_order), -1) + 1 FROM _Metadata WHERE table_type = 'main'",
                            [],
                            |r| r.get::<_, i32>(0),
                        )
                        .ok();

                    // Insert _Metadata row for this table
                    if let Err(e) = crate::sheets::database::schema::insert_table_metadata(
                        &conn,
                        desired_name,
                        &new_metadata,
                        next_order,
                    ) {
                        error!(
                            "Failed to insert _Metadata for new sheet '{:?}/{}': {}",
                            category, desired_name, e
                        );
                    }

                    // Create the per-table metadata and data tables
                    if let Err(e) = crate::sheets::database::schema::create_metadata_table(
                        &conn,
                        desired_name,
                        &new_metadata,
                    ) {
                        error!(
                            "Failed to create metadata table for '{:?}/{}': {}",
                            category, desired_name, e
                        );
                    }
                    if let Err(e) = crate::sheets::database::schema::create_data_table(
                        &conn,
                        desired_name,
                        &new_metadata.columns,
                    ) {
                        error!(
                            "Failed to create data table for '{:?}/{}': {}",
                            category, desired_name, e
                        );
                    }

                    // No AI groups initially; nothing to seed
                    info!(
                        "Saved new sheet '{:?}/{}' to DB at '{}'.",
                        category,
                        desired_name,
                        db_path.display()
                    );

                    // Immediately reload runtime metadata from DB so technical columns
                    // (e.g., row_index) are present in-memory before any render cache builds.
                    match crate::sheets::database::reader::DbReader::read_sheet(&conn, desired_name) {
                        Ok(loaded) => {
                            registry.add_or_replace_sheet(category.clone(), desired_name.to_string(), loaded);
                            info!(
                                "Reloaded new sheet '{:?}/{}' from DB to include technical columns before caching.",
                                category, desired_name
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Failed to reload sheet '{:?}/{}' from DB after creation: {:?}",
                                category, desired_name, e
                            );
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to open DB '{}' to persist new sheet '{:?}/{}': {}",
                        db_path.display(),
                        category,
                        desired_name,
                        e
                    );
                }
            }
        }

        feedback_writer.write(SheetOperationFeedback {
            message: format!("Sheet '{:?}/{}' created.", category, desired_name),
            is_error: false,
        });

        data_modified_writer.write(SheetDataModifiedInRegistryEvent {
            category: category.clone(),
            sheet_name: desired_name.to_string(),
        });

        // Optionally, set this new sheet as selected in the UI
        if let Some(editor_state) = editor_state_opt.as_mut() {
            editor_state.selected_category = category.clone();
            editor_state.selected_sheet_name = Some(desired_name.to_string());
            editor_state.reset_interaction_modes_and_selections(); // Reset modes
            editor_state.force_filter_recalculation = true; // Ensure UI updates
                                                            // Legacy AI config popup removed; no init flag needed
            info!(
                "Set newly created sheet '{:?}/{}' as active.",
                category, desired_name
            );
        }
    }
}
