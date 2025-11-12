// src/sheets/systems/io/lazy_load.rs
//! System to handle lazy loading of database table lists when categories are selected

use bevy::prelude::*;
use crate::sheets::{
    events::RequestSheetRevalidation,
    resources::SheetRegistry,
    database::daemon_resource::SharedDaemonClient,
};
use crate::ui::elements::editor::state::EditorWindowState;
use super::get_default_data_base_path;

/// System that loads table list for a category when needed (lazy loading)
pub fn lazy_load_category_tables(
    mut state: ResMut<EditorWindowState>,
    mut registry: ResMut<SheetRegistry>,
    mut revalidate_writer: EventWriter<RequestSheetRevalidation>,
    daemon_resource: Res<SharedDaemonClient>,
) {
    if !state.category_needs_table_list_load {
        return;
    }

    state.category_needs_table_list_load = false;

    let Some(category_name) = &state.selected_category else {
        return;
    };

    info!("Lazy loading table list for category '{}'", category_name);

    let base_path = get_default_data_base_path();
    let db_path = base_path.join(format!("{}.db", category_name));

    if !db_path.exists() {
        warn!("Database file not found for lazy load: {:?}", db_path);
        return;
    }

    // Load the table list (stubs, not full data)
    super::startup::scan_handlers::load_database_tables(
        &mut registry,
        &mut revalidate_writer,
        &db_path,
        daemon_resource.client(),
    );
}
