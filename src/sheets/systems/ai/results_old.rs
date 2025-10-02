// src/sheets/systems/ai/results.rs
use bevy::prelude::*;
use std::collections::HashMap;

use crate::sheets::events::{
    AiBatchResultKind, AiBatchTaskResult, AiTaskResult, SheetOperationFeedback, StructureProcessingContext,
};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{
    AiModeState, EditorWindowState, NewRowReview, ReviewChoice, RowReview, StructureNewRowContext,
    StructureReviewEntry, StructureSendJob,
};

use super::utils::parse_structure_rows_from_cell;

// ---- Single-row task results (non-batch root) ----
pub fn handle_ai_task_results(
    mut ev_ai_results: EventReader<AiTaskResult>,
    mut state: ResMut<EditorWindowState>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    if ev_ai_results.is_empty() {
        return;
    }
    debug!(
        "handle_ai_task_results: processing {} event(s). Current AI Mode: {:?}",
        ev_ai_results.len(),
        state.ai_mode
    );
    if state.ai_mode != AiModeState::Submitting && state.ai_mode != AiModeState::ResultsReady {
        let event_count = ev_ai_results.len();
        info!(
            "Ignoring {} AI result(s) received while not in Submitting/ResultsReady state (current: {:?})",
            event_count, state.ai_mode
        );
        ev_ai_results.clear(); // Consume events
        return;
    }

    let mut received_at_least_one_result = false;
    let mut all_tasks_successful_this_batch = true;

    for ev in ev_ai_results.read() {
        received_at_least_one_result = true;
        info!(
            "Received AI task result for row {}. Raw response present: {}",
            ev.original_row_index,
            ev.raw_response.is_some()
        );

        if let Some(raw) = &ev.raw_response {
            state.ai_raw_output_display = raw.clone();
        } else if let Err(e) = &ev.result {
            state.ai_raw_output_display = format!(
                "Error processing AI result for row {}: {}",
                ev.original_row_index, e
            );
        }

        match &ev.result {
            Ok(suggestion) => {
                // Re-expand suggestion back to full column width using mapping stored in event
                let expanded = if !ev.included_non_structure_columns.is_empty() {
                    let max_col = *ev.included_non_structure_columns.iter().max().unwrap_or(&0);
                    let mut row_buf = vec![String::new(); max_col + 1];
                    for (i, actual_col) in ev.included_non_structure_columns.iter().enumerate() {
                        let src_index = i + ev.context_only_prefix_count; // skip context-only prefix
                        if let Some(val) = suggestion.get(src_index) {
                            if let Some(slot) = row_buf.get_mut(*actual_col) {
                                *slot = val.clone();
                            }
                        }
                    }
                    row_buf
                } else {
                    suggestion.clone()
                };
                let included = ev.included_non_structure_columns.clone();
                let mut original_snapshot: Vec<String> = Vec::with_capacity(included.len());
                let mut ai_snapshot: Vec<String> = Vec::with_capacity(included.len());
                for (logical_i, _actual_col) in included.iter().enumerate() {
                    original_snapshot.push(String::new());
                    ai_snapshot.push(expanded.get(logical_i).cloned().unwrap_or_default());
                }
                state.ai_row_reviews.push(RowReview {
                    row_index: ev.original_row_index,
                    original: original_snapshot,
                    ai: ai_snapshot,
                    choices: vec![ReviewChoice::AI; included.len()],
                    non_structure_columns: included,
                });
                state.ai_batch_review_active = true;
            }
            Err(err_msg) => {
                feedback_writer.write(SheetOperationFeedback {
                    message: format!("AI Error (Row {}): {}", ev.original_row_index, err_msg),
                    is_error: true,
                });
                if let Some(raw) = &ev.raw_response {
                    state.ai_raw_output_display = format!(
                        "Row {} Error: {}\n--- Raw Model Output ---\n{}",
                        ev.original_row_index, err_msg, raw
                    );
                }
                state.ai_output_panel_visible = true;
                all_tasks_successful_this_batch = false;
            }
        }
    }

    if received_at_least_one_result && state.ai_mode == AiModeState::Submitting {
        if all_tasks_successful_this_batch {
            state.ai_mode = AiModeState::ResultsReady;
        } else {
            state.ai_mode = AiModeState::Preparing;
            state.ai_row_reviews.clear();
        }
    }
}

