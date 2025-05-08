// src/visual_copier/handler.rs

use bevy::prelude::*;
use rfd::FileDialog;

use super::io::save_copier_manager_to_file; // Keep save import
use super::resources::VisualCopierManager;
use super::events::*; // Imports all events from events.rs


/// Handles the `AddNewCopyTaskEvent` to add a new task to the manager.
pub(crate) fn handle_add_new_copy_task_event_system(
    mut events: EventReader<AddNewCopyTaskEvent>,
    mut manager: ResMut<VisualCopierManager>,
    mut state_changed_writer: EventWriter<VisualCopierStateChanged>,
) {
    let mut changed = false;
    for _event in events.read() {
        let new_id = manager.get_next_id();
        manager.copy_tasks.push(super::resources::CopyTask::new(new_id));
        info!("VisualCopier: Added new copy task with ID {}", new_id);
        changed = true;
    }
    if changed {
        state_changed_writer.send(VisualCopierStateChanged);
    }
}

/// Handles the `RemoveCopyTaskEvent` to remove a task from the manager.
pub(crate) fn handle_remove_copy_task_event_system(
    mut events: EventReader<RemoveCopyTaskEvent>,
    mut manager: ResMut<VisualCopierManager>,
    mut state_changed_writer: EventWriter<VisualCopierStateChanged>,
) {
    let mut changed = false;
    for event in events.read() {
        let id_to_remove = event.0;
        let initial_len = manager.copy_tasks.len();
        manager.copy_tasks.retain(|task| task.id != id_to_remove);
        if manager.copy_tasks.len() < initial_len {
            info!("VisualCopier: Removed copy task with ID {}", id_to_remove);
            changed = true;
        } else {
            warn!(
                "VisualCopier: Attempted to remove non-existent task with ID {}",
                id_to_remove
            );
        }
    }
    if changed {
        state_changed_writer.send(VisualCopierStateChanged);
    }
}

/// Handles the `PickFolderRequest` event to show a folder dialog.
/// Sends a `FolderPickedEvent` with the result.
pub(crate) fn handle_pick_folder_request_system(
    mut events: EventReader<PickFolderRequest>,
    mut folder_picked_writer: EventWriter<FolderPickedEvent>,
) {
    if let Some(event) = events.read().next() {
        info!("VisualCopier: Received PickFolderRequest: {:?}", event);
        let picked_path = FileDialog::new().pick_folder();
        folder_picked_writer.send(FolderPickedEvent {
            for_task_id: event.for_task_id,
            is_start_folder: event.is_start_folder,
            path: picked_path,
        });
    }
}

/// Handles the `FolderPickedEvent` to send specific update events.
pub(crate) fn handle_folder_picked_event_system(
    mut events: EventReader<FolderPickedEvent>,
    mut update_task_start_folder_writer: EventWriter<UpdateTaskStartFolderEvent>,
    mut update_task_end_folder_writer: EventWriter<UpdateTaskEndFolderEvent>,
    mut update_top_panel_from_folder_writer: EventWriter<UpdateTopPanelFromFolderEvent>,
    mut update_top_panel_to_folder_writer: EventWriter<UpdateTopPanelToFolderEvent>,
) {
    for event in events.read() {
        let path_display = event
            .path
            .as_ref()
            .map_or_else(|| "None (Cancelled)".to_string(), |p| p.display().to_string());
        info!(
            "VisualCopier: Processing FolderPickedEvent for {:?} (start: {}), path: {}",
            event.for_task_id, event.is_start_folder, path_display
        );

        if let Some(task_id) = event.for_task_id {
            if event.is_start_folder {
                update_task_start_folder_writer.send(UpdateTaskStartFolderEvent {
                    task_id,
                    path: event.path.clone(),
                });
            } else {
                update_task_end_folder_writer.send(UpdateTaskEndFolderEvent {
                    task_id,
                    path: event.path.clone(),
                });
            }
        } else {
            // This is for the top panel
            if event.is_start_folder {
                update_top_panel_from_folder_writer.send(UpdateTopPanelFromFolderEvent {
                    path: event.path.clone(),
                });
            } else {
                update_top_panel_to_folder_writer.send(UpdateTopPanelToFolderEvent {
                    path: event.path.clone(),
                });
            }
        }
    }
}

// --- Mutator Systems for specific folder updates ---

/// Applies `UpdateTaskStartFolderEvent` to the `VisualCopierManager`.
pub(crate) fn apply_task_start_folder_update_system(
    mut events: EventReader<UpdateTaskStartFolderEvent>,
    mut manager: ResMut<VisualCopierManager>,
    mut state_changed_writer: EventWriter<VisualCopierStateChanged>,
) {
    let mut changed = false;
    for event in events.read() {
        if let Some(task) = manager.copy_tasks.iter_mut().find(|t| t.id == event.task_id) {
            if task.start_folder != event.path {
                task.start_folder = event.path.clone();
                task.status = if event.path.is_some() {
                    "Start folder set.".to_string()
                } else {
                    "Start folder selection cancelled/cleared.".to_string()
                };
                info!("VisualCopier: Task {} start folder updated: {:?}", event.task_id, event.path);
                changed = true;
            }
        } else {
            warn!("VisualCopier: Task {} not found for start folder update.", event.task_id);
        }
    }
    if changed {
        state_changed_writer.send(VisualCopierStateChanged);
    }
}

