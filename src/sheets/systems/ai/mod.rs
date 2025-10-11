// src/sheets/systems/ai/mod.rs
// AI systems extracted from ui::systems

// Result handlers
pub mod legacy;
pub mod results; // Single-row result handler (potentially deprecated)

// Helper modules
pub mod phase2_helpers;
pub mod row_helpers; // Row processing utilities (snapshots, choices, normalization)
pub mod structure_jobs; // Structure job enqueueing logic
pub mod structure_results; // Structure result processing helpers // Phase 2 deep review processing (duplicate detection, merge workflow)

// Other systems
pub mod structure_processor;
pub mod throttled;
pub mod utils; // shared helpers (parser)
pub mod cache {
    pub mod linked_column_cache;
}
// Logic + task spawning for AI control panel (batch & prompt requests, metadata helpers)
pub mod control_handler;
