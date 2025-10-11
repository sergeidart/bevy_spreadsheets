// src/sheets/systems/logic/add_row_handlers/json_persistence.rs
// JSON-specific persistence operations for add_row functionality

use crate::sheets::{
    definitions::SheetMetadata,
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};

/// Saves sheet metadata to JSON file (legacy format)
pub(super) fn save_to_json(registry: &SheetRegistry, metadata: &SheetMetadata) {
    // Only save if this is a JSON-backed sheet (category is None)
    if metadata.category.is_none() {
        save_single_sheet(registry, metadata);
    }
}

/// Prepares and saves sheet metadata to JSON after row addition
pub(super) fn persist_row_addition_json(
    registry: &SheetRegistry,
    metadata: &SheetMetadata,
) {
    if metadata.category.is_none() {
        save_single_sheet(registry, metadata);
    }
}
