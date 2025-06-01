// src/ui/elements/editor/ai_control_panel.rs
use bevy::log::{error, info, warn};
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;
use crate::sheets::definitions::{default_ai_model_id, default_grounding_with_google_search};
use crate::sheets::events::AiTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::ui::systems::SendEvent;
use crate::SessionApiKey;
use gemini_client_rs::{types::{Content, ContentPart, GenerateContentRequest, PartResponse, Role, ToolConfig,DynamicRetrieval, DynamicRetrievalConfig},GeminiClient};
use serde_json::Value as JsonValue;
use super::state::{AiModeState, EditorWindowState};

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

        // --- NEW: Settings Button ---
        if ui.button("⚙ Settings").on_hover_text("Configure API Key (Session)").clicked() {
            state.show_settings_popup = true;
        }

        // --- NEW: Edit AI Config Button ---
        if ui.button("Edit AI Config").on_hover_text("Edit sheet-specific AI model, rules, and parameters").clicked() {
            state.show_ai_rule_popup = true;
            state.ai_rule_popup_needs_init = true;
        }

        ui.separator();

        // --- Existing Status Label ---
        ui.label(format!("✨ AI Mode: {:?}", state.ai_mode));
        ui.separator();

        // --- Existing Send to AI Button Logic ---
        let can_send = (state.ai_mode == AiModeState::Preparing || state.ai_mode == AiModeState::ResultsReady) // Allow sending even if results are ready (resubmit)
            && !state.ai_selected_rows.is_empty()
            && session_api_key.0.is_some();

        let mut hover_text_send = "Send selected row(s) for AI processing (currently processes first selected)".to_string();
        if session_api_key.0.is_none() {
            hover_text_send = "API Key not set. Please set it in Settings.".to_string();
        } else if state.ai_selected_rows.is_empty() {
            hover_text_send = "Select at least one row first.".to_string();
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
            state.ai_suggestions.clear(); // Clear previous suggestions on new submission
            state.ai_review_queue.clear();
            state.current_ai_suggestion_edit_buffer = None;
            ui.ctx().request_repaint();

            // --- Task Spawning Logic (remains the same) ---
            let task_category = selected_category_clone.clone();
            let task_sheet_name =
                selected_sheet_name_clone.clone().unwrap_or_default();
            let task_row_index = state
                .ai_selected_rows
                .iter()
                .next()
                .cloned()
                .unwrap_or_default(); // Still processes first selected for now

            let (
                active_model_id,
                general_rule_opt,
                column_contexts,
                row_data,
                _generation_config,
                enable_grounding,
            ) = {
                let sheet_data_opt =
                    registry.get_sheet(&task_category, &task_sheet_name);
                let metadata_opt_ref = sheet_data_opt.and_then(|d| d.metadata.as_ref());

                let model_id = metadata_opt_ref
                    .map_or_else(default_ai_model_id, |m| m.ai_model_id.clone());

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

                let gen_conf = metadata_opt_ref.map_or(
                    (crate::sheets::definitions::default_temperature(), crate::sheets::definitions::default_top_k(), crate::sheets::definitions::default_top_p()),
                    |m| (m.ai_temperature, m.ai_top_k, m.ai_top_p)
                );

                let grounding = metadata_opt_ref
                    .and_then(|m| m.requested_grounding_with_google_search)
                    .unwrap_or_else(|| default_grounding_with_google_search().unwrap_or(true));

                (model_id, rule, contexts, data, gen_conf, grounding)
            };


            let commands_entity = commands.spawn_empty().id();
            let api_key_for_task = session_api_key.0.clone();

            runtime.spawn_background_task(move |mut ctx| async move {
                info!(
                    "Background AI task started for sheet '{:?}/{}' row: {} using model '{}'. Grounding: {}",
                    task_category, task_sheet_name, task_row_index, active_model_id, enable_grounding
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

                        ctx.run_on_main_thread(move |world_ctx| {
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
                    "Considering the overall rules (if any were provided separately as a system instruction), and the following column-specific contexts and row data:\nColumn Contexts: {:?}\nRow Data: {:?}\n\nTask: Apply these rules and contexts to the Row Data. Return ONLY the modified row data as a JSON array of strings, with each element of the array being a string value for the corresponding column.",
                    column_contexts, row_data
                );

                if system_instruction_content.is_some() {
                    info!("System Instruction will be sent to Gemini.");
                }
                info!("User Prompt for Gemini (contents):\n{}", user_prompt_text);


                let client = GeminiClient::new(api_key_value);
                let model_name_to_use = active_model_id.as_str();

                let tools_config = if enable_grounding {
                    info!("Google Search Grounding is ENABLED for this request.");
                    Some(vec![
                        ToolConfig::DynamicRetieval {
                            google_search_retrieval: DynamicRetrieval {
                                dynamic_retrieval_config: DynamicRetrievalConfig {
                                    mode: "MODE_AUTO".to_string(),
                                    dynamic_threshold: 0.5,
                                },
                            },
                        }
                    ])
                } else {
                    info!("Google Search Grounding is DISABLED for this request.");
                    None
                };

                let request = GenerateContentRequest {
                    system_instruction: system_instruction_content,
                    contents: vec![Content {
                        parts: vec![ContentPart::Text(user_prompt_text.clone())],
                        role: Role::User,
                    }],
                    tools: tools_config,
                };

                match client.generate_content(model_name_to_use, &request).await {
                    Ok(response) => {
                        let mut combined_text_from_parts = String::new();
                        if let Some(candidates) = response.candidates {
                            for candidate in candidates {
                                for part_response in candidate.content.parts {
                                    if let PartResponse::Text(text_part) = part_response {
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

                            let initial_text_for_bracket_search = if (bom_cleaned_text.starts_with("```json") || bom_cleaned_text.starts_with("```"))
                                && bom_cleaned_text.ends_with("```") {
                                let stripped = bom_cleaned_text
                                    .trim_start_matches("```json")
                                    .trim_start_matches("```")
                                    .trim_end_matches("```")
                                    .trim();
                                info!("Text after Markdown fence removal: '{}'", stripped);
                                stripped
                            } else {
                                info!("No Markdown fences detected or not matching pattern. Using text as is (after BOM/trim) for bracket search: '{}'", bom_cleaned_text);
                                bom_cleaned_text
                            };

                            let mut final_text_to_parse = initial_text_for_bracket_search;

                            if !initial_text_for_bracket_search.is_empty() {
                                let mut open_brackets = 0;
                                let mut first_start_bracket_idx_opt = None;
                                let mut matching_end_bracket_idx_opt = None;

                                for (i, char_c) in initial_text_for_bracket_search.char_indices() {
                                    if char_c == '[' {
                                        if open_brackets == 0 {
                                            first_start_bracket_idx_opt = Some(i);
                                        }
                                        open_brackets += 1;
                                    } else if char_c == ']' {
                                        if open_brackets > 0 {
                                            open_brackets -= 1;
                                            if open_brackets == 0 && first_start_bracket_idx_opt.is_some() {
                                                matching_end_bracket_idx_opt = Some(i);
                                                break;
                                            }
                                        }
                                    }
                                }

                                if let (Some(start_idx), Some(end_idx)) = (first_start_bracket_idx_opt, matching_end_bracket_idx_opt) {
                                    if end_idx > start_idx {
                                        final_text_to_parse = &initial_text_for_bracket_search[start_idx..(end_idx + 1)];
                                        info!("Successfully extracted content by first matching brackets: '{}'", final_text_to_parse);
                                    } else {
                                        info!("Bracket indices invalid after matching (start_idx={}, end_idx={}). Will attempt to parse text after markdown strip: '{}'", start_idx, end_idx, initial_text_for_bracket_search);
                                    }
                                } else {
                                    if first_start_bracket_idx_opt.is_some() && matching_end_bracket_idx_opt.is_none() {
                                         info!("Opening '[' found at index {} but no matching ']' for the first complete array. Will attempt to parse text after markdown strip: '{}'", first_start_bracket_idx_opt.unwrap(), initial_text_for_bracket_search);
                                    } else if first_start_bracket_idx_opt.is_none() {
                                         info!("No opening '[' found. Will attempt to parse text after markdown strip: '{}'", initial_text_for_bracket_search);
                                    }
                                }
                            }

                            if final_text_to_parse.is_empty() {
                                let empty_after_clean_msg = format!(
                                    "AI response was empty or became empty after cleaning/extraction. Original raw: '{}'",
                                    combined_text_from_parts
                                );
                                warn!("{}",empty_after_clean_msg);
                                task_result_data.result = Err(empty_after_clean_msg);
                            } else {
                                match serde_json::from_str::<Vec<JsonValue>>(final_text_to_parse) {
                                    Ok(json_values) => {
                                        let suggestions: Vec<String> = json_values.into_iter().map(|val| {
                                            match val {
                                                JsonValue::String(s) => s,
                                                JsonValue::Null => String::new(),
                                                JsonValue::Number(n) => n.to_string(),
                                                JsonValue::Bool(b) => b.to_string(),
                                                _ => {
                                                    warn!("Unexpected JSON value type in array, converting to string: {}", val);
                                                    val.to_string()
                                                }
                                            }
                                        }).collect();
                                        task_result_data.result = Ok(suggestions);
                                        info!("Successfully parsed AI response into suggestions.");
                                    }
                                    Err(e) => {
                                        let parse_err_msg = format!(
                                            "Failed to parse AI JSON response: {}. Text Attempted: '{}'",
                                            e, final_text_to_parse
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
                            task_result_data.raw_response = Some(no_text_msg);
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("Gemini Error: {}", e.to_string());
                        error!("{}", err_msg);
                        task_result_data.result = Err(err_msg.clone());
                        task_result_data.raw_response = Some(err_msg);
                    }
                }

                ctx.run_on_main_thread(move |world_ctx| {
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
            // --- End Task Spawning Logic ---
        }
        // --- End Send to AI Button Logic ---

        // --- Review Suggestions Button (conditionally shown) ---
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
        // --- End Review Suggestions Button ---

        if state.ai_mode == AiModeState::Submitting {
            ui.spinner();
            // ui.label("Processing with AI..."); // Label moved to status
        }
    });
}