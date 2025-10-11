// Handlers for accepting/cancelling AI row suggestions (moved out of monolithic file)
use crate::sheets::events::{AddSheetRowRequest, UpdateCellEvent};
use crate::ui::elements::editor::state::{
    EditorWindowState, NewRowReview, ReviewChoice, StructureReviewEntry,
};
use bevy::prelude::{info, EventWriter};

fn remove_new_row_context(state: &mut EditorWindowState, new_index: usize) {
    state.ai_structure_new_row_contexts.remove(&new_index);
}

pub fn take_structure_entries_for_existing(
    state: &mut EditorWindowState,
    row_index: usize,
) -> Vec<StructureReviewEntry> {
    let mut extracted = Vec::new();
    let mut i = 0usize;
    while i < state.ai_structure_reviews.len() {
        if state.ai_structure_reviews[i].parent_new_row_index.is_none()
            && state.ai_structure_reviews[i].parent_row_index == row_index
        {
            extracted.push(state.ai_structure_reviews.remove(i));
        } else {
            i += 1;
        }
    }
    extracted
}

pub fn take_structure_entries_for_new(
    state: &mut EditorWindowState,
    new_index: usize,
) -> Vec<StructureReviewEntry> {
    let mut extracted = Vec::new();
    let mut i = 0usize;
    while i < state.ai_structure_reviews.len() {
        if state.ai_structure_reviews[i]
            .parent_new_row_index
            .map(|idx| idx == new_index)
            .unwrap_or(false)
        {
            extracted.push(state.ai_structure_reviews.remove(i));
        } else {
            i += 1;
        }
    }
    remove_new_row_context(state, new_index);
    extracted
}

pub fn queue_new_row_add(
    review: &NewRowReview,
    _selected_category: &Option<String>,
    _active_sheet_name: &str,
    state: &mut EditorWindowState,
    structure_entries: Vec<StructureReviewEntry>,
) {
    let mut init_vals: Vec<(usize, String)> = Vec::new();

    // Add non-structure column values
    for (pos, actual_col) in review.non_structure_columns.iter().enumerate() {
        if let Some(val) = review.ai.get(pos).cloned() {
            init_vals.push((*actual_col, val));
        }
    }

    // Add structure column values (serialized as JSON)
    // Include all structures - if decided and not rejected, use merged_rows; otherwise use AI rows
    for entry in structure_entries {
        info!("Structure entry: decided={}, rejected={}, accepted={}, has_changes={}, merged_rows.len()={}, ai_rows.len()={}", 
              entry.decided, entry.rejected, entry.accepted, entry.has_changes, entry.merged_rows.len(), entry.ai_rows.len());

        // Skip rejected structures
        if entry.rejected {
            continue;
        }

        // Use merged_rows if decided (contains user's final choices), otherwise use ai_rows
        let rows_to_serialize = if entry.decided {
            &entry.merged_rows
        } else {
            &entry.ai_rows
        };

        // Convert rows to array of objects using schema_headers
        info!(
            "Serializing structure for new row: {} rows, schema_headers={:?}",
            rows_to_serialize.len(),
            entry.schema_headers
        );
        let array_of_objects: Vec<serde_json::Map<String, serde_json::Value>> = rows_to_serialize
            .iter()
            .enumerate()
            .map(|(row_idx, row)| {
                info!("  Row {}: len={}, data={:?}", row_idx, row.len(), row);
                let mut obj = serde_json::Map::new();
                for (i, value) in row.iter().enumerate() {
                    let field_name = entry
                        .schema_headers
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("field_{}", i));
                    obj.insert(field_name, serde_json::Value::String(value.clone()));
                }
                obj
            })
            .collect();

        let json_value = serde_json::json!(array_of_objects);
        let json_string = serde_json::to_string(&json_value).unwrap_or_else(|_| "[]".to_string());

        let col_index = *entry.structure_path.first().unwrap_or(&0);
        info!(
            "Adding structure to new row: col={}, json={}",
            col_index, json_string
        );
        init_vals.push((col_index, json_string));
    }

    // Queue the row addition via throttling mechanism to ensure proper ordering
    // (one row per frame prevents race conditions where rows get inserted incorrectly)
    state.ai_throttled_apply_queue.push_back(
        crate::ui::elements::editor::state::ThrottledAiAction::AddRow {
            initial_values: init_vals,
        },
    );
}

pub fn finalize_if_empty(state: &mut EditorWindowState) {
    // Only exit if there are no row reviews AND no undecided structures
    let has_undecided_structures = state
        .ai_structure_reviews
        .iter()
        .any(|entry| entry.is_undecided());
    if state.ai_row_reviews.is_empty()
        && state.ai_new_row_reviews.is_empty()
        && !has_undecided_structures
    {
        state.ai_batch_review_active = false;
    }
}