// ---- Batch (root + structure) results ----
pub fn handle_ai_batch_results(
    mut ev_batch: EventReader<AiBatchTaskResult>,
    mut state: ResMut<EditorWindowState>,
    registry: Res<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    if ev_batch.is_empty() {
        return;
    }
    for ev in ev_batch.read() {
        match &ev.kind {
            AiBatchResultKind::Root { structure_context: Some(context) } => {
                handle_structure_batch_result(ev, context, &mut state, &registry, &mut feedback_writer);
            }
            AiBatchResultKind::Root { structure_context: None } => match &ev.result {
                Ok(rows) => {
                    let originals = ev.original_row_indices.len();
                    debug!(
                        "handle_ai_batch_results: received root batch rows={}, originals={}, key_prefix_count={}, included_non_structure_columns={}",
                        rows.len(),
                        originals,
                        ev.key_prefix_count,
                        ev.included_non_structure_columns.len()
                    );
                    if originals > 0 && rows.len() < originals {
                        feedback_writer.write(SheetOperationFeedback {
                            message: format!(
                                "AI batch result malformed: returned {} rows but expected at least {}",
                                rows.len(),
                                originals
                            ),
                            is_error: true,
                        });
                        continue;
                    }
                    if let Some(raw) = &ev.raw_response {
                        state.ai_raw_output_display = raw.clone();
                        // Add to call log
                        let status = format!("Completed - {} row(s) received", rows.len());
                        state.add_ai_call_log(status, Some(raw.clone()), None, false);
                    }
                    let (orig_slice, extra_slice) = if originals == 0 {
                        (&[][..], &rows[..])
                    } else {
                        rows.split_at(originals)
                    };
                    state.ai_context_only_prefix_count = ev.key_prefix_count;
                    state.ai_context_prefix_by_row.clear();
                    if !state.virtual_structure_stack.is_empty() {
                        let mut key_headers: Vec<String> = Vec::new();
                        let mut ancestors_with_keys: Vec<(Option<String>, String, usize, usize)> =
                            Vec::new();
                        for vctx in &state.virtual_structure_stack {
                            let anc_cat = vctx.parent.parent_category.clone();
                            let anc_sheet = vctx.parent.parent_sheet.clone();
                            let anc_row_idx = vctx.parent.parent_row;
                            if let Some(sheet) = registry.get_sheet(&anc_cat, &anc_sheet) {
                                if let Some(meta) = &sheet.metadata {
                                    if let Some(key_col_index) = meta
                                        .columns
                                        .iter()
                                        .enumerate()
                                        .find_map(|(_idx, c)| {
                                            if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) {
                                                c.structure_key_parent_column_index
                                            } else {
                                                None
                                            }
                                        })
                                    {
                                        if let Some(col_def) = meta.columns.get(key_col_index) {
                                            key_headers.push(col_def.header.clone());
                                        }
                                        ancestors_with_keys
                                            .push((anc_cat, anc_sheet, anc_row_idx, key_col_index));
                                    }
                                }
                            }
                        }
                        if !ancestors_with_keys.is_empty() && !key_headers.is_empty() {
                            for &row_index in ev.original_row_indices.iter() {
                                let mut pairs: Vec<(String, String)> =
                                    Vec::with_capacity(key_headers.len());
                                for (idx, (anc_cat, anc_sheet, anc_row_idx, key_col_index)) in
                                    ancestors_with_keys.iter().enumerate()
                                {
                                    let header = key_headers.get(idx).cloned().unwrap_or_default();
                                    let val = registry
                                        .get_sheet(anc_cat, anc_sheet)
                                        .and_then(|s| s.grid.get(*anc_row_idx))
                                        .and_then(|r| r.get(*key_col_index))
                                        .cloned()
                                        .unwrap_or_default();
                                    pairs.push((header, val));
                                }
                                state.ai_context_prefix_by_row.insert(row_index, pairs);
                            }
                        }
                    }

                    state.ai_row_reviews.clear();
                    state.ai_new_row_reviews.clear();
                    state.ai_structure_reviews.clear();
                    let included = ev.included_non_structure_columns.clone();
                    let (cat_ctx, sheet_ctx) = state.current_sheet_context();
                    for (i, &row_index) in ev.original_row_indices.iter().enumerate() {
                        let suggestion_full = &orig_slice[i];
                        let suggestion = if ev.key_prefix_count > 0
                            && suggestion_full.len() >= ev.key_prefix_count
                        {
                            &suggestion_full[ev.key_prefix_count..]
                        } else {
                            suggestion_full
                        };
                        if suggestion.len() < included.len() {
                            warn!(
                                "Skipping malformed original suggestion row {}: suggestion_cols={} < included_cols={} (full_len={}, key_prefix_count={})",
                                row_index,
                                suggestion.len(),
                                included.len(),
                                suggestion_full.len(),
                                ev.key_prefix_count
                            );
                            continue;
                        }
                        let mut original_snapshot: Vec<String> = Vec::with_capacity(included.len());
                        let mut ai_snapshot: Vec<String> = Vec::with_capacity(included.len());
                        let mut original_row_opt: Option<Vec<String>> = None;
                        if let Some(sheet_name) = &sheet_ctx {
                            if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                                original_row_opt = sheet_ref.grid.get(row_index).cloned();
                            }
                        }
                        for (logical_i, actual_col) in included.iter().enumerate() {
                            let orig_val = original_row_opt
                                .as_ref()
                                .and_then(|r| r.get(*actual_col))
                                .cloned()
                                .unwrap_or_default();
                            original_snapshot.push(orig_val);
                            let ai_val = suggestion.get(logical_i).cloned().unwrap_or_default();
                            ai_snapshot.push(ai_val);
                        }
                        let choices = included
                            .iter()
                            .enumerate()
                            .map(|(idx, _)| {
                                if original_snapshot.get(idx) != ai_snapshot.get(idx) {
                                    ReviewChoice::AI
                                } else {
                                    ReviewChoice::Original
                                }
                            })
                            .collect();
                        state.ai_row_reviews.push(RowReview {
                            row_index,
                            original: original_snapshot,
                            ai: ai_snapshot,
                            choices,
                            non_structure_columns: included.clone(),
                        });
                    }

                    let mut first_col_value_to_row: HashMap<String, usize> = HashMap::new();
                    if let Some(first_col_actual) = included.first() {
                        let normalize = |s: &str| s.replace(['\r', '\n'], "").trim().to_lowercase();
                        if let Some(sheet_name) = &sheet_ctx {
                            if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                                for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
                                    if let Some(val) = row.get(*first_col_actual) {
                                        let norm = normalize(val);
                                        if !norm.is_empty() {
                                            first_col_value_to_row.entry(norm).or_insert(row_idx);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    for new_row_full in extra_slice.iter() {
                        let new_row = if ev.key_prefix_count > 0
                            && new_row_full.len() >= ev.key_prefix_count
                        {
                            &new_row_full[ev.key_prefix_count..]
                        } else {
                            new_row_full
                        };
                        if new_row.len() < included.len() {
                            warn!(
                                "Skipping malformed new suggestion row (cols {} < included_cols={} full_len={} key_prefix_count={})",
                                new_row.len(),
                                included.len(),
                                new_row_full.len(),
                                ev.key_prefix_count
                            );
                            continue;
                        }
                        let mut ai_snapshot: Vec<String> = Vec::with_capacity(included.len());
                        for (logical_i, _actual_col) in included.iter().enumerate() {
                            ai_snapshot.push(new_row.get(logical_i).cloned().unwrap_or_default());
                        }

                        let mut duplicate_match_row: Option<usize> = None;
                        let mut choices: Option<Vec<ReviewChoice>> = None;
                        let mut original_for_merge: Option<Vec<String>> = None;
                        let mut merge_selected = false;
                        let merge_decided = false;
                        if let Some(first_val) = ai_snapshot.get(0) {
                            let normalized_first =
                                first_val.replace(['\r', '\n'], "").trim().to_lowercase();
                            if let Some(matched_row_index) =
                                first_col_value_to_row.get(&normalized_first)
                            {
                                duplicate_match_row = Some(*matched_row_index);
                                if let Some(sheet_name) = &sheet_ctx {
                                    if let Some(sheet_ref) =
                                        registry.get_sheet(&cat_ctx, sheet_name)
                                    {
                                        if let Some(existing_row) =
                                            sheet_ref.grid.get(*matched_row_index)
                                        {
                                            let mut orig_vec: Vec<String> =
                                                Vec::with_capacity(included.len());
                                            for actual_col in &included {
                                                orig_vec.push(
                                                    existing_row
                                                        .get(*actual_col)
                                                        .cloned()
                                                        .unwrap_or_default(),
                                                );
                                            }
                                            let ch: Vec<ReviewChoice> = orig_vec
                                                .iter()
                                                .zip(ai_snapshot.iter())
                                                .map(|(o, a)| {
                                                    if o != a {
                                                        ReviewChoice::AI
                                                    } else {
                                                        ReviewChoice::Original
                                                    }
                                                })
                                                .collect();
                                            choices = Some(ch);
                                            original_for_merge = Some(orig_vec);
                                            merge_selected = true;
                                        }
                                    }
                                }
                            }
                        }
                        state.ai_new_row_reviews.push(NewRowReview {
                            ai: ai_snapshot,
                            non_structure_columns: included.clone(),
                            duplicate_match_row,
                            choices,
                            merge_selected,
                            merge_decided,
                            original_for_merge,
                        });
                    }

                    let expected_structure_jobs =
                        enqueue_structure_jobs_for_batch(&mut state, registry.as_ref());

                    state.ai_batch_has_undecided_merge = state
                        .ai_new_row_reviews
                        .iter()
                        .any(|nr| nr.duplicate_match_row.is_some() && !nr.merge_decided);
                    state.ai_mode = AiModeState::ResultsReady;
                    if ev.prompt_only {
                        state.last_ai_prompt_only = true;
                    }
                    state.refresh_structure_waiting_state();
                    info!(
                            "Processed AI root batch result: {} originals, {} new rows (prompt_only={}, waiting_for_structures={}, structure_jobs={})",
                            originals,
                            state.ai_new_row_reviews.len(),
                            ev.prompt_only,
                            state.ai_waiting_for_structure_results,
                            expected_structure_jobs
                        );
                }
                Err(err) => {
                    if let Some(raw) = &ev.raw_response {
                        state.ai_raw_output_display = format!("Batch Error: {}\n--- Raw Model Output ---\n{}", err, raw);
                        // Add to call log
                        state.add_ai_call_log(
                            format!("Error: {}", err),
                            Some(raw.clone()),
                            None,
                            true,
                        );
                    } else {
                        state.ai_raw_output_display = format!("Batch Error: {} (no raw output returned)", err);
                        // Add to call log
                        state.add_ai_call_log(
                            format!("Error: {}", err),
                            None,
                            None,
                            true,
                        );
                    }
                    state.ai_output_panel_visible = true;
                    feedback_writer.write(SheetOperationFeedback {
                        message: format!("AI batch error: {}", err),
                        is_error: true,
                    });
                    state.ai_mode = AiModeState::Preparing;
                }
            },
        }
        break;
    }
}

fn enqueue_structure_jobs_for_batch(
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

fn handle_structure_batch_result(
    ev: &AiBatchTaskResult,
    context: &StructureProcessingContext,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
) {
    let root_category = &context.root_category;
    let root_sheet = &context.root_sheet;
    let structure_path = &context.structure_path;
    let target_rows = &context.target_rows;
    let row_partitions = &context.row_partitions;
    let generation_id = &context.generation_id;

    let root_category = root_category.clone();
    let root_sheet = root_sheet.clone();
    let structure_path = structure_path.clone();
    let target_rows = target_rows.clone();
    let mut row_partitions = row_partitions.clone();

    let Some(sheet) = registry.get_sheet(&root_category, &root_sheet) else {
        let msg = format!(
            "Structure result dropped: sheet {:?}/{} not found",
            root_category, root_sheet
        );
        error!("{}", msg);
        feedback_writer.write(SheetOperationFeedback {
            message: msg,
            is_error: true,
        });
        state.ai_active_structure_job = None;
        state.mark_structure_result_received();
        return;
    };

    let Some(meta) = sheet.metadata.as_ref() else {
        let msg = format!(
            "Structure result dropped: metadata missing for {:?}/{}",
            root_category, root_sheet
        );
        error!("{}", msg);
        feedback_writer.write(SheetOperationFeedback {
            message: msg,
            is_error: true,
        });
        state.ai_active_structure_job = None;
        state.mark_structure_result_received();
        return;
    };

    let Some(column_index) = structure_path.first().copied() else {
        let msg = format!(
            "Structure result dropped: empty structure path for {:?}/{}",
            root_category, root_sheet
        );
        error!("{}", msg);
        feedback_writer.write(SheetOperationFeedback {
            message: msg,
            is_error: true,
        });
        state.ai_active_structure_job = None;
        state.mark_structure_result_received();
        return;
    };

    let Some(schema_fields) = meta.structure_fields_for_path(&structure_path) else {
        let msg = format!(
            "Structure result dropped: schema missing for {:?}/{} path {:?}",
            root_category, root_sheet, structure_path
        );
        error!("{}", msg);
        feedback_writer.write(SheetOperationFeedback {
            message: msg,
            is_error: true,
        });
        state.ai_active_structure_job = None;
        state.mark_structure_result_received();
        return;
    };

    let schema_len = schema_fields.len();
    let included = ev.included_non_structure_columns.clone();

    if *generation_id != state.ai_structure_active_generation {
        debug!(
            "Ignoring stale structure result (generation {} vs active {}) for {:?}/{} path {:?}",
            generation_id,
            state.ai_structure_active_generation,
            root_category,
            root_sheet,
            structure_path
        );
        return;
    }

    state.ai_active_structure_job = None;

    if row_partitions.len() != target_rows.len() {
        row_partitions = vec![0; target_rows.len()];
    }

    let original_counts: Vec<usize> = target_rows
        .iter()
        .map(|row_idx| {
            ev.original_row_indices
                .iter()
                .filter(|idx| **idx == *row_idx)
                .count()
        })
        .collect();

    match &ev.result {
        Ok(rows) => {
            // For structure requests with parent_groups, the response should be grouped
            // Check if we received grouped data (array of group arrays)
            // The Python processor returns this in the "data" field
            
            // Try to detect if this is a grouped response by checking if first element is itself an array
            let is_grouped_response = !rows.is_empty() 
                && rows[0].len() > 0 
                && false; // For now, assume it's always flat until we verify the structure
            
            info!(
                "Structure batch result: rows.len()={}, target_rows.len()={}, is_grouped={}",
                rows.len(),
                target_rows.len(),
                is_grouped_response
            );
            
            // For now, handle as flat array with partitioning
            // TODO: Update to handle true grouped responses once Python side is confirmed working
            let originals = ev.original_row_indices.len();
            if originals > 0 && rows.len() < originals {
                let msg = format!(
                    "Structure batch result malformed: returned {} rows but expected at least {}",
                    rows.len(),
                    originals
                );
                error!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
                state.mark_structure_result_received();
                return;
            }

            let mut cursor = 0usize;

            for (idx, parent_row_index) in target_rows.iter().enumerate() {
                let mut partition_len = row_partitions.get(idx).copied().unwrap_or(0);
                if partition_len == 0 {
                    partition_len = original_counts.get(idx).copied().unwrap_or(0);
                }
                let start = cursor.min(rows.len());
                let end = (cursor + partition_len).min(rows.len());
                let partition_rows = &rows[start..end];
                cursor = end;

                let mut original_count = original_counts.get(idx).copied().unwrap_or(0);
                info!(
                    "Processing partition {}/{} for parent {}: partition_rows.len()={}, original_count={}",
                    idx + 1,
                    target_rows.len(),
                    parent_row_index,
                    partition_rows.len(),
                    original_count
                );
                if original_count > partition_rows.len() {
                    warn!(
                        "Structure batch result for {:?}/{} path {:?} parent {} returned fewer rows ({}) than originals ({})",
                        root_category,
                        root_sheet,
                        structure_path,
                        parent_row_index,
                        partition_rows.len(),
                        original_count
                    );
                    original_count = partition_rows.len();
                }

                let new_row_context = state
                    .ai_structure_new_row_contexts
                    .get(parent_row_index)
                    .cloned();
                let parent_new_row_index = new_row_context.as_ref().map(|ctx| ctx.new_row_index);

                let mut parent_row = if let Some(ctx) = &new_row_context {
                    let mut synthetic_row = vec![String::new(); meta.columns.len()];
                    for (col_idx, value) in &ctx.non_structure_values {
                        if let Some(slot) = synthetic_row.get_mut(*col_idx) {
                            *slot = value.clone();
                        }
                    }
                    synthetic_row
                } else {
                    sheet
                        .grid
                        .get(*parent_row_index)
                        .cloned()
                        .unwrap_or_default()
                };
                if parent_row.len() < meta.columns.len() {
                    parent_row.resize(meta.columns.len(), String::new());
                }

                let cell_value = if new_row_context.is_some() {
                    String::new()
                } else {
                    parent_row.get(column_index).cloned().unwrap_or_default()
                };
                let mut original_rows = parse_structure_rows_from_cell(&cell_value, &schema_fields);
                if original_rows.is_empty() {
                    original_rows.push(vec![String::new(); schema_len]);
                }
                for row in &mut original_rows {
                    if row.len() < schema_len {
                        row.resize(schema_len, String::new());
                    }
                }
                let mut original_rows_aligned = original_rows.clone();
                if original_rows_aligned.len() < original_count {
                    original_rows_aligned.resize(original_count, vec![String::new(); schema_len]);
                }

                let mut ai_rows: Vec<Vec<String>> = Vec::new();
                let mut merged_rows: Vec<Vec<String>> = Vec::new();
                let mut differences: Vec<Vec<bool>> = Vec::new();
                let mut has_changes = false;

                for local_idx in 0..original_count {
                    let suggestion_full = &partition_rows[local_idx];
                    let suggestion = if ev.key_prefix_count > 0
                        && suggestion_full.len() >= ev.key_prefix_count
                    {
                        &suggestion_full[ev.key_prefix_count..]
                    } else {
                        suggestion_full.as_slice()
                    };
                    if suggestion.len() < included.len() {
                        warn!(
                            "Skipping malformed structure suggestion row parent={} local_idx={} suggestion_cols={} < included_cols={} (full_len={}, key_prefix_count={})",
                            parent_row_index,
                            local_idx,
                            suggestion.len(),
                            included.len(),
                            suggestion_full.len(),
                            ev.key_prefix_count
                        );
                        continue;
                    }
                    let base = original_rows_aligned
                        .get(local_idx)
                        .cloned()
                        .unwrap_or_else(|| vec![String::new(); schema_len]);
                    let mut ai_row = base.clone();
                    let mut merged_row = base.clone();
                    let mut diff_row = vec![false; schema_len];
                    for (logical_i, col_index) in included.iter().enumerate() {
                        let ai_value = suggestion.get(logical_i).cloned().unwrap_or_default();
                        let orig_value = base.get(*col_index).cloned().unwrap_or_default();
                        if ai_value != orig_value {
                            diff_row[*col_index] = true;
                            has_changes = true;
                        }
                        if let Some(slot) = ai_row.get_mut(*col_index) {
                            *slot = ai_value.clone();
                        }
                        if diff_row[*col_index] {
                            if let Some(slot) = merged_row.get_mut(*col_index) {
                                *slot = ai_value;
                            }
                        }
                    }
                    ai_rows.push(ai_row);
                    merged_rows.push(merged_row);
                    differences.push(diff_row);
                }

                for (new_row_idx, suggestion_full) in partition_rows.iter().skip(original_count).enumerate() {
                    info!(
                        "Processing AI-added row {}/{} for parent {}: suggestion_full.len()={}, key_prefix_count={}",
                        new_row_idx + 1,
                        partition_rows.len() - original_count,
                        parent_row_index,
                        suggestion_full.len(),
                        ev.key_prefix_count
                    );
                    let suggestion = if ev.key_prefix_count > 0
                        && suggestion_full.len() >= ev.key_prefix_count
                    {
                        &suggestion_full[ev.key_prefix_count..]
                    } else {
                        suggestion_full.as_slice()
                    };
                    info!(
                        "After key_prefix skip: suggestion.len()={}, included.len()={}",
                        suggestion.len(),
                        included.len()
                    );
                    if suggestion.len() < included.len() {
                        warn!(
                            "Skipping malformed new structure suggestion row parent={} suggestion_cols={} < included_cols={} (full_len={}, key_prefix_count={})",
                            parent_row_index,
                            suggestion.len(),
                            included.len(),
                            suggestion_full.len(),
                            ev.key_prefix_count
                        );
                        continue;
                    }
                    let mut ai_row = vec![String::new(); schema_len];
                    let mut merged_row = vec![String::new(); schema_len];
                    let mut diff_row = vec![false; schema_len];
                    for (logical_i, col_index) in included.iter().enumerate() {
                        let ai_value = suggestion.get(logical_i).cloned().unwrap_or_default();
                        if let Some(slot) = ai_row.get_mut(*col_index) {
                            *slot = ai_value.clone();
                        }
                        if let Some(slot) = merged_row.get_mut(*col_index) {
                            *slot = ai_value.clone();
                        }
                        diff_row[*col_index] = true;
                    }
                    has_changes = true;
                    ai_rows.push(ai_row.clone());
                    merged_rows.push(merged_row);
                    differences.push(diff_row);
                    original_rows_aligned.push(vec![String::new(); schema_len]);
                    info!(
                        "Successfully added AI-generated row {}: ai_row={:?}",
                        ai_rows.len() - 1,
                        ai_row
                    );
                }
                if original_rows_aligned.len() > merged_rows.len() {
                    original_rows_aligned.truncate(merged_rows.len());
                }



                state.ai_structure_reviews.retain(|entry| {
                    !(entry.root_category == root_category
                        && entry.root_sheet == root_sheet
                        && entry.parent_row_index == *parent_row_index
                        && entry.parent_new_row_index == parent_new_row_index
                        && entry.structure_path == structure_path)
                });

                // Extract schema headers from structure fields
                let schema_headers: Vec<String> = schema_fields
                    .iter()
                    .map(|f| f.header.clone())
                    .collect();

                state.ai_structure_reviews.push(StructureReviewEntry {
                    root_category: root_category.clone(),
                    root_sheet: root_sheet.clone(),
                    parent_row_index: *parent_row_index,
                    parent_new_row_index,
                    structure_path: structure_path.clone(),
                    has_changes,
                    accepted: !has_changes,
                    rejected: false,
                    decided: !has_changes,
                    original_rows: original_rows_aligned,
                    ai_rows,
                    merged_rows,
                    differences,
                    row_operations: Vec::new(),
                    schema_headers,
                });

                if new_row_context.is_some() {
                    state.ai_structure_new_row_contexts.remove(parent_row_index);
                }

                feedback_writer.write(SheetOperationFeedback {
                    message: format!(
                        "AI structure review ready for {:?}/{} row {} ({} suggestion rows)",
                        root_category,
                        root_sheet,
                        parent_row_index,
                        partition_rows.len()
                    ),
                    is_error: false,
                });
            }

            if let Some(raw) = &ev.raw_response {
                state.ai_raw_output_display = raw.clone();
                // Add to call log for structure results
                let status = format!("Structure completed - {} rows across {} parent(s)", rows.len(), target_rows.len());
                state.add_ai_call_log(status, Some(raw.clone()), None, false);
            }

            info!(
                "Processed structure result for {:?}/{} path {:?}: {} suggestion rows across {} parents",
                root_category, root_sheet, structure_path, rows.len(), target_rows.len()
            );
        }
        Err(err) => {
            for parent_row_index in target_rows.iter() {
                let new_row_context = state
                    .ai_structure_new_row_contexts
                    .get(parent_row_index)
                    .cloned();
                let parent_new_row_index = new_row_context.as_ref().map(|ctx| ctx.new_row_index);
                let mut parent_row = if let Some(ctx) = &new_row_context {
                    let mut synthetic_row = vec![String::new(); meta.columns.len()];
                    for (col_idx, value) in &ctx.non_structure_values {
                        if let Some(slot) = synthetic_row.get_mut(*col_idx) {
                            *slot = value.clone();
                        }
                    }
                    synthetic_row
                } else {
                    sheet
                        .grid
                        .get(*parent_row_index)
                        .cloned()
                        .unwrap_or_default()
                };
                if parent_row.len() < meta.columns.len() {
                    parent_row.resize(meta.columns.len(), String::new());
                }
                let cell_value = if new_row_context.is_some() {
                    String::new()
                } else {
                    parent_row.get(column_index).cloned().unwrap_or_default()
                };
                let mut original_rows = parse_structure_rows_from_cell(&cell_value, &schema_fields);
                if original_rows.is_empty() {
                    original_rows.push(vec![String::new(); schema_len]);
                }
                for row in &mut original_rows {
                    if row.len() < schema_len {
                        row.resize(schema_len, String::new());
                    }
                }

                state.ai_structure_reviews.retain(|entry| {
                    !(entry.root_category == root_category
                        && entry.root_sheet == root_sheet
                        && entry.parent_row_index == *parent_row_index
                        && entry.parent_new_row_index == parent_new_row_index
                        && entry.structure_path == structure_path)
                });
                state.ai_structure_reviews.push(StructureReviewEntry {
                    root_category: root_category.clone(),
                    root_sheet: root_sheet.clone(),
                    parent_row_index: *parent_row_index,
                    parent_new_row_index,
                    structure_path: structure_path.clone(),
                    has_changes: false,
                    accepted: false,
                    rejected: true,
                    decided: true,
                    original_rows: original_rows,
                    ai_rows: Vec::new(),
                    merged_rows: Vec::new(),
                    differences: Vec::new(),
                    row_operations: Vec::new(),
                    schema_headers: Vec::new(),
                });
                if new_row_context.is_some() {
                    state.ai_structure_new_row_contexts.remove(parent_row_index);
                }
            }
            if let Some(raw) = &ev.raw_response {
                state.ai_raw_output_display = format!(
                    "Structure Batch Error: {}\n--- Raw Model Output ---\n{}",
                    err, raw
                );
                // Add to call log
                state.add_ai_call_log(
                    format!("Structure Error: {}", err),
                    Some(raw.clone()),
                    None,
                    true,
                );
            } else {
                state.ai_raw_output_display =
                    format!("Structure Batch Error: {} (no raw output returned)", err);
                // Add to call log
                state.add_ai_call_log(
                    format!("Structure Error: {}", err),
                    None,
                    None,
                    true,
                );
            }
            feedback_writer.write(SheetOperationFeedback {
                message: format!(
                    "AI structure review error for {:?}/{} ({} parents): {}",
                    root_category,
                    root_sheet,
                    target_rows.len(),
                    err
                ),
                is_error: true,
            });
            warn!(
                "Structure batch result error for {:?}/{} path {:?}: {}",
                root_category, root_sheet, structure_path, err
            );
        }
    }

    state.mark_structure_result_received();
}


