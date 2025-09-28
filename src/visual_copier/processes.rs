// src/visual_copier/processes.rs

use bevy::prelude::*;
use bevy_tokio_tasks::TokioTasksRuntime;

use super::events::CopyOperationResultEvent;
use super::executers::execute_single_copy_operation;
use super::resources::VisualCopierManager; // Import from executers

/// System to process queued copy operations asynchronously using bevy_tokio_tasks.
pub(crate) fn process_copy_operations_system(
    mut manager: ResMut<VisualCopierManager>,
    runtime: Res<TokioTasksRuntime>,
    mut _commands: Commands, // Kept for symmetry; remove if unused warning bothers you
                             // Removed result_writer as events are sent from the spawned task directly
) {
    // --- Top Panel copy ---
    if manager.top_panel_copy_status == "Queued..." {
        manager.top_panel_copy_status = "Copying...".to_string(); // Update status before spawning
        info!("VisualCopier: Spawning async task for top panel copy...");

        if let (Some(from), Some(to)) = (
            manager.top_panel_from_folder.clone(),
            manager.top_panel_to_folder.clone(),
        ) {
            runtime.spawn_background_task(move |mut ctx| async move {
                // Execute the copy operation.
                // The result is a Result<String, CopyError>
                let result = execute_single_copy_operation(&from, &to, "Top Panel");

                // Send the result back to the main thread using an event.
                // The event will be handled by `handle_copy_operation_result_event_system`.
                ctx.run_on_main_thread(move |ctx| {
                    ctx.world.send_event(CopyOperationResultEvent {
                        task_id: None,
                        result,
                    });
                })
                .await;
            });
        } else {
            // This case might happen if folders are unset between queueing and processing.
            manager.top_panel_copy_status = "Error: Folders became unset before copy.".to_string();
            warn!("VisualCopier: Top panel folders became unset before copy task could start.");
        }
    }

    // --- Individual task copies ---
    // Iterate over tasks that are queued and ready for processing.
    for task in manager.copy_tasks.iter_mut() {
        if task.status == "Queued..." {
            if let (Some(from), Some(to)) = (task.start_folder.clone(), task.end_folder.clone()) {
                task.status = "Copying...".to_string(); // Update status before spawning
                let task_id = task.id;
                info!("VisualCopier: Spawning async task for Task {}...", task_id);

                runtime.spawn_background_task(move |mut ctx| async move {
                    let result =
                        execute_single_copy_operation(&from, &to, &format!("Task {}", task_id));

                    ctx.run_on_main_thread(move |ctx| {
                        ctx.world.send_event(CopyOperationResultEvent {
                            task_id: Some(task_id),
                            result,
                        });
                    })
                    .await;
                });
            } else {
                // This case might happen if folders are unset.
                task.status = "Error: Folders became unset before copy.".to_string();
                warn!(
                    "VisualCopier: Folders for Task {} became unset before copy task could start.",
                    task.id
                );
            }
        }
    }
}
