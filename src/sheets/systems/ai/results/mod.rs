// src/sheets/systems/ai/results/mod.rs
// Main AI result handlers module - coordinates batch processing

mod context_setup;
mod duplicate_detection;
mod root_handlers;
mod row_processors;
mod structure_handlers;

pub use context_setup::setup_context_prefixes;
pub use duplicate_detection::check_for_duplicate;
pub use root_handlers::{handle_root_batch_error, handle_root_batch_result_phase1};
pub use row_processors::{process_new_rows, process_original_rows};
pub use structure_handlers::handle_structure_batch_result;

use bevy::prelude::*;

use crate::sheets::events::{AiBatchResultKind, AiBatchTaskResult, SheetOperationFeedback};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

use super::phase2_helpers;

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
        // Check if we're expecting a Phase 2 result (flag-based routing)
        if state.ai_expecting_phase2_result {
            // Phase 2 result - extract duplicate info from stored Phase 1 data
            if let Some(ref phase1) = state.ai_phase1_intermediate {
                let duplicate_indices = phase1.duplicate_indices.clone();
                let established_row_count = phase1.original_count + duplicate_indices.len();
                state.ai_expecting_phase2_result = false;
                phase2_helpers::handle_deep_review_result_phase2(
                    ev,
                    &duplicate_indices,
                    established_row_count,
                    &mut state,
                    &registry,
                    &mut feedback_writer,
                );
            } else {
                error!("Phase 2 result expected but no Phase 1 data found!");
                state.ai_expecting_phase2_result = false;
            }
        } else {
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
                    handle_root_batch_result_phase1(
                        ev,
                        &mut state,
                        &registry,
                        &mut feedback_writer,
                        &mut commands,
                        &runtime,
                        &session_api_key,
                    );
                }
                AiBatchResultKind::DeepReview { .. } => {
                    error!("Unexpected DeepReview result kind without flag set!");
                }
            }
        }
        break;
    }
}
