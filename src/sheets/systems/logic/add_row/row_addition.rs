// src/sheets/systems/logic/add_row_handlers/row_addition.rs
// Core row addition handler - orchestrates JSON and DB persistence

use crate::sheets::{
    events::{AddSheetRowRequest, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;

use super::{
    cache_handlers::{get_structure_context, invalidate_sheet_cache, resolve_virtual_context},
    db_persistence::persist_row_to_db,
    json_persistence::persist_row_addition_json,
};

/// Main handler for add row requests - orchestrates row addition to sheets
pub fn handle_add_row_request(
    mut events: EventReader<AddSheetRowRequest>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut editor_state: Option<ResMut<EditorWindowState>>,
) {
    for event in events.read() {
        // Resolve virtual context if active
        let (category, sheet_name) = resolve_virtual_context(
            &editor_state,
            event.category.clone(),
            event.sheet_name.clone(),
        );

        // Get structure context (parent_key) if in structure navigation
        let structure_context = get_structure_context(&editor_state, &sheet_name, &category);

        let mut metadata_cache: Option<crate::sheets::definitions::SheetMetadata> = None;
        let mut pending_json_save: Option<crate::sheets::definitions::SheetMetadata> = None;

        if let Some(sheet_data) = registry.get_sheet_mut(&category, &sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_cols = metadata.columns.len();
                
                // Unified behavior: always insert at top for consistency
                sheet_data.grid.insert(0, vec![String::new(); num_cols]);

                // Detect if this is a structure sheet by checking if it has 'id' and 'parent_key' columns at indices 0 and 1
                let is_structure_sheet = num_cols >= 2
                    && metadata
                        .columns
                        .get(0)
                        .map(|c| c.header.eq_ignore_ascii_case("id"))
                        .unwrap_or(false)
                    && metadata
                        .columns
                        .get(1)
                        .map(|c| c.header.eq_ignore_ascii_case("parent_key"))
                        .unwrap_or(false);

                // Auto-fill structure sheet columns if in structure navigation context OR if this is a structure sheet
                if is_structure_sheet {
                    if let Some(row0) = sheet_data.grid.get_mut(0) {
                        // Auto-fill id column (index 0) with unique ID
                        if row0.len() > 0 && row0[0].is_empty() {
                            // Generate unique ID using timestamp + random component
                            let unique_id = format!(
                                "{}-{}",
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis(),
                                uuid::Uuid::new_v4()
                                    .to_string()
                                    .split('-')
                                    .next()
                                    .unwrap_or("0")
                            );
                            row0[0] = unique_id;
                        }
                        // Auto-fill parent_key column (index 1) if we have a parent_key from navigation context
                        if row0.len() > 1 && row0[1].is_empty() {
                            if let Some(parent_key) = &structure_context {
                                row0[1] = parent_key.clone();
                            }
                        }
                    }
                }

                // If initial values provided, set them now to avoid race with subsequent events
                if let Some(init) = &event.initial_values {
                    if let Some(row0) = sheet_data.grid.get_mut(0) {
                        for (col, val) in init {
                            if *col < row0.len() {
                                row0[*col] = val.clone();
                            }
                        }
                    }
                }

                let msg = format!(
                    "Added new row at the top of sheet '{:?}/{}'.",
                    category, sheet_name
                );
                info!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: false,
                });

                // Invalidate any cached filtered indices for this sheet to force UI refresh
                invalidate_sheet_cache(&mut editor_state, &category, &sheet_name);

                metadata_cache = Some(metadata.clone());

                // Persist to DB if DB-backed, otherwise save JSON
                if let Some(meta) = &sheet_data.metadata {
                    if meta.category.is_some() {
                        // DB-backed: prepend row in database too
                        if let Err(e) = persist_row_to_db(meta, &sheet_name, &category, &sheet_data.grid) {
                            warn!("Failed to persist row to DB: {}", e);
                        }
                    } else {
                        // Legacy JSON: defer save until after mutable borrow ends
                        pending_json_save = Some(meta.clone());
                    }
                }

                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: sheet_name.clone(),
                });
            } else {
                let msg = format!(
                    "Cannot add row to sheet '{:?}/{}': Metadata missing.",
                    category, sheet_name
                );
                warn!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        } else {
            let msg = format!(
                "Cannot add row: Sheet '{:?}/{}' not found in registry.",
                category, sheet_name
            );
            warn!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
        }

        if let Some(meta_to_save) = metadata_cache {
            info!(
                "Row added to '{:?}/{}', triggering immediate save.",
                category, sheet_name
            );
            let registry_immut = registry.as_ref();
            // Skip JSON save if DB-backed (category is Some)
            if meta_to_save.category.is_none() {
                persist_row_addition_json(registry_immut, &meta_to_save);
            }
        }

        // Perform deferred JSON save after mutable borrows are released
        if let Some(meta) = pending_json_save.take() {
            persist_row_addition_json(registry.as_ref(), &meta);
        }
    }
}
