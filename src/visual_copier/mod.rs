// src/visual_copier/mod.rs

// Declare the submodules for the visual copier feature
pub mod events;
pub mod plugin;
pub mod resources;
// pub mod systems; // Old systems file, now split
pub mod io; // For loading/saving copy task configurations

// New split system files
pub mod executers;
pub mod handler;
pub mod processes;

// Re-export the plugin for easy use in main.rs
pub use plugin::VisualCopierPlugin;
