// src/visual_copier/plugin.rs

use bevy::app::{App, Plugin, Startup, Update, AppExit};
use bevy::prelude::*;

use super::resources::VisualCopierManager;
use super::events::*;
use super::handler::{
    handle_add_new_copy_task_event_system,
    handle_copy_operation_result_event_system,
    handle_folder_picked_event_system,
    handle_pick_folder_request_system,
    handle_queue_all_copy_tasks_event_system,
    handle_queue_copy_task_event_system,
    handle_queue_top_panel_copy_event_system,
    handle_remove_copy_task_event_system,
    handle_reverse_top_panel_folders_event_system,
    apply_task_start_folder_update_system,
    apply_task_end_folder_update_system,
    apply_top_panel_from_folder_update_system,
    apply_top_panel_to_folder_update_system,
};
use super::processes::process_copy_operations_system;
// Import new io functions
use super::io::{load_copier_manager_from_file, save_copier_manager_to_file};

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
            .add_event::<UpdateTaskStartFolderEvent>()
            .add_event::<UpdateTaskEndFolderEvent>()
            .add_event::<UpdateTopPanelFromFolderEvent>()
            .add_event::<UpdateTopPanelToFolderEvent>()
            .add_event::<QueueCopyTaskEvent>()
            .add_event::<QueueTopPanelCopyEvent>()
            .add_event::<QueueAllCopyTasksEvent>()
            .add_event::<ReverseTopPanelFoldersEvent>()
            .add_event::<CopyOperationResultEvent>();

        // Updated startup system
        app.add_systems(Startup, load_visual_copier_state_on_startup_system);

        // Add systems (order remains the same)
        app.add_systems(Update,
            (
                // Initial event handlers
                handle_add_new_copy_task_event_system,
                handle_remove_copy_task_event_system,
                handle_pick_folder_request_system,
                handle_folder_picked_event_system,

                // Apply the specific updates
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

            ).chain()
        );

        // Updated exit system
        app.add_systems(Update, save_on_exit_system);

        info!("VisualCopierPlugin initialized with Manager load/save.");
    }
}

// --- Updated Helper Functions ---

// Updated startup system
fn load_visual_copier_state_on_startup_system(mut manager: ResMut<VisualCopierManager>) {
    info!("VisualCopier: Attempting to load copier state at startup...");
    match load_copier_manager_from_file() {
        Ok(loaded_manager) => {
            // Assign loaded data to the resource
            *manager = loaded_manager;
            // Recalculate next ID and reset transient state
            manager.recalculate_next_id();
            manager.reset_transient_state(); // Reset status fields etc.
            info!(
                "VisualCopier: Successfully loaded state ({} tasks, Top From: {:?}, Top To: {:?}). Next ID is {}.",
                manager.copy_tasks.len(),
                manager.top_panel_from_folder,
                manager.top_panel_to_folder,
                manager.next_id
            );
        }
        Err(e) => {
            // load_copier_manager_from_file now returns default on error, so just log
             error!("VisualCopier: IO error during load: {}. Starting with default state.", e);
             *manager = VisualCopierManager::default(); // Ensure default state
             manager.reset_transient_state();
        }
    }
}

// Updated exit system
fn save_on_exit_system(
    mut exit_event_reader: EventReader<AppExit>,
    mut manager: ResMut<VisualCopierManager>, // Use ResMut to set the is_saving flag
) {
    // Check only once per AppExit event occurrence
    if exit_event_reader.read().next().is_some() {
        // Use a flag to prevent multiple save attempts if AppExit is sent multiple times
        if !manager.is_saving_on_exit {
            manager.is_saving_on_exit = true; // Set flag immediately
            info!("VisualCopier: AppExit event received. Attempting to save copier state...");
            // Pass the entire manager resource to the save function
            match save_copier_manager_to_file(&manager) {
                Ok(_) => info!("VisualCopier: Successfully saved copier state on exit."),
                Err(e) => error!("VisualCopier: Failed to save copier state on exit: {}", e),
            }
        }
    }
}