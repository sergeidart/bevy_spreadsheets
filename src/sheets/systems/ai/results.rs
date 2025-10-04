// src/sheets/systems/ai/results.rs
// Main AI result handlers - coordinates batch processing and delegates to helpers

use bevy::prelude::*;
use std::collections::HashMap;

use crate::sheets::events::{
    AiBatchResultKind, AiBatchTaskResult, SheetOperationFeedback,
};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{
    EditorWindowState, NewRowReview, ReviewChoice, RowReview,
};

use super::row_helpers::{
    create_row_snapshots, extract_ai_snapshot_from_new_row,
    extract_original_snapshot_for_merge, generate_review_choices, normalize_cell_value,
    skip_key_prefix,
};
use super::structure_results::{handle_structure_error, process_structure_partition};
use super::phase2_helpers;

// Re-export legacy single-row handler for backwards compatibility
pub use super::legacy::handle_ai_task_results;

/// Handle batch (root + structure) AI results
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
                phase2_helpers::handle_deep_review_result_phase2(ev, &duplicate_indices, established_row_count, &mut state, &registry, &mut feedback_writer);
            } else {
                error!("Phase 2 result expected but no Phase 1 data found!");
                state.ai_expecting_phase2_result = false;
            }
        } else {
            match &ev.kind {
                AiBatchResultKind::Root { structure_context: Some(context) } => {
                    handle_structure_batch_result(ev, context, &mut state, &registry, &mut feedback_writer);
                }
                AiBatchResultKind::Root { structure_context: None } => {
                    handle_root_batch_result_phase1(ev, &mut state, &registry, &mut feedback_writer, &mut commands, &runtime, &session_api_key);
                }
                AiBatchResultKind::DeepReview { .. } => {
                    error!("Unexpected DeepReview result kind without flag set!");
                }
            }
        }
        break;
    }
}

/// Handle root batch results - Phase 1: Initial discovery call
/// Detects duplicates and triggers Phase 2 deep review automatically
fn handle_root_batch_result_phase1(
    ev: &AiBatchTaskResult,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
    commands: &mut Commands,
    runtime: &bevy_tokio_tasks::TokioTasksRuntime,
    session_api_key: &crate::SessionApiKey,
) {
    match &ev.result {
        Ok(rows) => {
            let originals = ev.original_row_indices.len();
            info!(
                "PHASE 1: Received {} rows ({} originals, {} new)",
                rows.len(),
                originals,
                rows.len().saturating_sub(originals)
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
                return;
            }

            if let Some(raw) = &ev.raw_response {
                state.ai_raw_output_display = raw.clone();
                let status = format!("Phase 1 complete - {} row(s) received, analyzing...", rows.len());
                state.add_ai_call_log(status, Some(raw.clone()), None, false);
            }

            // Detect duplicates in new rows
            let (_orig_slice, extra_slice) = if originals == 0 {
                (&[][..], &rows[..])
            } else {
                rows.split_at(originals)
            };

            let duplicate_indices = phase2_helpers::detect_duplicate_indices(
                extra_slice,
                &ev.included_non_structure_columns,
                ev.key_prefix_count,
                state,
                registry,
            );

            info!(
                "PHASE 1: Detected {} duplicates out of {} new rows",
                duplicate_indices.len(),
                extra_slice.len()
            );

            // Store Phase 1 intermediate data
            let (cat_ctx, sheet_ctx) = state.current_sheet_context();
            let sheet_name = sheet_ctx.unwrap_or_default();
            
            state.ai_phase1_intermediate = Some(crate::ui::elements::editor::state::Phase1IntermediateData {
                all_ai_rows: rows.clone(),
                duplicate_indices: duplicate_indices.clone(),
                original_count: originals,
                included_columns: ev.included_non_structure_columns.clone(),
                category: cat_ctx.clone(),
                sheet_name: sheet_name.clone(),
                key_prefix_count: ev.key_prefix_count,
                original_row_indices: ev.original_row_indices.clone(),
            });

            // OPTIMIZATION: Skip Phase 2 if only one column is being processed
            // With single column, there's no merge complexity, so use Phase 1 results directly
            if ev.included_non_structure_columns.len() <= 1 {
                info!(
                    "SINGLE-COLUMN OPTIMIZATION: Skipping Phase 2 (only {} column(s)), using Phase 1 results directly",
                    ev.included_non_structure_columns.len()
                );
                
                // Process Phase 1 results directly as final results
                let established_row_count = originals + duplicate_indices.len();
                phase2_helpers::handle_deep_review_result_phase2(
                    ev,
                    &duplicate_indices,
                    established_row_count,
                    state,
                    registry,
                    feedback_writer,
                );
            } else {
                // Trigger Phase 2: Deep review call
                phase2_helpers::trigger_phase2_deep_review(
                    state,
                    registry,
                    commands,
                    runtime,
                    session_api_key,
                    rows,
                    &duplicate_indices,
                    originals,
                    &ev.included_non_structure_columns,
                    ev.key_prefix_count,
                );

                // Do NOT enqueue structure jobs yet - wait for Phase 2 to complete
                // Structure jobs will be enqueued in handle_deep_review_result_phase2
            }
        }
        Err(err) => {
            handle_root_batch_error(state, ev, err, feedback_writer);
        }
    }
}

