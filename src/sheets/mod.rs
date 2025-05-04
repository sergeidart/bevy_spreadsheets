// src/sheets/mod.rs

// --- Public Interface ---
// Declare modules first
pub mod definitions;
pub mod events;
pub mod plugin;
pub mod resources;

// Declare internal implementation module
// Made pub(crate) as systems are mostly internal implementation details now
pub(crate) mod systems;

// Re-export types needed externally (e.g., by UI or potentially other crates)
pub use definitions::{ColumnDataType, SheetGridData, SheetMetadata};
// Remove RequestSaveSheets from the re-export list
pub use events::{AddSheetRowRequest}; // Only re-export events potentially used outside this module
pub use plugin::SheetsPlugin;
pub use resources::SheetRegistry;

// Note: Consider if AddSheetRowRequest is still needed externally now that UI
// sends events handled entirely within the sheets module systems. If not,
// this line could potentially be removed too, further encapsulating the module.
// For now, we'll leave it as it doesn't hurt.