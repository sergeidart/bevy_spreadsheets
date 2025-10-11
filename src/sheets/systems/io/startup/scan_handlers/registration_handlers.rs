// src/sheets/systems/io/startup/scan_handlers/registration_handlers.rs
//! Handlers for sheet registration during scanning.

use crate::sheets::definitions::SheetMetadata;
use crate::sheets::resources::SheetRegistry;
use bevy::prelude::*;

/// Add a scanned sheet to the registry with validation
pub fn add_scanned_sheet_to_registry(
    registry: &mut SheetRegistry,
    category: Option<String>,
    sheet_name: String,
    metadata: SheetMetadata,
    grid: Vec<Vec<String>>,
    source_path: String,
) -> bool {
    // Create sheet data structure
    let sheet_data = crate::sheets::definitions::SheetGridData {
        metadata: Some(metadata),
        grid,
    };

    // Add to registry
    registry.add_or_replace_sheet(category.clone(), sheet_name.clone(), sheet_data);

    info!(
        "Startup Scan: Successfully registered sheet '{:?}/{}' from '{}'",
        category, sheet_name, source_path
    );

    true
}
