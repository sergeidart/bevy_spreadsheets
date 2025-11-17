// src/sheets/systems/ai_review/child_table_loader.rs
// System to load structure child tables once when AI Review starts

use crate::sheets::resources::SheetRegistry;
use crate::sheets::database::daemon_resource::SharedDaemonClient;
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;

/// System that runs once per frame to check if child tables need loading
/// This runs BEFORE the UI render to ensure tables are ready
pub fn load_structure_child_tables_system(
    mut state: ResMut<EditorWindowState>,
    mut registry: ResMut<SheetRegistry>,
    daemon: Res<SharedDaemonClient>,
) {
    // Only run if flag is set
    if !state.ai_needs_structure_child_tables_loaded {
        return;
    }
    
    info!("[LOADER] Flag detected! Starting structure child table loading...");
    
    // Clear flag immediately to prevent re-running
    state.ai_needs_structure_child_tables_loaded = false;
    
    // Get the current sheet that's being reviewed
    let category = state.selected_category.clone();
    let Some(sheet_name) = state.selected_sheet_name.clone() else { return };
    
    debug!(
        "[ONCE] Loading structure child tables for AI Review: category={:?}, sheet={}",
        category, sheet_name
    );
    
    // Reuse the existing load_linked_target_sheets function
    // It already handles both linked sheets AND structure child tables
    crate::sheets::systems::ui_handlers::sheet_handlers::load_linked_target_sheets(
        &mut state,
        &mut registry,
        daemon.client(),
        &sheet_name,
        &category,
    );
    
    info!("[ONCE] Structure child tables loaded successfully for AI Review");
}