/// Setup AI context prefixes for key columns
pub(super) fn setup_context_prefixes(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ev: &AiBatchTaskResult,
) {
    state.ai_context_only_prefix_count = ev.key_prefix_count;
    state.ai_context_prefix_by_row.clear();

    if state.virtual_structure_stack.is_empty() {
        return;
    }

    let mut key_headers: Vec<String> = Vec::new();
    let mut ancestors_with_keys: Vec<(Option<String>, String, usize, usize)> = Vec::new();

    for vctx in &state.virtual_structure_stack {
        let anc_cat = vctx.parent.parent_category.clone();
        let anc_sheet = vctx.parent.parent_sheet.clone();
        let anc_row_idx = vctx.parent.parent_row;

        if let Some(sheet) = registry.get_sheet(&anc_cat, &anc_sheet) {
            if let Some(meta) = &sheet.metadata {
                if let Some(key_col_index) = meta
                    .columns
                    .iter()
                    .find_map(|c| {
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
                    ancestors_with_keys.push((anc_cat, anc_sheet, anc_row_idx, key_col_index));
                }
            }
        }
    }

    if !ancestors_with_keys.is_empty() && !key_headers.is_empty() {
        for &row_index in ev.original_row_indices.iter() {
            let mut pairs: Vec<(String, String)> = Vec::with_capacity(key_headers.len());
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

/// Process original (existing) rows from batch result
fn process_original_rows(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ev: &AiBatchTaskResult,
    orig_slice: &[Vec<String>],
) {
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    state.ai_structure_reviews.clear();

    let included = &ev.included_non_structure_columns;
    let (cat_ctx, sheet_ctx) = state.current_sheet_context();

    for (i, &row_index) in ev.original_row_indices.iter().enumerate() {
        let suggestion_full = &orig_slice[i];
        let suggestion = skip_key_prefix(suggestion_full, ev.key_prefix_count);

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

        let Some(sheet_name) = &sheet_ctx else {
            continue;
        };

        let (original_snapshot, ai_snapshot) =
            create_row_snapshots(registry, &cat_ctx, sheet_name, row_index, suggestion, included);

        let choices = generate_review_choices(&original_snapshot, &ai_snapshot);

        state.ai_row_reviews.push(RowReview {
            row_index,
            original: original_snapshot,
            ai: ai_snapshot,
            choices,
            non_structure_columns: included.clone(),
        });

        // CACHE POPULATION: Store full grid row for rendering original previews
        // Includes raw structure JSON for on-demand parsing or lookup in StructureReviewEntry
        if let Some(sheet_name) = &sheet_ctx {
            if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                if let Some(full_row) = sheet_ref.grid.get(row_index) {
                    state.ai_original_row_snapshot_cache.insert(
                        (Some(row_index), None),
                        full_row.clone(),
                    );
                }
            }
        }
    }
}

/// Process new (AI-added) rows from batch result
fn process_new_rows(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ev: &AiBatchTaskResult,
    extra_slice: &[Vec<String>],
) {
    let included = &ev.included_non_structure_columns;
    let (cat_ctx, sheet_ctx) = state.current_sheet_context();

    // Build duplicate detection map
    let mut first_col_value_to_row: HashMap<String, usize> = HashMap::new();
    if let Some(first_col_actual) = included.first() {
        if let Some(sheet_name) = &sheet_ctx {
            if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
                    if let Some(val) = row.get(*first_col_actual) {
                        let norm = normalize_cell_value(val);
                        if !norm.is_empty() {
                            first_col_value_to_row.entry(norm).or_insert(row_idx);
                        }
                    }
                }
            }
        }
    }

    for new_row_full in extra_slice.iter() {
        let new_row = skip_key_prefix(new_row_full, ev.key_prefix_count);

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

        let ai_snapshot = extract_ai_snapshot_from_new_row(new_row, included);

        let (duplicate_match_row, choices, original_for_merge, merge_selected) =
            check_for_duplicate(
                &ai_snapshot,
                &first_col_value_to_row,
                included,
                &cat_ctx,
                &sheet_ctx,
                registry,
            );

        let new_row_idx = state.ai_new_row_reviews.len();
        state.ai_new_row_reviews.push(NewRowReview {
            ai: ai_snapshot.clone(),
            non_structure_columns: included.clone(),
            duplicate_match_row,
            choices,
            merge_selected,
            merge_decided: false,
            original_for_merge: original_for_merge.clone(),
        });

        // CACHE POPULATION: Store snapshot for new/duplicate rows
        // - Duplicates: Use matched existing row (includes structure JSON)
        // - New rows: Empty snapshot (no original content)
        // This unifies preview rendering across all row types (existing/new/duplicate)
        if let Some(matched_idx) = duplicate_match_row {
            if let Some(sheet_name) = &sheet_ctx {
                if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                    if let Some(full_row) = sheet_ref.grid.get(matched_idx) {
                        state.ai_original_row_snapshot_cache.insert(
                            (None, Some(new_row_idx)),
                            full_row.clone(),
                        );
                    }
                }
            }
        } else {
            // Truly new rows (no duplicate): empty snapshot matching column count
            if let Some(sheet_name) = &sheet_ctx {
                if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                    if let Some(meta) = &sheet_ref.metadata {
                        let empty_row = vec![String::new(); meta.columns.len()];
                        state.ai_original_row_snapshot_cache.insert(
                            (None, Some(new_row_idx)),
                            empty_row,
                        );
                    }
                }
            }
        }
    }
}

