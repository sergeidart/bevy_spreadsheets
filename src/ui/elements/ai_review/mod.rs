// src/ui/elements/ai_review/mod.rs
// Central module for AI Review related UI extracted from editor
pub mod ai_batch_review_ui;
pub mod ai_context_utils;
pub mod ai_control_left_panel; // new split left section
pub mod ai_group_panel; // new group schema management UI
pub mod ai_panel; // new orchestrator replacing old ai_control_panel
pub mod navigation; // Navigation logic for drilling into child tables
pub mod structure_review_helpers; // Helper functions for structure review conversion
                                  // New modularized components
pub mod render {
    pub mod ai_row;
    pub mod cell_render;
    pub mod column_headers;
    pub mod original_row;
    pub mod row_render;
}
pub mod handlers;
pub mod header_actions;
pub mod serialization_helpers; // Shared helpers for structure serialization and parent key resolution
pub mod table_headers;