pub fn process_existing_accept(
    indices: &[usize],
    state: &mut EditorWindowState,
    selected_category: &Option<String>,
    active_sheet_name: &str,
    cell_update_writer: &mut EventWriter<UpdateCellEvent>,
) {
    let mut sorted = indices.to_vec();
    if sorted.is_empty() {
        return;
    }
    sorted.sort_unstable();
    sorted.dedup();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    for idx in sorted {
        if idx < state.ai_row_reviews.len() {
            let rr = state.ai_row_reviews.remove(idx);
            for (pos, actual_col) in rr.non_structure_columns.iter().enumerate() {
                let choice = rr
                    .choices
                    .get(pos)
                    .copied()
                    .unwrap_or(ReviewChoice::Original);
                // Skip parent_key column (non-editable) to avoid overriding it during merge
                if *actual_col == 1 {
                    continue;
                }
                if matches!(choice, ReviewChoice::AI) {
                    if let Some(ai_val) = rr.ai.get(pos).cloned() {
                        cell_update_writer.write(UpdateCellEvent {
                            category: selected_category.clone(),
                            sheet_name: active_sheet_name.to_string(),
                            row_index: rr.row_index,
                            col_index: *actual_col,
                            new_value: ai_val,
                        });
                    }
                }
            }

            // Write structure cells
            // Include all non-rejected structures - use merged_rows if decided, ai_rows otherwise
            let structure_entries = take_structure_entries_for_existing(state, rr.row_index);
            info!(
                "process_existing_accept: row_index={}, found {} structure entries",
                rr.row_index,
                structure_entries.len()
            );
            for entry in structure_entries {
                info!("Structure entry for row {}: decided={}, rejected={}, accepted={}, merged_rows.len()={}, ai_rows.len()={}", 
                      rr.row_index, entry.decided, entry.rejected, entry.accepted, entry.merged_rows.len(), entry.ai_rows.len());

                // Skip rejected structures
                if entry.rejected {
                    info!("Skipping rejected structure");
                    state.ai_structure_reviews.push(entry);
                    continue;
                }

                // Use merged_rows if decided (contains user's final choices), otherwise use ai_rows
                let rows_to_serialize = if entry.decided {
                    info!("Using merged_rows (decided=true)");
                    &entry.merged_rows
                } else {
                    info!("Using ai_rows (decided=false)");
                    &entry.ai_rows
                };

                // Convert rows to array of objects using schema_headers
                info!(
                    "Serializing structure: {} rows, schema_headers={:?}",
                    rows_to_serialize.len(),
                    entry.schema_headers
                );
                let array_of_objects: Vec<serde_json::Map<String, serde_json::Value>> =
                    rows_to_serialize
                        .iter()
                        .enumerate()
                        .map(|(row_idx, row)| {
                            info!("  Row {}: len={}, data={:?}", row_idx, row.len(), row);
                            let mut obj = serde_json::Map::new();
                            for (i, value) in row.iter().enumerate() {
                                let field_name = entry
                                    .schema_headers
                                    .get(i)
                                    .cloned()
                                    .unwrap_or_else(|| format!("field_{}", i));
                                obj.insert(field_name, serde_json::Value::String(value.clone()));
                            }
                            obj
                        })
                        .collect();

                let json_value = serde_json::json!(array_of_objects);
                let json_string =
                    serde_json::to_string(&json_value).unwrap_or_else(|_| "[]".to_string());

                let col_index = *entry.structure_path.first().unwrap_or(&0);
                info!(
                    "Writing structure to cell: row={}, col={}, json={}",
                    entry.parent_row_index, col_index, json_string
                );
                cell_update_writer.write(UpdateCellEvent {
                    category: selected_category.clone(),
                    sheet_name: active_sheet_name.to_string(),
                    row_index: entry.parent_row_index,
                    col_index,
                    new_value: json_string,
                });
            }

            state.ai_selected_rows.remove(&rr.row_index);
        }
    }
}

pub fn process_existing_decline(indices: &[usize], state: &mut EditorWindowState) {
    let mut sorted = indices.to_vec();
    if sorted.is_empty() {
        return;
    }
    sorted.sort_unstable();
    sorted.dedup();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    for idx in sorted {
        if idx < state.ai_row_reviews.len() {
            let rr = state.ai_row_reviews.remove(idx);
            take_structure_entries_for_existing(state, rr.row_index);
            state.ai_selected_rows.remove(&rr.row_index);
        }
    }
}

