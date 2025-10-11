// src/ui/elements/bottom_panel/mod.rs
//! Bottom panel UI components for sheet and category management
//! 
//! This module contains the UI for selecting and managing sheets and categories,
//! which appears at the bottom of the application window.

// Internal modules
mod category_row;
mod dropdowns;
mod drop_visuals;
mod popups;
mod sheet_row;

// Public module
pub mod sheet_management_bar;

// Re-export the main function for convenience
pub use sheet_management_bar::{show_sheet_management_controls, SheetManagementEventWriters};
