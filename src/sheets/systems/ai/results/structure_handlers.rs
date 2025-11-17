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
            
            // Strip first column (parent key) from AI response
            // Structure calls send parent key as first visible column, just like regular AI calls
            let stripped_rows: Vec<Vec<String>> = rows
                .iter()
                .map(|row| {
                    if row.len() > 1 {
                        row[1..].to_vec()  // Skip first column (parent key)
                    } else {
                        vec![String::new(); schema_len]
                    }
                })
                .collect();

            let originals = ev.original_row_indices.len();
            if originals > 0 && stripped_rows.len() < originals {
                let msg = format!(
                    "Structure batch result malformed: returned {} rows but expected at least {}",
                    stripped_rows.len(),
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
                let start = cursor.min(stripped_rows.len());
                let end = (cursor + partition_len).min(stripped_rows.len());
                let partition_rows = &stripped_rows[start..end];
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
                    registry,
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
