// src/sheets/mod.rs

// --- Public Interface ---
// Declare modules first
pub mod definitions;
pub mod events;
pub mod plugin;
pub mod resources;

pub(crate) mod systems;

pub use plugin::SheetsPlugin;