/// Applies `UpdateTaskEndFolderEvent` to the `VisualCopierManager`.
pub(crate) fn apply_task_end_folder_update_system(
    mut events: EventReader<UpdateTaskEndFolderEvent>,
    mut manager: ResMut<VisualCopierManager>,
    mut state_changed_writer: EventWriter<VisualCopierStateChanged>,
) {
    let mut changed = false;
    for event in events.read() {
        if let Some(task) = manager.copy_tasks.iter_mut().find(|t| t.id == event.task_id) {
             if task.end_folder != event.path {
                task.end_folder = event.path.clone();
                task.status = if event.path.is_some() {
                    "End folder set.".to_string()
                } else {
                    "End folder selection cancelled/cleared.".to_string()
                };
                info!("VisualCopier: Task {} end folder updated: {:?}", event.task_id, event.path);
                changed = true;
            }
        } else {
            warn!("VisualCopier: Task {} not found for end folder update.", event.task_id);
        }
    }
    if changed {
        state_changed_writer.send(VisualCopierStateChanged);
    }
}

/// Applies `UpdateTopPanelFromFolderEvent` to the `VisualCopierManager`.
pub(crate) fn apply_top_panel_from_folder_update_system(
    mut events: EventReader<UpdateTopPanelFromFolderEvent>,
    mut manager: ResMut<VisualCopierManager>,
    mut state_changed_writer: EventWriter<VisualCopierStateChanged>,
) {
    let mut changed = false;
    for event in events.read() {
        if manager.top_panel_from_folder != event.path {
            manager.top_panel_from_folder = event.path.clone();
            manager.top_panel_copy_status = if event.path.is_some() {
                "Top panel 'From' folder set.".to_string()
            } else {
                "Top panel 'From' folder selection cancelled/cleared.".to_string()
            };
            info!("VisualCopier: Top panel 'From' folder updated: {:?}", event.path);
            changed = true;
        }
    }
    if changed {
        state_changed_writer.send(VisualCopierStateChanged);
    }
}

/// Applies `UpdateTopPanelToFolderEvent` to the `VisualCopierManager`.
pub(crate) fn apply_top_panel_to_folder_update_system(
    mut events: EventReader<UpdateTopPanelToFolderEvent>,
    mut manager: ResMut<VisualCopierManager>,
    mut state_changed_writer: EventWriter<VisualCopierStateChanged>,
) {
    let mut changed = false;
    for event in events.read() {
        if manager.top_panel_to_folder != event.path {
            manager.top_panel_to_folder = event.path.clone();
            manager.top_panel_copy_status = if event.path.is_some() {
                "Top panel 'To' folder set.".to_string()
            } else {
                "Top panel 'To' folder selection cancelled/cleared.".to_string()
            };
            info!("VisualCopier: Top panel 'To' folder updated: {:?}", event.path);
            changed = true;
        }
    }
    if changed {
        state_changed_writer.send(VisualCopierStateChanged);
    }
}

// --- End Mutator Systems ---


/// Handles the `ReverseTopPanelFoldersEvent`.
pub(crate) fn handle_reverse_top_panel_folders_event_system(
    mut events: EventReader<ReverseTopPanelFoldersEvent>,
    mut manager: ResMut<VisualCopierManager>,
    mut state_changed_writer: EventWriter<VisualCopierStateChanged>,
) {
    let mut event_occurred = false;
    for _event in events.read() {
        event_occurred = true;
        break;
    }

    if event_occurred {
        let temp = manager.top_panel_from_folder.take();
        manager.top_panel_from_folder = manager.top_panel_to_folder.take();
        manager.top_panel_to_folder = temp;

        manager.top_panel_copy_status = "Folders reversed.".to_string();
        info!("VisualCopier: Top panel folders reversed.");
        state_changed_writer.send(VisualCopierStateChanged);
    }
}

/// Handles `QueueCopyTaskEvent` - marks a task as queued.
pub(crate) fn handle_queue_copy_task_event_system(
    mut events: EventReader<QueueCopyTaskEvent>,
    mut manager: ResMut<VisualCopierManager>,
) {
    let task_ids_to_queue: Vec<usize> = events.read().map(|event| event.0).collect();
    for task_id in task_ids_to_queue {
        if let Some(task) = manager.copy_tasks.iter_mut().find(|t| t.id == task_id) {
            if task.start_folder.is_some() && task.end_folder.is_some() {
                if !task.status.starts_with("Copying...") && !task.status.starts_with("Queued...") {
                    task.status = "Queued...".to_string();
                    info!("VisualCopier: Task {} queued for copy.", task_id);
                } else {
                    info!("VisualCopier: Task {} already copying or queued.", task_id);
                }
            } else {
                task.status = "Error: Both folders must be set to queue.".to_string();
                warn!("VisualCopier: Cannot queue Task {}: Folders not set.", task_id);
            }
        } else {
            warn!("VisualCopier: Cannot queue Task {}: Not found.", task_id);
        }
    }
}

