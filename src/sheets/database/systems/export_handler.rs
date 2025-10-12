// src/sheets/database/systems/export_handler.rs

use crate::sheets::events::{RequestExportSheetToJson, SheetOperationFeedback};
use crate::sheets::database::migration::MigrationTools;
use bevy::prelude::*;

/// Handle requests to export a sheet from SQLite database to JSON format
pub fn handle_export_requests(
    mut events: EventReader<RequestExportSheetToJson>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    for event in events.read() {
        info!(
            "Exporting table '{}' from {:?} to {:?}",
            event.table_name, event.db_path, event.output_folder
        );

        match rusqlite::Connection::open(&event.db_path) {
            Ok(conn) => {
                match MigrationTools::export_sheet_to_json(
                    &conn,
                    &event.table_name,
                    &event.output_folder,
                ) {
                    Ok(_) => {
                        let msg = format!("Successfully exported '{}' to JSON", event.table_name);
                        info!("{}", msg);
                        feedback_writer.write(SheetOperationFeedback {
                            message: msg,
                            is_error: false,
                        });
                    }
                    Err(e) => {
                        let msg = format!("Failed to export '{}': {}", event.table_name, e);
                        error!("{}", msg);
                        feedback_writer.write(SheetOperationFeedback {
                            message: msg,
                            is_error: true,
                        });
                    }
                }
            }
            Err(e) => {
                let msg = format!("Failed to open database: {}", e);
                error!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        }
    }
}
