// src/ui/elements/editor/ai_control_panel.rs
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::sheets::events::AiTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::definitions::{SheetMetadata, ColumnDefinition, ColumnDataType, ColumnValidator};
// Import ReviewChoice and the setup_review_for_index helper
use super::state::{AiModeState, EditorWindowState, ReviewChoice};
use super::ai_helpers::{setup_review_for_index, exit_review_mode}; // Added exit_review_mode
use crate::ui::systems::SendEvent;
use crate::SessionApiKey;


#[derive(Serialize, Deserialize, Debug)]
struct AiColumnContext {
    header: String,
    original_value: String,
    data_type: String,
    validator: Option<String>,
    ai_column_context: Option<String>,
    width: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug)]
struct AiPromptData {
    general_sheet_rule: Option<String>,
    columns: Vec<AiColumnContext>,
}

pub(super) fn show_ai_control_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    selected_sheet_name_clone: &Option<String>,
    runtime: &TokioTasksRuntime,
    registry: &SheetRegistry,
    commands: &mut Commands,
    session_api_key: &SessionApiKey,
) {
    ui.horizontal(|ui| {
        ui.label(format!("✨ AI Mode: {:?}", state.ai_mode));
        ui.separator();

        let can_send = state.ai_mode == AiModeState::Preparing
            && !state.ai_selected_rows.is_empty();
        let send_button_text = if state.ai_selected_rows.len() > 1 {
            format!("🚀 Send to AI ({} Rows)", state.ai_selected_rows.len())
        } else {
            "🚀 Send to AI (1 Row)".to_string()
        };

        if ui
            .add_enabled(can_send, egui::Button::new(send_button_text))
            .on_hover_text("Send selected row(s) for AI processing (processes first selected)")
            .clicked()
        {
            info!(
                "'Send to AI' clicked. Selected rows: {:?}",
                state.ai_selected_rows
            );
            state.ai_mode = AiModeState::Submitting;
            state.ai_prompt_display = "Generating prompt...".to_string();
            ui.ctx().request_repaint();

            let task_category = selected_category_clone.clone();
            let task_sheet_name =
                selected_sheet_name_clone.clone().unwrap_or_default();
            let task_row_index = state
                .ai_selected_rows
                .iter()
                .next()
                .cloned()
                .unwrap_or_default();

            let mut ai_prompt_struct = AiPromptData {
                general_sheet_rule: None,
                columns: Vec::new(),
            };
            let mut generation_error: Option<String> = None;

            if let Some(sheet_data) = registry.get_sheet(&task_category, &task_sheet_name) {
                if let Some(metadata) = &sheet_data.metadata {
                    ai_prompt_struct.general_sheet_rule = metadata.ai_general_rule.clone();
                    if let Some(row_data) = sheet_data.grid.get(task_row_index) {
                        for (col_idx, cell_value) in row_data.iter().enumerate() {
                            if let Some(col_def) = metadata.columns.get(col_idx) {
                                ai_prompt_struct.columns.push(AiColumnContext {
                                    header: col_def.header.clone(),
                                    original_value: cell_value.clone(),
                                    data_type: col_def.data_type.to_string(),
                                    validator: col_def.validator.as_ref().map(|v| v.to_string()),
                                    ai_column_context: col_def.ai_context.clone(),
                                    width: col_def.width,
                                });
                            } else {
                                warn!("Row {} in sheet '{:?}/{}' has more cells than column definitions. Skipping extra cell data for AI prompt.", task_row_index, task_category, task_sheet_name);
                                ai_prompt_struct.columns.push(AiColumnContext {
                                    header: format!("Unknown Column {}", col_idx + 1),
                                    original_value: cell_value.clone(),
                                    data_type: "String".to_string(),
                                    validator: None,
                                    ai_column_context: None,
                                    width: None,
                                });
                            }
                        }
                    } else {
                        generation_error = Some(format!("Selected row index {} out of bounds for sheet '{:?}/{}'.", task_row_index, task_category, task_sheet_name));
                    }
                } else {
                    generation_error = Some(format!("Metadata not found for sheet '{:?}/{}'.", task_category, task_sheet_name));
                }
            } else {
                generation_error = Some(format!("Sheet '{:?}/{}' not found.", task_category, task_sheet_name));
            }

            let prompt_json_string = match serde_json::to_string_pretty(&ai_prompt_struct) {
                Ok(json_str) => json_str,
                Err(e) => {
                    generation_error = Some(format!("Failed to serialize AI prompt data: {}", e));
                    "Error generating prompt JSON.".to_string()
                }
            };
            state.ai_prompt_display = prompt_json_string.clone();

            if let Some(err_msg) = generation_error {
                error!("AI Prompt Generation Error: {}", err_msg);
                state.ai_mode = AiModeState::Preparing;
                state.ai_prompt_display = format!("Error generating prompt:\n{}", err_msg);
                return;
            }

            let commands_entity = commands.spawn_empty().id();
            let current_api_key: Option<String> = session_api_key.0.clone();

            runtime.spawn_background_task(move |mut ctx| async move {
                info!(
                    "Background AI task started for sheet '{:?}/{}' row: {}",
                    task_category, task_sheet_name, task_row_index
                );
                debug!("AI Task received prompt JSON:\n{}", prompt_json_string);

                struct TaskResultData {
                    original_row_index: usize,
                    result: Result<Vec<String>, String>,
                }

                let api_key_result: Result<String, String> = match current_api_key {
                    Some(key) if !key.is_empty() => Ok(key),
                    Some(_) => Err("API Key is empty in session!".to_string()),
                    None => Err("No API Key set in current session.".to_string()),
                };

                let mut task_result_data = TaskResultData {
                    original_row_index: task_row_index,
                    result: Err("Task did not complete.".to_string()),
                };

                match api_key_result {
                    Ok(_api_key_from_session) => {
                        info!("Simulated API Key Check (from session): [REDACTED] - OK");
                        let parsed_prompt: Result<AiPromptData, _> = serde_json::from_str(&prompt_json_string);
                        match parsed_prompt {
                            Ok(prompt_data) => {
                                info!("Simulating *SUCCESSFUL* AI call with parsed prompt data...");
                                tokio::time::sleep(Duration::from_secs(1)).await;

                                let mut processed_row: Vec<String> = Vec::new();
                                for col_ctx in prompt_data.columns.iter() {
                                    let mut current_val = col_ctx.original_value.clone();
                                    if let Some(rule) = &prompt_data.general_sheet_rule {
                                        if rule.to_lowercase().contains("uppercase all") {
                                            current_val = current_val.to_uppercase();
                                        }
                                    }
                                    if let Some(col_rule) = &col_ctx.ai_column_context {
                                        if col_rule.to_lowercase().contains("append processed") {
                                            current_val = format!("{} [AI Processed]", current_val);
                                        }
                                    }
                                    if current_val == col_ctx.original_value && !current_val.contains("[Simulated OK]") {
                                         current_val = format!("{} [Simulated OK]", current_val);
                                     }
                                    processed_row.push(current_val);
                                }
                                task_result_data.result = Ok(processed_row);
                            }
                            Err(e) => {
                                 let err_msg = format!("Internal Simulation Error: Failed to parse prompt JSON back in task: {}", e);
                                 error!("{}", err_msg);
                                 task_result_data.result = Err(err_msg);
                            }
                        }
                        info!("Simulated AI call finished.");
                    }
                    Err(err_msg) => {
                        error!("Session API Key Error: {}", err_msg);
                        task_result_data.result = Err(err_msg);
                    }
                };

                tokio::time::sleep(Duration::from_millis(150)).await;

                ctx.run_on_main_thread(move |mut world_ctx| {
                    info!("Sending AiTaskResult event via Commands.");
                    world_ctx.world.commands().entity(commands_entity).insert(
                        SendEvent::<AiTaskResult> {
                            event: AiTaskResult {
                                original_row_index: task_result_data.original_row_index,
                                result: task_result_data.result,
                            },
                        },
                    );
                })
                .await;
            });
        }

        if state.ai_mode == AiModeState::Submitting || state.ai_mode == AiModeState::ResultsReady {
            if !state.ai_prompt_display.is_empty() {
                ui.separator();
                ui.collapsing("Show AI Prompt Context", |ui| {
                    egui::ScrollArea::both().max_height(100.0).show(ui, |ui| {
                        let mut display_text_clone = state.ai_prompt_display.clone();
                        let text_edit_widget = egui::TextEdit::multiline(&mut display_text_clone)
                            .font(egui::TextStyle::Monospace)
                            .interactive(false);
                        ui.add(text_edit_widget);
                    });
                });
            }
        }

        if state.ai_mode == AiModeState::Preparing
            || state.ai_mode == AiModeState::ResultsReady
            || state.ai_mode == AiModeState::Submitting
        {
            if ui.button("❌ Cancel AI Mode").clicked() {
                // Use exit_review_mode for consistent cleanup
                exit_review_mode(state); // This will also clear suggestions, queue, etc.
                                         // and set mode to Idle
                state.ai_prompt_display.clear(); // Specifically clear prompt
            }
        }

        if state.ai_mode == AiModeState::ResultsReady {
            let num_results = state.ai_suggestions.len();
            if ui
                .add_enabled(
                    num_results > 0,
                    egui::Button::new(format!("🧐 Review Suggestions ({})", num_results)),
                )
                .clicked()
            {
                info!("Starting review process (inline)...");
                state.ai_review_queue = state.ai_suggestions.keys().cloned().collect();
                state.ai_review_queue.sort_unstable();
                state.ai_current_review_index = None; // Will be set by setup_review_for_index

                let mut first_review_setup = false;
                let mut current_idx_in_queue = 0;
                while current_idx_in_queue < state.ai_review_queue.len() {
                    if setup_review_for_index(state, current_idx_in_queue) {
                        state.ai_mode = AiModeState::Reviewing;
                        first_review_setup = true;
                        break;
                    }
                    current_idx_in_queue += 1;
                }

                if !first_review_setup {
                    warn!("Review initiation failed: No valid suggestions found in the queue to set up.");
                    exit_review_mode(state); // Clean up if no items are reviewable
                }
            }
        }

        if state.ai_mode == AiModeState::Submitting {
            ui.spinner();
        }
    });
}