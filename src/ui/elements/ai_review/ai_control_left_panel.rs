// Left side of AI control panel: context & send controls (refactored stub)
// This file will progressively replace inline monolith logic in `ai_control_panel.rs`.
use bevy::prelude::*;
use bevy_egui::egui;

use super::ai_panel::send_selected_rows;
use crate::sheets::systems::ai::processor::{DirectorSession, start_director_session_v2};
use crate::{
    sheets::resources::SheetRegistry,
    ui::elements::editor::state::{AiModeState, EditorWindowState},
    SessionApiKey,
};
use bevy_tokio_tasks::TokioTasksRuntime;

// Extended variant used internally when runtime/commands available for sending
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_left_panel_impl(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    _selected_category: &Option<String>,
    selected_sheet: &Option<String>,
    session_api_key: &SessionApiKey,
    runtime: Option<&TokioTasksRuntime>,
    commands: Option<&mut bevy::prelude::Commands>,
    director_session: Option<&mut DirectorSession>,
) {
    let selection_allowed = matches!(
        state.ai_mode,
        AiModeState::Preparing | AiModeState::ResultsReady
    );
    let has_api = session_api_key.0.is_some();
    let can_send = selection_allowed && has_api;
    let send_button_text = "🚀 Send to AI";
    let mut hover_text_send = if has_api {
        "Send selected row(s) for AI processing".to_string()
    } else {
        "API Key not set".to_string()
    };
    if state.ai_selected_rows.is_empty() {
        hover_text_send = "No rows selected: click to open prompt popup".to_string();
    }
    // Primary button: mouse click
    let mut trigger_batch_send = false;
    if ui
        .add_enabled(can_send, egui::Button::new(send_button_text))
        .on_hover_text(hover_text_send)
        .clicked()
    {
        if state.ai_selected_rows.is_empty() {
            state.show_ai_prompt_popup = true;
            ui.ctx().request_repaint();
        } else {
            trigger_batch_send = true;
        }
    }

    // Keyboard accelerator: Ctrl+Enter triggers the same action when not typing into a text field
    // Guard: only when sending is allowed and the UI doesn't currently want keyboard input (i.e., no TextEdit focused)
    let ctrl_enter = ui
        .ctx()
        .input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl);
    if can_send && ctrl_enter && !ui.ctx().wants_keyboard_input() {
        if state.ai_selected_rows.is_empty() {
            // Open prompt popup if nothing selected
            state.show_ai_prompt_popup = true;
            ui.ctx().request_repaint();
        } else {
            trigger_batch_send = true;
        }
    }

    // Perform batch send once if triggered
    if trigger_batch_send {
        if let (Some(rt), Some(mut_cmds)) = (runtime, commands) {
            // Use the new Director-based v2 processing if director_session is available
            if let Some(session) = director_session {
                start_director_session_v2(
                    state,
                    registry,
                    session_api_key,
                    rt,
                    mut_cmds,
                    session,
                    None,
                );
            } else {
                // Fallback to legacy send_selected_rows
                send_selected_rows(state, registry, rt, mut_cmds, session_api_key, None);
            }
            ui.ctx().request_repaint();
        }
    }

    let status_text = match state.ai_mode {
        AiModeState::Preparing => format!("Preparing ({} Rows)", state.ai_selected_rows.len()),
        AiModeState::Submitting => "Submitting".to_string(),
        AiModeState::ResultsReady => "Results Ready".to_string(),
        AiModeState::Reviewing => "Reviewing".to_string(),
        AiModeState::Idle => String::new(),
    };
    if !status_text.is_empty() {
        ui.label(status_text);
    }

    if ui
        .add_enabled(selected_sheet.is_some(), egui::Button::new("⚙"))
        .on_hover_text("Edit per-sheet AI model and context")
        .clicked()
    {
        state.ai_rule_popup_last_category = None;
        state.ai_rule_popup_last_sheet = None;
        state.ai_rule_popup_needs_init = true;
        state.show_ai_rule_popup = true;
    }
}
