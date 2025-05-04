// src/sheets/mod.rs

// --- Public Interface ---
// Declare modules first
pub mod definitions;
pub mod events;
pub mod plugin;
pub mod resources;

// Declare internal implementation module
pub(crate) mod systems;

// Re-export types needed externally (e.g., by UI)
pub use definitions::{ColumnDataType, SheetGridData, SheetMetadata};
pub use events::{AddSheetRowRequest, RequestSaveSheets};
pub use plugin::SheetsPlugin;
pub use resources::SheetRegistry;