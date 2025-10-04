// Left side of AI control panel: context & send controls (refactored stub)
// This file will progressively replace inline monolith logic in `ai_control_panel.rs`.
use bevy::prelude::*;
use bevy_egui::egui;

use crate::{
	sheets::resources::SheetRegistry,
	ui::elements::editor::state::{EditorWindowState, AiModeState},
	SessionApiKey,
};
use super::ai_panel::send_selected_rows;
use bevy_tokio_tasks::TokioTasksRuntime;

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_left_panel(
	ui: &mut egui::Ui,
	state: &mut EditorWindowState,
	registry: &SheetRegistry,
	_selected_category: &Option<String>,
	selected_sheet: &Option<String>,
	session_api_key: &SessionApiKey,
)
{
	draw_left_panel_impl(ui, state, registry, _selected_category, selected_sheet, session_api_key, None, None);
}

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
) {
	let selection_allowed = matches!(state.ai_mode, AiModeState::Preparing | AiModeState::ResultsReady);
	let has_api = session_api_key.0.is_some();
	let can_send = selection_allowed && has_api;
	let send_button_text = "🚀 Send to AI";
	let mut hover_text_send = if has_api { "Send selected row(s) for AI processing".to_string() } else { "API Key not set".to_string() };
	if state.ai_selected_rows.is_empty() { hover_text_send = "No rows selected: click to open prompt popup".to_string(); }
	if ui.add_enabled(can_send, egui::Button::new(send_button_text)).on_hover_text(hover_text_send).clicked() {
		if state.ai_selected_rows.is_empty() { state.show_ai_prompt_popup = true; ui.ctx().request_repaint(); }
		else if let (Some(rt), Some(cmds)) = (runtime, commands) { // perform batch send
			// move to submitting state and spawn task via helper
			send_selected_rows(state, registry, rt, cmds, session_api_key, None);
			ui.ctx().request_repaint();
		}
	}

	let status_text = match state.ai_mode { AiModeState::Preparing => format!("Preparing ({} Rows)", state.ai_selected_rows.len()), AiModeState::Submitting => "Submitting".to_string(), AiModeState::ResultsReady => "Results Ready".to_string(), AiModeState::Reviewing => "Reviewing".to_string(), AiModeState::Idle => String::new() };
	if !status_text.is_empty() { ui.label(status_text); }

	if ui.add_enabled(selected_sheet.is_some(), egui::Button::new("⚙")).on_hover_text("Edit per-sheet AI model and context").clicked() {
		state.ai_rule_popup_last_category = None; state.ai_rule_popup_last_sheet = None; state.ai_rule_popup_needs_init = true; state.show_ai_rule_popup = true;
	}
}
