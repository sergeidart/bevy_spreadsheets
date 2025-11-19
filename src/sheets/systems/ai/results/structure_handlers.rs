// src/sheets/systems/ai/results/structure_handlers.rs
// Structure batch result handlers

use bevy::prelude::*;

use crate::sheets::events::{AiBatchTaskResult, SheetOperationFeedback};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

use crate::sheets::systems::ai::structure_results::{
    handle_structure_error, process_structure_partition,
};

/// Handle structure batch results with validation and partition processing
pub fn handle_structure_batch_result(
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

    match &ev.result {
        Ok(rows) => {
            info!(
                "Structure batch result: rows.len()={}, target_rows.len()={}",
                rows.len(),
                target_rows.len()
            );
            
            // Group AI rows by parent key (first column)
            // Map: parent_key -> Vec<Row>
            let mut rows_by_parent: std::collections::HashMap<String, Vec<Vec<String>>> = std::collections::HashMap::new();
            let mut unkeyed_rows: Vec<Vec<String>> = Vec::new();
            
            for row in rows {
                if let Some(parent_key) = row.first() {
                    let content = if row.len() > 1 {
                        row[1..].to_vec()
                    } else {
                        vec![String::new(); schema_len]
                    };
                    rows_by_parent.entry(parent_key.clone())
                        .or_default()
                        .push(content);
                } else {
                    unkeyed_rows.push(if row.len() > 1 { row[1..].to_vec() } else { vec![String::new(); schema_len] });
                }
            }

            info!("Structure Handler (HashMap): Parsed {} keyed groups and {} unkeyed rows", rows_by_parent.len(), unkeyed_rows.len());

            let _originals = ev.original_row_indices.len();
            // Note: We can't easily validate total row count against originals here because 
            // rows are now partitioned by parent key, and some parents might be missing or added.

            // Calculate original counts per parent
            let original_counts: Vec<usize> = original_row_partitions.clone();

            // Process each target row
            for (idx, &parent_row_index) in target_rows.iter().enumerate() {
                // Determine parent key for this target row to look up in response
                let parent_key = if let Some(context) = state.ai_structure_new_row_contexts.get(&parent_row_index) {
                    // New row: extract key from full_ai_row (Phase 1 data)
                    if let Some(full_row) = &context.full_ai_row {
                        // Calculate dynamic prefix (same logic as task_executor)
                        let dynamic_prefix = if let Some(new_row_review) = state.ai_new_row_reviews.get(context.new_row_index) {
                            full_row.len().saturating_sub(new_row_review.ai.len())
                        } else {
                            0
                        };
                        full_row.get(dynamic_prefix).cloned().unwrap_or_default()
                    } else if let Some(new_row_review) = state.ai_new_row_reviews.get(context.new_row_index) {
                        new_row_review.ai.first().cloned().unwrap_or_default()
                    } else {
                        String::new()
                    }
                } else {
                    // Existing row: use first data column (typically Name at index 1)
                    // This matches task_executor logic
                    sheet.grid.get(parent_row_index)
                        .and_then(|r| r.get(1))
                        .cloned()
                        .unwrap_or_default()
                };

                // Get rows for this parent from the map
                // If not found, fallback to empty list (no AI suggestions for this parent)
                let mut partition_rows = rows_by_parent.remove(&parent_key).unwrap_or_default();

                // Fallback: if this is the ONLY target row, and we have unkeyed rows, assign them
                if target_rows.len() == 1 && !unkeyed_rows.is_empty() {
                     info!("Assigning {} unkeyed rows to single parent '{}'", unkeyed_rows.len(), parent_key);
                     partition_rows.extend(unkeyed_rows.drain(..));
                }

                // Fallback: if this is the ONLY target row, and we found nothing, but there is exactly one other group in the map
                // This handles cases where AI returns a slightly different key (e.g. trimmed, or case difference, or just wrong)
                if target_rows.len() == 1 && partition_rows.is_empty() && rows_by_parent.len() == 1 {
                    if let Some((wrong_key, rows)) = rows_by_parent.drain().next() {
                        warn!("Single parent fallback: Parent '{}' taking {} rows keyed with '{}'", parent_key, rows.len(), wrong_key);
                        partition_rows = rows;
                    }
                }

                let original_count = original_counts.get(idx).copied().unwrap_or(0);
                
                info!(
                    "Processing partition {}/{} for parent {} (key='{}'): partition_rows.len()={}, original_count={}",
                    idx + 1,
                    target_rows.len(),
                    parent_row_index,
                    parent_key,
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
                }

                process_structure_partition(
                    parent_row_index,
                    &partition_rows,
                    original_count,
                    &root_category,
                    &root_sheet,
                    &structure_path,
                    column_index,
                    &schema_fields,
                    schema_len,
                    &included,
                    0, // key_prefix_count is 0 because we already stripped the parent key
                    sheet,
                    state,
                    feedback_writer,
                    registry,
                );
            }
            
            // Log any remaining rows (orphaned or new parents)
            if !rows_by_parent.is_empty() {
                info!(
                    "Structure batch result contained {} orphaned parent groups: {:?}",
                    rows_by_parent.len(),
                    rows_by_parent.keys().collect::<Vec<_>>()
                );
            }
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
                registry,
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
