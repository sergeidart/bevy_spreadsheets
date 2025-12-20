// src/sheets/systems/ai/results/mod.rs
// Main AI result handlers module - coordinates batch processing

mod context_setup;
mod duplicate_detection;
mod root_handlers;
mod structure_handlers;

pub use context_setup::setup_context_prefixes;
pub use duplicate_detection::check_for_duplicate;
pub use root_handlers::handle_batch_result;
pub use structure_handlers::handle_structure_batch_result;

use bevy::prelude::*;

use crate::sheets::events::{AiBatchResultKind, AiBatchTaskResult, SheetOperationFeedback};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

// Re-export legacy single-row handler for backwards compatibility
pub use super::legacy::handle_ai_task_results;

/// Handle batch (root + structure) AI results
/// Main entry point that routes to appropriate handlers based on result kind
pub fn handle_ai_batch_results(
    mut ev_batch: EventReader<AiBatchTaskResult>,
    mut state: ResMut<EditorWindowState>,
    registry: Res<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut commands: Commands,
    runtime: Res<bevy_tokio_tasks::TokioTasksRuntime>,
    session_api_key: Res<crate::SessionApiKey>,
) {
    if ev_batch.is_empty() {
        return;
    }

    for ev in ev_batch.read() {
        match &ev.kind {
            AiBatchResultKind::Root {
                structure_context: Some(context),
            } => {
                handle_structure_batch_result(
                    ev,
                    context,
                    &mut state,
                    &registry,
                    &mut feedback_writer,
                );
            }
            AiBatchResultKind::Root {
                structure_context: None,
            } => {
                handle_batch_result(
                    ev,
                    &mut state,
                    &registry,
                    &mut feedback_writer,
                    &mut commands,
                    &runtime,
                    &session_api_key,
                );
            }
        }
        break;
    }
}