/// Check if a new row is a duplicate of an existing row
pub(super) fn check_for_duplicate(
    ai_snapshot: &[String],
    first_col_value_to_row: &HashMap<String, usize>,
    included: &[usize],
    cat_ctx: &Option<String>,
    sheet_ctx: &Option<String>,
    registry: &SheetRegistry,
) -> (
    Option<usize>,
    Option<Vec<ReviewChoice>>,
    Option<Vec<String>>,
    bool,
) {
    let Some(first_val) = ai_snapshot.get(0) else {
        return (None, None, None, false);
    };

    let normalized_first = normalize_cell_value(first_val);
    let Some(&matched_row_index) = first_col_value_to_row.get(&normalized_first) else {
        return (None, None, None, false);
    };

    let Some(sheet_name) = sheet_ctx else {
        return (Some(matched_row_index), None, None, false);
    };

    let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_name) else {
        return (Some(matched_row_index), None, None, false);
    };

    let Some(existing_row) = sheet_ref.grid.get(matched_row_index) else {
        return (Some(matched_row_index), None, None, false);
    };

    let orig_vec = extract_original_snapshot_for_merge(existing_row, included);
    let choices = generate_review_choices(&orig_vec, ai_snapshot);

    (Some(matched_row_index), Some(choices), Some(orig_vec), true)
}

