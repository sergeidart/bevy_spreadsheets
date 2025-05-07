// src/ui/elements/editor/ai_control_panel.rs
use bevy::log::{debug, error, info, warn};
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;

use crate::sheets::definitions::SheetMetadata;
use crate::sheets::events::AiTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::ui::systems::SendEvent;
use crate::SessionApiKey;

use gemini_client_rs::{
    types::{Content, ContentPart, GenerateContentRequest, PartResponse, Role, ToolConfig},
    GeminiClient, GeminiError,
};
// Ensure serde_json::Value is available
use serde_json::Value as JsonValue;

use super::state::{AiModeState, EditorWindowState};

/// Shows the AI mode control panel (buttons for Send, Cancel, Review).
#[allow(clippy::too_many_arguments)]
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
            && !state.ai_selected_rows.is_empty()
            && session_api_key.0.is_some();

        let mut hover_text_send = "Send selected row(s) for AI processing (currently processes first selected)".to_string();
        if session_api_key.0.is_none() {
            hover_text_send = "API Key not set. Please set it in Settings.".to_string();
        }

        let send_button_text = if state.ai_selected_rows.len() > 1 {
            format!("🚀 Send to AI ({} Rows)", state.ai_selected_rows.len())
        } else {
            "🚀 Send to AI (1 Row)".to_string()
        };

        if ui
            .add_enabled(can_send, egui::Button::new(send_button_text))
            .on_hover_text(hover_text_send)
            .clicked()
        {
            info!(
                "'Send to AI' clicked. Selected rows: {:?}",
                state.ai_selected_rows
            );
            state.ai_mode = AiModeState::Submitting;
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

            let (general_rule_opt, column_contexts, row_data) = {
                let sheet_data_opt =
                    registry.get_sheet(&task_category, &task_sheet_name);
                let metadata_opt_ref = sheet_data_opt.and_then(|d| d.metadata.as_ref());

                let rule = metadata_opt_ref.and_then(|m| m.ai_general_rule.clone());
                let contexts: Vec<Option<String>> = metadata_opt_ref
                    .map(|m| {
                        m.columns
                            .iter()
                            .map(|c| c.ai_context.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                let data = sheet_data_opt
                    .and_then(|d| d.grid.get(task_row_index))
                    .cloned()
                    .unwrap_or_default();
                
                (rule, contexts, data)
            };

            let commands_entity = commands.spawn_empty().id();
            let api_key_for_task = session_api_key.0.clone();

            runtime.spawn_background_task(move |mut ctx| async move {
                info!(
                    "Background AI task started for sheet '{:?}/{}' row: {}",
                    task_category, task_sheet_name, task_row_index
                );

                struct TaskResultData {
                    original_row_index: usize,
                    result: Result<Vec<String>, String>,
                    raw_response: Option<String>,
                }

                let mut task_result_data = TaskResultData {
                    original_row_index: task_row_index,
                    result: Err("AI task did not complete successfully.".to_string()),
                    raw_response: None,
                };
                
                let api_key_value = match api_key_for_task {
                    Some(key) if !key.is_empty() => key,
                    _ => {
                        let err_msg = "API Key not found or empty in session.".to_string();
                        error!("{}", err_msg);
                        task_result_data.result = Err(err_msg.clone());
                        task_result_data.raw_response = Some(format!("Error: {}", err_msg));
                        
                        ctx.run_on_main_thread(move |mut world_ctx| {
                            world_ctx.world.commands().entity(commands_entity).insert(
                                SendEvent::<AiTaskResult> { 
                                    event: AiTaskResult {
                                        original_row_index: task_result_data.original_row_index,
                                        result: task_result_data.result,
                                        raw_response: task_result_data.raw_response,
                                    }
                                },
                            );
                        }).await;
                        return;
                    }
                };

                let system_instruction_content: Option<Content> = 
                    general_rule_opt.map(|rule_text| Content {
                        parts: vec![ContentPart::Text(rule_text)],
                        role: Role::System,
                    });

                let user_prompt_text = format!(
                    "Considering the overall rules (if any were provided separately as a system instruction), and the following column-specific contexts and row data:\nColumn Contexts: {:?}\nRow Data: {:?}\n\nTask: Apply these rules and contexts to the Row Data. Return ONLY the modified row data as a JSON array of strings.",
                    column_contexts, row_data
                );
                if system_instruction_content.is_some() {
                    info!("System Instruction will be sent to Gemini.");
                }
                info!("User Prompt for Gemini (contents):\n{}", user_prompt_text);

                let client = GeminiClient::new(api_key_value);
                let model_name = "gemini-1.5-flash";

                let request = GenerateContentRequest {
                    system_instruction: system_instruction_content,
                    contents: vec![Content {
                        parts: vec![ContentPart::Text(user_prompt_text.clone())],
                        role: Role::User,
                    }],
                    tools: None,
                };
                
                match client.generate_content(model_name, &request).await {
                    Ok(response) => {
                        let mut combined_text_from_parts = String::new();
                        if let Some(candidates) = response.candidates {
                            for candidate in candidates {
                                for part in candidate.content.parts {
                                    if let PartResponse::Text(text_part) = part {
                                        combined_text_from_parts.push_str(&text_part);
                                    }
                                }
                            }
                        }
                        
                        task_result_data.raw_response = Some(combined_text_from_parts.clone());

                        if !combined_text_from_parts.is_empty() {
                            info!("Gemini raw response (before cleaning): '{}'", combined_text_from_parts);
                            
                            let trimmed_text = combined_text_from_parts.trim();
                            let bom_cleaned_text = if trimmed_text.starts_with('\u{FEFF}') {
                                let mut chars = trimmed_text.chars();
                                chars.next(); 
                                chars.as_str()
                            } else {
                                trimmed_text
                            };

                            // Attempt to strip Markdown fences
                            let mut text_to_parse = bom_cleaned_text;
                            if (text_to_parse.starts_with("```json") || text_to_parse.starts_with("```")) 
                                && text_to_parse.ends_with("```") {
                                text_to_parse = text_to_parse.trim_start_matches("```json"); // Handles ```json
                                text_to_parse = text_to_parse.trim_start_matches("```");    // Handles ```
                                text_to_parse = text_to_parse.trim_end_matches("```");
                                text_to_parse = text_to_parse.trim(); // Trim whitespace around the actual JSON content
                                info!("Text after Markdown fence removal: '{}'", text_to_parse);
                            } else {
                                info!("No Markdown fences detected or not matching expected pattern. Parsing as is (after BOM/trim).");
                            }
                            
                            if text_to_parse.is_empty() {
                                let empty_after_clean_msg = format!(
                                    "AI response was empty or became empty after cleaning/extraction. Original raw: '{}'",
                                    combined_text_from_parts
                                );
                                warn!("{}",empty_after_clean_msg);
                                task_result_data.result = Err(empty_after_clean_msg);
                            } else {
                                // Parse into Vec<JsonValue> first to handle nulls correctly
                                match serde_json::from_str::<Vec<JsonValue>>(text_to_parse) {
                                    Ok(json_values) => {
                                        let suggestions: Vec<String> = json_values.into_iter().map(|val| {
                                            match val {
                                                JsonValue::String(s) => s,
                                                JsonValue::Null => String::new(), // Convert JSON null to empty string
                                                JsonValue::Number(n) => n.to_string(),
                                                JsonValue::Bool(b) => b.to_string(),
                                                // For complex types (Array/Object), convert to their string representation
                                                // or handle as an error/empty string if not expected.
                                                // The prompt asks for an array of strings, so these are less likely.
                                                _ => {
                                                    warn!("Unexpected JSON value type in array, converting to string: {}", val);
                                                    val.to_string() 
                                                }
                                            }
                                        }).collect();
                                        task_result_data.result = Ok(suggestions);
                                        info!("Successfully parsed Gemini response into suggestions.");
                                    }
                                    Err(e) => {
                                        let parse_err_msg = format!(
                                            "Failed to parse AI JSON response: {}. Text Attempted: '{}'", 
                                            e, text_to_parse
                                        );
                                        error!("{}", parse_err_msg);
                                        task_result_data.result = Err(parse_err_msg);
                                    }
                                }
                            }
                        } else {
                            let no_text_msg = "AI response was empty (no text parts found).".to_string();
                            warn!("{}", no_text_msg);
                            task_result_data.result = Err(no_text_msg.clone());
                            // raw_response is already Some("") or the original empty string
                        }
                    }
                    Err(e) => { 
                        let err_msg = format!("Gemini Error: {}", e.to_string());
                        error!("{}", err_msg);
                        task_result_data.result = Err(err_msg.clone());
                        task_result_data.raw_response = Some(err_msg);
                    }
                }

                ctx.run_on_main_thread(move |mut world_ctx| {
                    info!("Sending AiTaskResult event via Commands.");
                    world_ctx.world.commands().entity(commands_entity).insert(
                        SendEvent::<AiTaskResult> {
                            event: AiTaskResult {
                                original_row_index: task_result_data.original_row_index,
                                result: task_result_data.result,
                                raw_response: task_result_data.raw_response,
                            },
                        },
                    );
                })
                .await;
            });
        }

        if state.ai_mode == AiModeState::Preparing
            || state.ai_mode == AiModeState::ResultsReady
        {
            if ui.button("❌ Cancel AI Mode").clicked() {
                info!("Cancelling AI mode.");
                super::ai_helpers::exit_review_mode(state);
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
                info!("Starting review process for {} suggestions...", num_results);
                state.ai_mode = AiModeState::Reviewing;
                state.ai_review_queue =
                    state.ai_suggestions.keys().cloned().collect();
                state.ai_review_queue.sort_unstable();
                if !state.ai_review_queue.is_empty() {
                    super::ai_helpers::setup_review_for_index(state, 0);
                } else {
                    warn!("Review initiated but no suggestions in queue. Exiting review mode.");
                    super::ai_helpers::exit_review_mode(state);
                }
            }
        }

        if state.ai_mode == AiModeState::Submitting {
            ui.spinner();
            ui.label("Processing with AI...");
        }
    });
}