// src/visual_copier/plugin.rs

use bevy::app::{App, Plugin, Startup, Update, AppExit};
use bevy::prelude::*;

use super::resources::VisualCopierManager;
use super::events::*; // Make sure this imports all events including the new ones
use super::handler::{
    handle_add_new_copy_task_event_system,
    handle_copy_operation_result_event_system,
    handle_folder_picked_event_system,     // The system that processes FolderPickedEvent
    handle_pick_folder_request_system,
    handle_queue_all_copy_tasks_event_system,
    handle_queue_copy_task_event_system,
    handle_queue_top_panel_copy_event_system,
    handle_remove_copy_task_event_system,
    handle_reverse_top_panel_folders_event_system,
    // Add new mutator systems that APPLY the updates
    apply_task_start_folder_update_system,
    apply_task_end_folder_update_system,
    apply_top_panel_from_folder_update_system,
    apply_top_panel_to_folder_update_system,
};
use super::processes::process_copy_operations_system;
use super::io::{load_copy_tasks_from_file, save_copy_tasks_to_file};

/// Plugin to integrate the Visual Copier functionality into a Bevy application.
pub struct VisualCopierPlugin;

impl Plugin for VisualCopierPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisualCopierManager>();

        // Register Events
        app.add_event::<AddNewCopyTaskEvent>()
            .add_event::<RemoveCopyTaskEvent>()
            .add_event::<PickFolderRequest>()
            .add_event::<FolderPickedEvent>()
            // --- !!! ENSURE THESE LINES ARE PRESENT !!! ---
            .add_event::<UpdateTaskStartFolderEvent>()
            .add_event::<UpdateTaskEndFolderEvent>()
            .add_event::<UpdateTopPanelFromFolderEvent>()
            .add_event::<UpdateTopPanelToFolderEvent>()
            // --- !!! END ENSURE ---
            .add_event::<QueueCopyTaskEvent>()
            .add_event::<QueueTopPanelCopyEvent>()
            .add_event::<QueueAllCopyTasksEvent>()
            .add_event::<ReverseTopPanelFoldersEvent>()
            .add_event::<CopyOperationResultEvent>();

        app.add_systems(Startup, load_visual_copier_state_on_startup_system);

        // Add systems with explicit ordering
        app.add_systems(Update,
            (
                // Initial event handlers
                handle_add_new_copy_task_event_system,
                handle_remove_copy_task_event_system,
                handle_pick_folder_request_system,
                handle_folder_picked_event_system, // Reads FolderPickedEvent, Writes specific Update...Events

                // Apply the specific updates (read Update...Events, mutate Manager)
                // Run these after the handle_folder_picked_event_system
                (
                    apply_task_start_folder_update_system,
                    apply_task_end_folder_update_system,
                    apply_top_panel_from_folder_update_system,
                    apply_top_panel_to_folder_update_system,
                ).after(handle_folder_picked_event_system),

                // Other handlers
                handle_reverse_top_panel_folders_event_system,
                handle_queue_copy_task_event_system,
                handle_queue_top_panel_copy_event_system,
                handle_queue_all_copy_tasks_event_system,

                // Copy processing
                process_copy_operations_system,
                handle_copy_operation_result_event_system.after(process_copy_operations_system),

            ).chain() // Apply ordering within the tuple if needed, chain ensures sequence here
                     // Consider using explicit System Sets for more complex schedules
        );

        app.add_systems(Update, save_on_exit_system);

        info!("VisualCopierPlugin initialized with refactored folder picking logic.");
    }
}

// --- Helper Functions (load_visual_copier_state_on_startup_system, save_on_exit_system) ---
// (These functions remain the same as in the previous correct version)
fn load_visual_copier_state_on_startup_system(mut manager: ResMut<VisualCopierManager>) {
    info!("VisualCopier: Attempting to load copy tasks at startup...");
    match load_copy_tasks_from_file() {
        Ok(tasks) => {
            manager.copy_tasks = tasks;
            manager.recalculate_next_id();
            info!("VisualCopier: Successfully loaded {} tasks. Next ID is {}.", manager.copy_tasks.len(), manager.next_id);
        }
        Err(e) => {
            error!("VisualCopier: Failed to load copy tasks at startup: {}. Starting with default state.", e);
            *manager = VisualCopierManager::default();
        }
    }
    manager.top_panel_copy_status = "Idle".to_string();
    manager.is_saving_on_exit = false;
}

fn save_on_exit_system(
    mut exit_event_reader: EventReader<AppExit>,
    mut manager: ResMut<VisualCopierManager>,
) {
    if exit_event_reader.read().next().is_some() {
        if !manager.is_saving_on_exit {
            manager.is_saving_on_exit = true;
            info!("VisualCopier: AppExit event received. Attempting to save copy tasks...");
            match save_copy_tasks_to_file(&manager.copy_tasks) {
                Ok(_) => info!("VisualCopier: Successfully saved copy tasks on exit."),
                Err(e) => error!("VisualCopier: Failed to save copy tasks on exit: {}", e),
            }
        }
    }
}