// src/sheets/systems/ai/structure_jobs.rs
// Logic for enqueueing and managing structure processing jobs

use bevy::prelude::*;

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{EditorWindowState, StructureNewRowContext, StructureSendJob};

/// Enqueue structure jobs for a batch result, preparing tokens for new rows
pub fn enqueue_structure_jobs_for_batch(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
) -> usize {
    state.ai_pending_structure_jobs.clear();
    state.ai_active_structure_job = None;
    state.ai_structure_results_expected = 0;
    state.ai_structure_results_received = 0;
    state.ai_structure_new_row_contexts.clear();
    state.ai_structure_new_row_token_counter = 0;

    state.ai_structure_generation_counter = state.ai_structure_generation_counter.wrapping_add(1);
    state.ai_structure_active_generation = state.ai_structure_generation_counter;
    let generation_id = state.ai_structure_active_generation;

    if state.ai_planned_structure_paths.is_empty() {
        return 0;
    }

    let Some(root_sheet) = state.ai_last_send_root_sheet.clone() else {
        return 0;
    };
    let root_category = state.ai_last_send_root_category.clone();

    let Some(sheet) = registry.get_sheet(&root_category, &root_sheet) else {
        warn!(
            "Skipping structure review planning: sheet {:?}/{} not found",
            root_category, root_sheet
        );
        return 0;
    };
    let Some(meta) = sheet.metadata.as_ref() else {
        warn!(
            "Skipping structure review planning: metadata missing for {:?}/{}",
            root_category, root_sheet
        );
        return 0;
    };

    if state.ai_last_send_root_rows.is_empty() && state.ai_new_row_reviews.is_empty() {
        return 0;
    }

    let mut expected_jobs = 0usize;
    let planned_paths = state.ai_planned_structure_paths.clone();
    for path in planned_paths {
        if path.is_empty() {
            continue;
        }
        if path.len() > 1 {
            warn!(
                "Skipping nested structure path {:?} for {:?}/{}: nested structures not supported yet",
                path, root_category, root_sheet
            );
            continue;
        }

        let label = meta.describe_structure_path(&path);
        let mut target_rows: Vec<usize> = Vec::new();

        if !state.ai_last_send_root_rows.is_empty() {
            target_rows.extend(state.ai_last_send_root_rows.iter().copied());
        }

        if !state.ai_new_row_reviews.is_empty() {
            let new_row_context_inputs: Vec<(usize, Vec<(usize, String)>)> = state
                .ai_new_row_reviews
                .iter()
                .enumerate()
                .map(|(new_idx, nr)| {
                    let non_structure_values = nr
                        .non_structure_columns
                        .iter()
                        .zip(nr.ai.iter())
                        .map(|(col_idx, value)| (*col_idx, value.clone()))
                        .collect();
                    (new_idx, non_structure_values)
                })
                .collect();

            for (new_idx, non_structure_values) in new_row_context_inputs {
                let token = state.allocate_structure_new_row_token();
                let context = StructureNewRowContext {
                    new_row_index: new_idx,
                    non_structure_values,
                };
                state.ai_structure_new_row_contexts.insert(token, context);
                target_rows.push(token);
            }
        }

        if target_rows.is_empty() {
            continue;
        }

        let job = StructureSendJob {
            root_category: root_category.clone(),
            root_sheet: root_sheet.clone(),
            structure_path: path.clone(),
            label: label.clone(),
            target_rows,
            generation_id,
        };
        expected_jobs = expected_jobs.saturating_add(1);
        state.ai_pending_structure_jobs.push_back(job);
    }

    state.ai_structure_results_expected = expected_jobs;
    state.ai_structure_results_received = 0;
    expected_jobs
}
