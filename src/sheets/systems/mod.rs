// src/sheets/systems/mod.rs

pub mod ai;
pub mod ai_review;
pub mod io; // Declares the io submodule (maps to the io/ directory)
pub mod logic; // Declares the logic submodule (maps to the logic/ directory) // New: AI-related systems (results handling, throttled apply)
pub mod ui_handlers; // UI logic handlers (separated from rendering, organized in submodules)
