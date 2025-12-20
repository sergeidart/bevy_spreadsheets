// src/ui/elements/popups/ai_prompt_popup.rs
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai::processor::{DirectorSession, start_director_session_v2};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::SessionApiKey;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;
pub fn show_ai_prompt_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    runtime: &TokioTasksRuntime,
    commands: &mut Commands,
    session_api_key: &SessionApiKey,
    director_session: &mut DirectorSession,
) {
    if !state.show_ai_prompt_popup {
        return;
    }

    let mut is_open = state.show_ai_prompt_popup;
    let mut do_send = false;
    egui::Window::new("AI Prompt")
        .id(egui::Id::new("ai_prompt_popup_window"))
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut is_open)
        .show(ctx, |ui| {
            ui.label("Enter a prompt. Result rows will be treated as new AI rows.");
            ui.add_sized(
                [ui.available_width(), 120.0],
                egui::TextEdit::multiline(&mut state.ai_prompt_input)
                    .hint_text("e.g. Give me list of games released this month"),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.ai_prompt_input.trim().is_empty() && session_api_key.0.is_some(),
                        egui::Button::new("Send"),
                    )
                    .on_hover_text(if session_api_key.0.is_none() {
                        "API key missing"
                    } else {
                        "Send prompt to AI"
                    })
                    .clicked()
                {
                    do_send = true;
                }
                if ui.button("Cancel").clicked() {
                    state.show_ai_prompt_popup = false;
                }
            });
        });
    if !is_open {
        state.show_ai_prompt_popup = false;
    }

    if do_send {
        state.show_ai_prompt_popup = false;
        // Use the new Director-based v2 processing
        start_director_session_v2(
            state,
            registry,
            session_api_key,
            runtime,
            commands,
            director_session,
            Some(state.ai_prompt_input.clone()),
        );
    }
}
