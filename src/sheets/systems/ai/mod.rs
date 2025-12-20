// src/sheets/systems/ai/mod.rs
// AI systems extracted from ui::systems

// New unified processor (replacing multi-step logic)
pub mod processor;

// Result handlers
pub mod legacy;
pub mod results; // Batch result handlers (split into submodules)

// Helper modules
pub mod column_helpers; // Column extraction utilities (linked columns, dynamic prefix)
pub mod duplicate_map_helpers; // Duplicate detection map building (parent chain aware)
pub mod original_cache; // Original row caching for AI review
pub mod parent_chain_helpers; // Parent chain filtering and row matching
pub mod phase2_helpers; // Phase 2 deep review processing (duplicate detection, merge workflow)
pub mod phase2_row_processors; // Phase 2 row processing (original, duplicate, new rows)
pub mod row_helpers; // Row processing utilities (snapshots, choices, normalization)
pub mod structure_jobs; // Structure job enqueueing logic
pub mod structure_results; // Structure result processing helpers

// Other systems
pub mod structure_processor;
pub mod throttled;
pub mod utils; // shared helpers (parser)
pub mod cache {
    pub mod linked_column_cache;
}
// Logic + task spawning for AI control panel (batch & prompt requests, metadata helpers)
pub mod control_handler;