/// Handle root batch errors
pub(super) fn handle_root_batch_error(
    state: &mut EditorWindowState,
    ev: &AiBatchTaskResult,
    err: &str,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
) {
    if let Some(raw) = &ev.raw_response {
        state.ai_raw_output_display =
            format!("Batch Error: {}\n--- Raw Model Output ---\n{}", err, raw);
        state.add_ai_call_log(format!("Error: {}", err), Some(raw.clone()), None, true);
    } else {
        state.ai_raw_output_display =
            format!("Batch Error: {} (no raw output returned)", err);
        state.add_ai_call_log(format!("Error: {}", err), None, None, true);
    }

    // Don't auto-open log panel - user will open manually if needed
    feedback_writer.write(SheetOperationFeedback {
        message: format!("AI batch error: {}", err),
        is_error: true,
    });
    state.ai_mode = crate::ui::elements::editor::state::AiModeState::Preparing;
}

/// Handle structure batch results with validation and partition processing
fn handle_structure_batch_result(
    ev: &AiBatchTaskResult,
    context: &crate::sheets::events::StructureProcessingContext,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
) {
    let root_category = context.root_category.clone();
    let root_sheet = context.root_sheet.clone();
    let structure_path = context.structure_path.clone();
    let target_rows = context.target_rows.clone();
    let original_row_partitions = context.original_row_partitions.clone();
    let mut row_partitions = context.row_partitions.clone();
    let generation_id = context.generation_id;

    // Validate sheet exists
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
        // PARALLEL MODE: No need to clear ai_active_structure_job
        state.mark_structure_result_received();
        return;
    };

    // Validate metadata exists
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
        // PARALLEL MODE: No need to clear ai_active_structure_job
        state.mark_structure_result_received();
        return;
    };

    // Validate structure path
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
        // PARALLEL MODE: No need to clear ai_active_structure_job
        state.mark_structure_result_received();
        return;
    };

    // Validate schema exists
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
        // PARALLEL MODE: No need to clear ai_active_structure_job
        state.mark_structure_result_received();
        return;
    };

    // Check generation (stale results)
    if generation_id != state.ai_structure_active_generation {
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

    // PARALLEL MODE: No need to clear ai_active_structure_job
    // state.ai_active_structure_job = None;

    let schema_len = schema_fields.len();
    let included = ev.included_non_structure_columns.clone();

    // Normalize partitions
    if row_partitions.len() != target_rows.len() {
        row_partitions = vec![0; target_rows.len()];
    }

    match &ev.result {
        Ok(rows) => {
            info!(
                "Structure batch result: rows.len()={}, target_rows.len()={}",
                rows.len(),
                target_rows.len()
            );

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

            // Calculate original counts per parent
            // For structures, original_row_partitions contains the actual count of structure rows per parent
            // before AI added any rows. This represents how many original structure rows each parent has.
            // row_partitions may be larger if AI added rows.
            let original_counts: Vec<usize> = original_row_partitions.clone();

            // Process each partition
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

                process_structure_partition(
                    *parent_row_index,
                    partition_rows,
                    original_count,
                    &root_category,
                    &root_sheet,
                    &structure_path,
                    column_index,
                    &schema_fields,
                    schema_len,
                    &included,
                    ev.key_prefix_count,
                    sheet,
                    state,
                    feedback_writer,
                );
            }

            if let Some(raw) = &ev.raw_response {
                state.ai_raw_output_display = raw.clone();
                let status = format!(
                    "Structure completed - {} rows across {} parent(s)",
                    rows.len(),
                    target_rows.len()
                );
                state.add_ai_call_log(status, Some(raw.clone()), None, false);
            }

            info!(
                "Processed structure result for {:?}/{} path {:?}: {} suggestion rows across {} parents",
                root_category, root_sheet, structure_path, rows.len(), target_rows.len()
            );
        }
        Err(err) => {
            handle_structure_error(
                &target_rows,
                &root_category,
                &root_sheet,
                &structure_path,
                column_index,
                &schema_fields,
                schema_len,
                sheet,
                state,
            );

            if let Some(raw) = &ev.raw_response {
                state.ai_raw_output_display = format!(
                    "Structure Batch Error: {}\n--- Raw Model Output ---\n{}",
                    err, raw
                );
                state.add_ai_call_log(
                    format!("Structure Error: {}", err),
                    Some(raw.clone()),
                    None,
                    true,
                );
            } else {
                state.ai_raw_output_display =
                    format!("Structure Batch Error: {} (no raw output returned)", err);
                state.add_ai_call_log(format!("Structure Error: {}", err), None, None, true);
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
