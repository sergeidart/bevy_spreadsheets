// src/sheets/systems/ui_handlers/mod.rs
//! UI-related handlers - separating data/cache mechanics from rendering.
//! Split into multiple modules for better organization.

pub mod ai_include_handlers;
pub mod category_handlers;
pub mod column_handlers;
pub mod sheet_handlers;
pub mod structure_handlers;
pub mod ui_cache;

// Re-export commonly used functions for convenience
pub use ai_include_handlers::*;
pub use column_handlers::*;
pub use structure_handlers::*;
