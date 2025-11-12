// src/sheets/systems/ui_handlers/sheet_handlers.rs
use bevy::prelude::*;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::database::daemon_client::DaemonClient;
use crate::ui::elements::editor::state::EditorWindowState;

/// Handle sheet selection change
/// NOTE: This function only updates UI state. Cache reload happens in the system that has access to SheetRegistry.
pub fn handle_sheet_selection(
    state: &mut EditorWindowState,
    new_sheet: Option<String>,
) {
    if state.selected_sheet_name != new_sheet {
        state.selected_sheet_name = new_sheet;
        if state.selected_sheet_name.is_none() {
            state.reset_interaction_modes_and_selections();
        } else {
            // On direct sheet open, clear hidden navigation filters
            state.structure_navigation_stack.clear();
            state.filtered_row_indices_cache.clear();
            
            // Mark that cache needs to be reloaded from DB
            state.force_cache_reload = true;
        }
        state.force_filter_recalculation = true;
        state.show_column_options_popup = false;
    }
}

/// Reload sheet data from DB if needed (call this from a system with access to SheetRegistry)
pub fn reload_sheet_cache_from_db(
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
    daemon_client: &DaemonClient,
) {
    if !state.force_cache_reload {
        return;
    }
    
    state.force_cache_reload = false;
    
    let Some(sheet_name) = &state.selected_sheet_name else {
        return;
    };
    
    let category = &state.selected_category;
    
    // Check if this is a DB-backed sheet
    let is_db_backed = registry
        .get_sheet(category, sheet_name)
        .and_then(|s| s.metadata.as_ref())
        .and_then(|m| m.category.as_ref())
        .is_some();
    
    if !is_db_backed {
        debug!("Sheet '{}' is not DB-backed, skipping cache reload", sheet_name);
        return;
    }
    
    // Reload from database
    let Some(cat_str) = category.as_ref() else {
        return;
    };
    
    info!("Reloading cache for sheet '{}' from DB", sheet_name);
    let base_path = crate::sheets::systems::io::get_default_data_base_path();
    let db_path = base_path.join(format!("{}.db", cat_str));
    
    if !db_path.exists() {
        warn!("Database file not found for cache reload: {:?}", db_path);
        return;
    }
    
    match rusqlite::Connection::open(&db_path) {
        Ok(conn) => {
            match crate::sheets::database::reader::DbReader::read_sheet(&conn, sheet_name, daemon_client, Some(cat_str)) {
                Ok(sheet_data) => {
                    info!("Successfully reloaded {} rows from DB for sheet '{}'", sheet_data.grid.len(), sheet_name);
                    registry.add_or_replace_sheet(
                        category.clone(),
                        sheet_name.clone(),
                        sheet_data,
                    );
                }
                Err(e) => {
                    error!("Failed to reload sheet data from DB: {}", e);
                }
            }
        }
        Err(e) => {
            error!("Failed to open DB for cache reload: {}", e);
        }
    }
}

/// Validate that the selected sheet still exists in the registry
pub fn validate_sheet_selection(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
) {
    if let Some(current_sheet_name) = state.selected_sheet_name.as_ref() {
        if !registry
            .get_sheet_names_in_category(&state.selected_category)
            .contains(current_sheet_name)
        {
            state.selected_sheet_name = None;
            state.reset_interaction_modes_and_selections();
            state.force_filter_recalculation = true;
            state.show_column_options_popup = false;
        }
    }
}

/// Handle rename sheet request
pub fn handle_rename_sheet_request(
    state: &mut EditorWindowState,
) {
    if let Some(ref name_to_rename) = state.selected_sheet_name {
        state.rename_target_category = state.selected_category.clone();
        state.rename_target_sheet = name_to_rename.clone();
        state.new_name_input = state.rename_target_sheet.clone();
        state.show_rename_popup = true;
    }
}

/// Handle delete sheet request
pub fn handle_delete_sheet_request(
    state: &mut EditorWindowState,
) {
    if let Some(ref name_to_delete) = state.selected_sheet_name {
        state.delete_target_category = state.selected_category.clone();
        state.delete_target_sheet = name_to_delete.clone();
        state.show_delete_confirm_popup = true;
    }
}

/// Handle new sheet request
pub fn handle_new_sheet_request(
    state: &mut EditorWindowState,
) {
    state.new_sheet_target_category = state.selected_category.clone();
    state.new_sheet_name_input.clear();
    state.show_new_sheet_popup = true;
}

/// Handle sheet picker expand/collapse toggle
pub fn handle_sheet_picker_toggle(
    state: &mut EditorWindowState,
) {
    state.sheet_picker_expanded = !state.sheet_picker_expanded;
}

/// Handle sheet drag start
pub fn handle_sheet_drag_start(
    state: &mut EditorWindowState,
    sheet_name: String,
) {
    state.dragged_sheet = Some((state.selected_category.clone(), sheet_name));
}

/// Clear drag state
pub fn clear_drag_state(state: &mut EditorWindowState) {
    state.dragged_sheet = None;
}
