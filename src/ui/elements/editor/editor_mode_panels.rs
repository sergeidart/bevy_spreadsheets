// src/ui/elements/editor/editor_mode_panels.rs
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState, SheetInteractionState};
use crate::ui::elements::editor::ai_control_panel::show_ai_control_panel;
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
        // No leading separator before AI panel per new design
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
        // No leading separator before Delete panel either
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

    // No trailing separator after mode panels

    // Review panel is separate as it replaces the main table view
    if state.current_interaction_mode == SheetInteractionState::AiModeActive && state.ai_mode == AiModeState::Reviewing {
        if current_sheet_name_clone.is_some() {
            // Only unified batch review remains
            draw_ai_batch_review_panel(ui, state, current_category_clone, current_sheet_name_clone, registry, &mut sheet_writers.cell_update, &mut sheet_writers.add_row);
            ui.add_space(5.0);
        } else {
            warn!("In Review Mode but no sheet selected. Exiting review mode.");
            state.ai_mode = AiModeState::Idle;
            state.ai_selected_rows.clear();
        }
    }
    panel_shown
}