pub fn process_new_accept(
    indices: &[usize],
    state: &mut EditorWindowState,
    selected_category: &Option<String>,
    active_sheet_name: &str,
    cell_update_writer: &mut EventWriter<UpdateCellEvent>,
    _add_row_writer: &mut EventWriter<AddSheetRowRequest>, // Kept for compatibility, but using throttling queue instead
) {
    let mut sorted = indices.to_vec();
    if sorted.is_empty() {
        return;
    }
    sorted.sort_unstable();
    sorted.dedup();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    for idx in sorted {
        if idx < state.ai_new_row_reviews.len() {
            let nr = state.ai_new_row_reviews.remove(idx);

            // Extract structure entries for this new row
            let structure_entries = take_structure_entries_for_new(state, idx);
            info!(
                "Processing new row accept: found {} structure entries for new row idx {}",
                structure_entries.len(),
                idx
            );

            if let Some(match_row) = nr.duplicate_match_row {
                if nr.merge_selected {
                    // Merging into existing row - update cells directly
                    if let Some(choices) = nr.choices.as_ref() {
                        for (pos, actual_col) in nr.non_structure_columns.iter().enumerate() {
                            if let Some(choice) = choices.get(pos) {
                                    // Skip parent_key column (non-editable)
                                    if *actual_col == 1 {
                                        continue;
                                    }
                                    if matches!(choice, ReviewChoice::AI) {
                                        if let Some(val) = nr.ai.get(pos).cloned() {
                                            cell_update_writer.write(UpdateCellEvent {
                                                category: selected_category.clone(),
                                                sheet_name: active_sheet_name.to_string(),
                                                row_index: match_row,
                                                col_index: *actual_col,
                                                new_value: val,
                                            });
                                        }
                                    }
                            }
                        }
                    }

                    // Write structure cells for merged row
                    // Include all non-rejected structures - use merged_rows if decided, ai_rows otherwise
                    for entry in structure_entries {
                        // Skip rejected structures
                        if entry.rejected {
                            state.ai_structure_reviews.push(entry);
                            continue;
                        }

                        // Use merged_rows if decided (contains user's final choices), otherwise use ai_rows
                        let rows_to_serialize = if entry.decided {
                            &entry.merged_rows
                        } else {
                            &entry.ai_rows
                        };

                        // Convert rows to array of objects using schema_headers
                        let array_of_objects: Vec<serde_json::Map<String, serde_json::Value>> =
                            rows_to_serialize
                                .iter()
                                .map(|row| {
                                    let mut obj = serde_json::Map::new();
                                    for (i, value) in row.iter().enumerate() {
                                        let field_name = entry
                                            .schema_headers
                                            .get(i)
                                            .cloned()
                                            .unwrap_or_else(|| format!("field_{}", i));
                                        obj.insert(
                                            field_name,
                                            serde_json::Value::String(value.clone()),
                                        );
                                    }
                                    obj
                                })
                                .collect();

                        let json_value = serde_json::json!(array_of_objects);
                        let json_string =
                            serde_json::to_string(&json_value).unwrap_or_else(|_| "[]".to_string());

                        let col_index = *entry.structure_path.first().unwrap_or(&0);
                        cell_update_writer.write(UpdateCellEvent {
                            category: selected_category.clone(),
                            sheet_name: active_sheet_name.to_string(),
                            row_index: match_row,
                            col_index,
                            new_value: json_string,
                        });
                    }
                } else {
                    // Creating separate new row with structure data (throttled)
                    queue_new_row_add(
                        &nr,
                        selected_category,
                        active_sheet_name,
                        state,
                        structure_entries,
                    );
                }
            } else {
                // Creating new row with structure data (throttled)
                queue_new_row_add(
                    &nr,
                    selected_category,
                    active_sheet_name,
                    state,
                    structure_entries,
                );
            }

            // CRITICAL: Update all structure entries with higher parent_new_row_index
            // to account for the removed index shift
            for entry in state.ai_structure_reviews.iter_mut() {
                if let Some(parent_idx) = entry.parent_new_row_index {
                    if parent_idx > idx {
                        entry.parent_new_row_index = Some(parent_idx - 1);
                    }
                }
            }
        }
    }
}

pub fn process_new_decline(indices: &[usize], state: &mut EditorWindowState) {
    let mut sorted = indices.to_vec();
    if sorted.is_empty() {
        return;
    }
    sorted.sort_unstable();
    sorted.dedup();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    for idx in sorted {
        if idx < state.ai_new_row_reviews.len() {
            // Remove structure entries for this index before removal
            take_structure_entries_for_new(state, idx);
            // Remove the review entry
            state.ai_new_row_reviews.remove(idx);
            // CRITICAL: Update all structure entries with higher parent_new_row_index
            // to account for the removed index shift
            for entry in state.ai_structure_reviews.iter_mut() {
                if let Some(parent_idx) = entry.parent_new_row_index {
                    if parent_idx > idx {
                        entry.parent_new_row_index = Some(parent_idx - 1);
                    }
                }
            }
        }
    }
}
