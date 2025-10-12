// src/sheets/database/systems/completion_handler.rs

use crate::sheets::events::MigrationCompleted;
use bevy::prelude::*;

/// Handle migration completion events and update UI state
pub fn handle_migration_completion(
    mut events: EventReader<MigrationCompleted>,
    mut migration_state: ResMut<crate::ui::elements::popups::MigrationPopupState>,
) {
    for _event in events.read() {
        migration_state.migration_in_progress = false;
        // Could add more UI feedback here
    }
}
