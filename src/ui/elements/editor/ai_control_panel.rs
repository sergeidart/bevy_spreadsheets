// src/ui/elements/editor/ai_control_panel.rs
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;
use std::time::Duration;

use crate::sheets::events::AiTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::{KEYRING_API_KEY_USERNAME, KEYRING_SERVICE_NAME};
use super::state::{AiModeState, EditorWindowState};
use crate::ui::systems::SendEvent;

/// Shows the AI mode control panel (buttons for Send, Cancel, Review).
pub(super) fn show_ai_control_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>, // Pass as immutable ref
    selected_sheet_name_clone: &Option<String>, // Pass as immutable ref
    runtime: &TokioTasksRuntime, // Pass as immutable ref
    registry: &SheetRegistry, // Pass as immutable ref
    commands: &mut Commands, // Pass as mutable ref
) {
    ui.horizontal(|ui| {
        ui.label(format!("✨ AI Mode: {:?}", state.ai_mode));
        ui.separator();

        // --- Send Button ---
        let can_send = state.ai_mode == AiModeState::Preparing
            && !state.ai_selected_rows.is_empty();
        let send_button_text = if state.ai_selected_rows.len() > 1 {
            format!("🚀 Send to AI ({} Rows)", state.ai_selected_rows.len())
        } else {
            "🚀 Send to AI (1 Row)".to_string()
        };

        if ui
            .add_enabled(can_send, egui::Button::new(send_button_text))
            .on_hover_text("Send selected row(s) for AI processing (currently processes first selected)")
            .clicked()
        {
            info!(
                "'Send to AI' clicked. Selected rows: {:?}",
                state.ai_selected_rows
            );
            state.ai_mode = AiModeState::Submitting;
            ui.ctx().request_repaint(); // Request repaint to show spinner

            // Clone necessary data for the async task
            let task_category = selected_category_clone.clone();
            let task_sheet_name =
                selected_sheet_name_clone.clone().unwrap_or_default();
            // Process only the first selected row for now
            let task_row_index = state
                .ai_selected_rows
                .iter()
                .next()
                .cloned()
                .unwrap_or_default(); // Default to 0 if somehow empty

            // Get data needed for the task immutably
            let (general_rule, column_contexts, row_data) = {
                // No need for second immutable borrow, registry is already immutable here
                let sheet_data =
                    registry.get_sheet(&task_category, &task_sheet_name);
                let metadata = sheet_data.and_then(|d| d.metadata.as_ref());
                let rule = metadata.and_then(|m| m.ai_general_rule.clone());
                let contexts: Vec<Option<String>> = metadata
                    .map(|m| {
                        m.columns
                            .iter()
                            .map(|c| c.ai_context.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                let data = sheet_data
                    .and_then(|d| d.grid.get(task_row_index))
                    .cloned()
                    .unwrap_or_default();
                (rule, contexts, data)
            };

            // Spawn a temporary entity to hold the SendEvent component
            let commands_entity = commands.spawn_empty().id();

            // --- Spawn the Async Task ---
            runtime.spawn_background_task(move |mut ctx| async move {
                info!(
                    "Background AI task started for sheet '{:?}/{}' row: {}",
                    task_category, task_sheet_name, task_row_index
                );

                struct TaskResultData {
                    original_row_index: usize,
                    result: Result<Vec<String>, String>,
                }

                // 1. Get API Key
                let api_key_result: Result<String, String> = match keyring::Entry::new(
                    KEYRING_SERVICE_NAME,
                    KEYRING_API_KEY_USERNAME,
                ) {
                    Ok(entry) => match entry.get_password() {
                        Ok(key) if !key.is_empty() => Ok(key),
                        Ok(_) => Err("API Key is empty!".to_string()),
                        Err(e) => Err(format!(
                            "Failed to get API key from keyring: {}",
                            e
                        )),
                    },
                    Err(e) => Err(format!("Failed to create keyring entry: {}", e)),
                };

                let mut task_result_data = TaskResultData {
                    original_row_index: task_row_index,
                    result: Err("Task did not complete.".to_string()),
                };

                match api_key_result {
                    Ok(_api_key) => {
                        info!("Retrieved API Key: [REDACTED]");

                        // 3. Format Prompt (Placeholder for actual LLM call)
                        let prompt = format!(
                                     "General Rule: {}\nColumn Contexts: {:?}\nRow Data: {:?}\n\nTask: Apply the rule and contexts to the Row Data. Return ONLY the modified row data as a JSON array of strings.",
                                     general_rule.unwrap_or_else(|| "None".to_string()), column_contexts, row_data
                                 );
                        info!("Formatted Prompt:\n{}", prompt);

                        // 4. Make API Call (Placeholder/Simulation)
                        info!("Simulating API call...");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        // Simulate success/failure and response
                        let success = rand::random::<f32>() > 0.2; // 80% success rate
                        let simulated_response_str = if success {
                            // Simple processing: prepend "Processed: "
                            let processed_data: Vec<String> = row_data
                                .iter()
                                .map(|s| format!("Processed: {}", s))
                                .collect();
                            serde_json::to_string(&processed_data).ok()
                        } else {
                            None // Simulate failure
                        };

                        // Parse the simulated response
                        task_result_data.result = match simulated_response_str {
                            Some(json_str) => {
                                serde_json::from_str(&json_str).map_err(|e| {
                                    format!(
                                        "Failed to parse AI JSON response: {}",
                                        e
                                    )
                                })
                            }
                            None => Err(
                                "AI simulation failed or returned empty/invalid response."
                                    .to_string(),
                            ),
                        };
                        info!("Simulated API call finished.");
                    }
                    Err(err_msg) => {
                        error!("{}", err_msg);
                        task_result_data.result = Err(err_msg);
                    }
                };

                // 5. Send Result Back via Event using Commands on the main thread
                ctx.run_on_main_thread(move |mut world_ctx| {
                    info!("Sending AiTaskResult event via Commands.");
                    // Insert the component onto the temporary entity
                    world_ctx.world.commands().entity(commands_entity).insert(
                        SendEvent::<AiTaskResult> {
                            event: AiTaskResult {
                                original_row_index: task_result_data
                                    .original_row_index,
                                result: task_result_data.result,
                            },
                        },
                    );
                })
                .await;
            }); // End spawn_background_task
        } // End Send button clicked block

        // --- Cancel Button ---
        if state.ai_mode == AiModeState::Preparing
            || state.ai_mode == AiModeState::ResultsReady
        {
            if ui.button("❌ Cancel AI Mode").clicked() {
                info!("Cancelling AI mode.");
                // Reset state directly here is okay as it's a simple state change
                state.ai_mode = AiModeState::Idle;
                state.ai_selected_rows.clear();
                state.ai_suggestions.clear();
                state.ai_review_queue.clear();
                state.ai_current_review_index = None;
            }
        }

        // --- Review Button ---
        if state.ai_mode == AiModeState::ResultsReady {
            let num_results = state.ai_suggestions.len();
            if ui
                .add_enabled(
                    num_results > 0,
                    egui::Button::new(format!("🧐 Review Suggestions ({})", num_results)),
                )
                .clicked()
            {
                info!("Starting review process...");
                state.ai_mode = AiModeState::Reviewing;
                // Prepare review queue
                state.ai_review_queue =
                    state.ai_suggestions.keys().cloned().collect();
                state.ai_review_queue.sort_unstable(); // Ensure consistent review order
                state.ai_current_review_index = Some(0); // Start at the first item
            }
        }

        // --- Spinner / Loading Indicator ---
        if state.ai_mode == AiModeState::Submitting {
            ui.spinner();
        }
    }); // End AI mode horizontal layout
}