// src/visual_copier/plugin.rs

// --- MODIFIED: Import AppExit here ---
use bevy::app::{App, Plugin, Startup, Update, AppExit};
// --- END MODIFIED ---
use bevy::prelude::*;

use super::resources::VisualCopierManager;
// --- MODIFIED: Add RequestAppExit event ---
use super::events::*; // Includes VisualCopierStateChanged, RequestAppExit
// --- END MODIFIED ---
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
    handle_visual_copier_state_change_and_save_system,
};
use super::processes::process_copy_operations_system;
use super::io::{load_copier_manager_from_file, save_copier_manager_to_file};

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
            .add_event::<CopyOperationResultEvent>()
            // --- MODIFIED: Register new events ---
            .add_event::<VisualCopierStateChanged>()
            .add_event::<RequestAppExit>();
            // --- END MODIFIED ---

        app.add_systems(Startup, load_visual_copier_state_on_startup);

        // Add systems
        app.add_systems(Update,
            (
                // Folder picking and updates
                handle_pick_folder_request_system,
                handle_folder_picked_event_system,
                (
                    apply_task_start_folder_update_system,
                    apply_task_end_folder_update_system,
                    apply_top_panel_from_folder_update_system,
                    apply_top_panel_to_folder_update_system,
                ).after(handle_folder_picked_event_system),

                // Other state modifications
                handle_add_new_copy_task_event_system,
                handle_remove_copy_task_event_system,
                handle_reverse_top_panel_folders_event_system,

                // Handle immediate save
                handle_visual_copier_state_change_and_save_system
                    .after(apply_top_panel_to_folder_update_system) // Example ordering
                    .after(handle_add_new_copy_task_event_system)
                    .after(handle_remove_copy_task_event_system)
                    .after(handle_reverse_top_panel_folders_event_system),

                // Queueing and processing copies (asynchronous)
                handle_queue_copy_task_event_system,
                handle_queue_top_panel_copy_event_system,
                handle_queue_all_copy_tasks_event_system,
                apply_deferred,
                process_copy_operations_system,
                handle_copy_operation_result_event_system.after(process_copy_operations_system),

                // --- ADDED: Register new custom exit handler ---
                custom_exit_handler_system,
                // --- END ADDED ---

            ).chain()
        );

        // --- REMOVED old exit system ---
        // app.add_systems(Update, perform_actions_on_exit_system);
        // --- END REMOVED ---

        info!("VisualCopierPlugin initialized with immediate saves and custom pre-exit handling.");
    }
}

// --- Startup system (Remains the same as previous version) ---
fn load_visual_copier_state_on_startup(
    mut manager: ResMut<VisualCopierManager>,
) {
    info!("VisualCopier: Attempting to load copier state at startup...");
    match load_copier_manager_from_file() {
        Ok(loaded_manager) => {
            info!("VisualCopier: Successfully loaded/parsed manager state file.");
            *manager = loaded_manager;
        }
        Err(e) => {
             error!("VisualCopier: Failed to load copier state: {}. Using default state.", e);
             *manager = VisualCopierManager::default();
        }
    }

    manager.recalculate_next_id();
    manager.reset_transient_state();

    info!(
        "VisualCopier: Initial state loaded: {} tasks, Top From: {:?}, Top To: {:?}, CopyOnExit: {}. Next ID: {}.",
        manager.copy_tasks.len(),
        manager.top_panel_from_folder,
        manager.top_panel_to_folder,
        manager.copy_top_panel_on_exit,
        manager.next_id
    );
}
// --- End Startup system ---

// --- REMOVED perform_actions_on_exit_system ---

// --- NEW SYSTEM: Handles RequestAppExit, performs sync copy, sends AppExit ---
fn custom_exit_handler_system(
    mut request_exit_reader: EventReader<RequestAppExit>,
    mut app_exit_writer: EventWriter<AppExit>,
    // Use Res, not ResMut, as state should be saved by immediate saves now
    // If resetting the flag after copy is desired, need ResMut & save again.
    manager: Res<VisualCopierManager>,
    // Optional: Add ResMut manager and state_changed_writer if resetting flag after copy
    // mut manager_mut: ResMut<VisualCopierManager>,
    // mut state_changed_writer: EventWriter<VisualCopierStateChanged>,
) {
    if request_exit_reader.read().next().is_some() {
        // Ensure this doesn't run multiple times if RequestAppExit is sent rapidly
        // Usually not an issue, but could add a local boolean flag if needed.

        // 1. Perform synchronous copy if manager.copy_top_panel_on_exit is true
        let mut copy_error: Option<String> = None;
        if manager.copy_top_panel_on_exit {
            if let (Some(from), Some(to)) = (&manager.top_panel_from_folder, &manager.top_panel_to_folder) {
                info!("VisualCopier: Performing synchronous copy before exiting...");
                // BLOCKING CALL - UI WILL FREEZE
                match crate::visual_copier::executers::execute_single_copy_operation(from, to, "Sync Copy on Exit") {
                    Ok(msg) => {
                        info!("VisualCopier: Sync copy on exit successful: {}", msg);
                        // --- Optional: Reset flag and trigger immediate save ---
                        // manager_mut.copy_top_panel_on_exit = false;
                        // state_changed_writer.send(VisualCopierStateChanged);
                        // Bevy might need an extra frame/tick for the save system to run
                        // which might complicate exiting immediately.
                        // For simplicity now, we won't auto-reset the flag.
                    }
                    Err(e) => {
                        let err_msg = format!("VisualCopier: Sync copy on exit FAILED: {}", e);
                        error!("{}", err_msg);
                        copy_error = Some(err_msg);
                        // Decide if failure should prevent exit?
                        // For now, we proceed to exit even on copy failure.
                    }
                }
            } else {
                warn!("VisualCopier: 'Copy on Exit' true, but paths not set. Skipping copy.");
            }
        } else {
             info!("VisualCopier: 'Copy on Exit' is false. Skipping sync copy.");
        }

        // 2. Save final state (Potentially redundant if immediate saves are reliable)
        // If not resetting the flag, this final save is less critical.
        // If resetting the flag, an immediate save should be triggered by VisualCopierStateChanged.
        // info!("VisualCopier: Saving final copier state before exiting...");
        // match save_copier_manager_to_file(&manager) { /* ... */ }

        // 3. Send the actual AppExit event
        if let Some(err) = copy_error {
             warn!("VisualCopier: Requesting app termination following exit request, but sync copy failed: {}", err);
        } else {
             info!("VisualCopier: All pre-exit tasks done. Requesting actual app termination.");
        }
        app_exit_writer.send(AppExit::Success); // Use success variant or appropriate code
    }
}
// --- END NEW SYSTEM ---