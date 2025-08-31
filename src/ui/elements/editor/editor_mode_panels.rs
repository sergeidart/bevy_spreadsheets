// src/ui/elements/editor/editor_mode_panels.rs
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState, SheetInteractionState};
use crate::ui::elements::editor::ai_control_panel::show_ai_control_panel;
use crate::ui::elements::editor::ai_review_ui::draw_inline_ai_review_panel;
use crate::ui::elements::editor::ai_batch_review_ui::draw_ai_batch_review_panel;
use crate::ui::elements::top_panel::controls::delete_mode_panel::show_delete_mode_active_controls;
use super::main_editor::SheetEventWriters; // Assuming SheetEventWriters is made public or moved
use crate::SessionApiKey;


#[allow(clippy::too_many_arguments)]
pub(super) fn show_active_mode_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    current_category_clone: &Option<String>,
    current_sheet_name_clone: &Option<String>,
    runtime: &TokioTasksRuntime,
    registry: &SheetRegistry,
    commands: &mut Commands,
    session_api_key: &SessionApiKey,
    sheet_writers: &mut SheetEventWriters, // For cell updates during review & delete mode
) -> bool { // Returns true if any panel was shown that might need a separator
    let mut panel_shown = false;

    if state.current_interaction_mode == SheetInteractionState::AiModeActive &&
       matches!(state.ai_mode, AiModeState::Preparing | AiModeState::Submitting | AiModeState::ResultsReady) {
        ui.separator();
        show_ai_control_panel(
            ui,
            state,
            current_category_clone,
            current_sheet_name_clone,
            runtime,
            registry,
            commands,
            session_api_key,
        );
        panel_shown = true;
     }

    if state.current_interaction_mode == SheetInteractionState::DeleteModeActive {
         if !panel_shown { ui.separator(); }
         show_delete_mode_active_controls(
             ui,
             state,
             crate::ui::elements::top_panel::controls::delete_mode_panel::DeleteModeEventWriters {
                delete_rows_event_writer: &mut sheet_writers.delete_rows,
                delete_columns_event_writer: &mut sheet_writers.delete_columns,
             }
         );
         panel_shown = true;
    }

    if panel_shown {
        ui.separator();
    }

    // Review panel is separate as it replaces the main table view
    if state.current_interaction_mode == SheetInteractionState::AiModeActive && state.ai_mode == AiModeState::Reviewing {
        if current_sheet_name_clone.is_some() {
            if state.ai_batch_review_active {
                draw_ai_batch_review_panel(ui, state, current_category_clone, current_sheet_name_clone, registry, &mut sheet_writers.cell_update, &mut sheet_writers.add_row);
            } else {
                draw_inline_ai_review_panel(ui, state, current_category_clone, current_sheet_name_clone, registry, &mut sheet_writers.cell_update);
            }
            ui.add_space(5.0);
        } else {
            warn!("In Review Mode but no sheet selected. Exiting review mode.");
            super::ai_helpers::exit_review_mode(state); // Assuming ai_helpers is accessible
        }
    }
    panel_shown
}