/// Handles `QueueTopPanelCopyEvent` - marks the top panel copy as queued.
pub(crate) fn handle_queue_top_panel_copy_event_system(
    mut events: EventReader<QueueTopPanelCopyEvent>,
    mut manager: ResMut<VisualCopierManager>,
) {
    let mut event_received = false;
    for _event in events.read() {
        event_received = true;
        break;
    }
    if event_received {
        if manager.top_panel_from_folder.is_some() && manager.top_panel_to_folder.is_some() {
            if !manager.top_panel_copy_status.starts_with("Copying...")
                && !manager.top_panel_copy_status.starts_with("Queued...")
            {
                manager.top_panel_copy_status = "Queued...".to_string();
                info!("VisualCopier: Top panel copy queued.");
            } else {
                info!("VisualCopier: Top panel copy already copying or queued.");
            }
        } else {
            manager.top_panel_copy_status =
                "Error: Both 'From' and 'To' folders must be set.".to_string();
            warn!("VisualCopier: Cannot queue top panel copy: Folders not set.");
        }
    }
}

/// Handles `QueueAllCopyTasksEvent` - marks all valid tasks as queued.
pub(crate) fn handle_queue_all_copy_tasks_event_system(
    mut events: EventReader<QueueAllCopyTasksEvent>,
    mut manager: ResMut<VisualCopierManager>,
) {
    let mut process_request = false;
    for _event in events.read() {
        process_request = true;
        break;
    }
    if !process_request {
        return;
    }
    let mut count = 0;
    for task in manager.copy_tasks.iter_mut() {
        if task.start_folder.is_some() && task.end_folder.is_some() {
            if !task.status.starts_with("Copying...") && !task.status.starts_with("Queued...") {
                task.status = "Queued...".to_string();
                count += 1;
            }
        }
    }
    if count > 0 {
        info!("VisualCopier: Queued {} tasks for 'Copy All'.", count);
    } else {
        info!("VisualCopier: 'Copy All' requested, but no tasks were eligible for queueing.");
    }
}

/// Handles `CopyOperationResultEvent` to update task statuses.
pub(crate) fn handle_copy_operation_result_event_system(
    mut events: EventReader<CopyOperationResultEvent>,
    mut manager: ResMut<VisualCopierManager>,
) {
    for event in events.read() {
        info!(
            "VisualCopier: Received CopyOperationResultEvent: TaskID {:?}, Success: {}",
            event.task_id,
            event.result.is_ok()
        );
        match event.task_id {
            Some(task_id) => {
                if let Some(task) = manager.copy_tasks.iter_mut().find(|t| t.id == task_id) {
                    if task.status.starts_with("Copying...") {
                        match &event.result {
                            Ok(success_msg) => task.status = success_msg.clone(),
                            Err(e) => {
                                let error_string = e.to_string();
                                task.status = format!("Error: {}", error_string);
                                error!("VisualCopier: Error copying task {}: {}", task.id, error_string);
                            }
                        }
                    } else {
                        info!(
                            "VisualCopier: Ignoring stale copy result for Task {} (current status: '{}')",
                            task_id, task.status
                        );
                    }
                } else {
                    warn!(
                        "VisualCopier: Received copy result for non-existent task ID {}",
                        task_id
                    );
                }
            }
            None => { // This is for the Top Panel Copy
                if manager.top_panel_copy_status.starts_with("Copying...") {
                     match &event.result {
                        Ok(success_msg) => manager.top_panel_copy_status = success_msg.clone(),
                        Err(e) => {
                            let error_string = e.to_string();
                            manager.top_panel_copy_status = format!("Error: {}", error_string);
                            error!("VisualCopier: Error in top‑panel copy: {}", error_string);
                        }
                    }
                } else {
                    info!(
                        "VisualCopier: Ignoring stale copy result for Top Panel (current status: '{}')",
                        manager.top_panel_copy_status
                    );
                }
            }
        }
    }
}

/// Saves the VisualCopierManager state whenever a `VisualCopierStateChanged` event is received.
pub(crate) fn handle_visual_copier_state_change_and_save_system(
    mut events: EventReader<VisualCopierStateChanged>,
    manager: Res<VisualCopierManager>, // Use Res, not ResMut if only reading for save
) {
    if !events.is_empty() {
        events.clear(); // Consume all events for this frame
        info!("VisualCopier: State changed, attempting immediate save...");
        match save_copier_manager_to_file(&manager) {
            Ok(_) => info!("VisualCopier: Successfully saved copier state after change."),
            Err(e) => error!("VisualCopier: Failed to save copier state after change: {}", e),
        }
    }
}