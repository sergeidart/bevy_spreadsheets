// Handlers for accepting/cancelling AI row suggestions (moved out of monolithic file)
use crate::sheets::events::{AddSheetRowRequest, UpdateCellEvent};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::logic::lineage_helpers::resolve_parent_key_from_lineage;
use crate::ui::elements::editor::state::{
    EditorWindowState, NewRowReview, ReviewChoice, StructureReviewEntry,
};
use crate::ui::elements::ai_review::serialization_helpers::{
    serialize_structure_rows_to_json, get_rows_to_serialize,
    resolve_parent_key_for_new_row, adjust_parent_indices_after_removal,
};
use bevy::prelude::{info, warn, EventWriter};

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

/// Queue a new row addition (legacy helper - kept for potential future use)
#[allow(dead_code)]
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

        // Get the appropriate rows based on decision status
        let rows_to_serialize = get_rows_to_serialize(&entry);

        // Serialize rows to JSON using shared helper
        info!(
            "Serializing structure for new row: {} rows, schema_headers={:?}",
            rows_to_serialize.len(),
            entry.schema_headers
        );
        let json_string = serialize_structure_rows_to_json(rows_to_serialize, &entry.schema_headers);

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
    registry: &SheetRegistry,
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

            // Apply ancestor overrides: map human-readable values to numeric row_index for the full chain
            if !state.virtual_structure_stack.is_empty() {
                if let Some(child_meta) = registry
                    .get_sheet(selected_category, active_sheet_name)
                    .and_then(|s| s.metadata.as_ref())
                {
                    // Only handle parent_key (immediate parent), not grand_N columns
                    let chain_len = state.virtual_structure_stack.len();
                    let immediate_parent_idx = chain_len - 1;
                    
                    let override_flag = *rr.key_overrides.get(&(1000 + immediate_parent_idx)).unwrap_or(&false);
                    if override_flag {
                        // Get lineage values up to immediate parent
                        let lineage_values: Vec<String> = (0..chain_len)
                            .filter_map(|i| rr.ancestor_key_values.get(i).cloned())
                            .collect();
                        
                        if !lineage_values.is_empty() {
                            // Get parent sheet name from virtual structure stack
                            if let Some(parent_ctx) = state.virtual_structure_stack.last() {
                                // Resolve parent_key from lineage
                                if let Some(parent_row_idx) = resolve_parent_key_from_lineage(
                                    registry,
                                    selected_category,
                                    &parent_ctx.parent.parent_sheet,
                                    &lineage_values,
                                ) {
                                    // Find parent_key column in child table
                                    if let Some(parent_key_col) = child_meta
                                        .columns
                                        .iter()
                                        .position(|c| c.header.eq_ignore_ascii_case("parent_key"))
                                    {
                                        cell_update_writer.write(UpdateCellEvent {
                                            category: selected_category.clone(),
                                            sheet_name: active_sheet_name.to_string(),
                                            row_index: rr.row_index,
                                            col_index: parent_key_col,
                                            new_value: parent_row_idx.to_string(),
                                        });
                                    }
                                } else {
                                    warn!("Failed to resolve parent_key from lineage: {:?}", lineage_values);
                                }
                            }
                        }
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

                // Get the appropriate rows based on decision status
                let rows_to_serialize = get_rows_to_serialize(&entry);

                // Serialize rows to JSON using shared helper
                info!(
                    "Serializing structure: {} rows, schema_headers={:?}",
                    rows_to_serialize.len(),
                    entry.schema_headers
                );
                let json_string = serialize_structure_rows_to_json(rows_to_serialize, &entry.schema_headers);

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
    _add_row_writer: &mut EventWriter<AddSheetRowRequest>, // Kept for compatibility, but now using batch queue
    registry: &SheetRegistry,
) {
    let mut sorted = indices.to_vec();
    if sorted.is_empty() {
        return;
    }
    sorted.sort_unstable();
    sorted.dedup();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    
    // Collect all new rows to add in batch
    let mut batch_rows: Vec<Vec<(usize, String)>> = Vec::new();
    
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

                        // Get the appropriate rows and serialize using shared helper
                        let rows_to_serialize = get_rows_to_serialize(&entry);
                        let json_string = serialize_structure_rows_to_json(rows_to_serialize, &entry.schema_headers);

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
                    // Creating separate new row with structure data - collect for batch
                    let mut init_vals: Vec<(usize, String)> = Vec::new();
                    // Non-structure
                    for (pos, actual_col) in nr.non_structure_columns.iter().enumerate() {
                        if let Some(val) = nr.ai.get(pos).cloned() {
                            init_vals.push((*actual_col, val));
                        }
                    }
                    
                    // Resolve parent_key for structure tables (both virtual and real)
                    if let Some((col, val)) = resolve_parent_key_for_new_row(
                        state,
                        registry,
                        selected_category,
                        active_sheet_name,
                        &nr.key_overrides,
                        &nr.ancestor_key_values,
                    ) {
                        init_vals.push((col, val));
                    }
                    
                    // Structures - serialize using shared helper
                    for entry in structure_entries {
                        if entry.rejected { continue; }
                        let rows_to_serialize = get_rows_to_serialize(&entry);
                        let json_string = serialize_structure_rows_to_json(rows_to_serialize, &entry.schema_headers);
                        let col_index = *entry.structure_path.first().unwrap_or(&0);
                        init_vals.push((col_index, json_string));
                    }
                    batch_rows.push(init_vals);
                }
            } else {
                // Creating new row with structure data - collect for batch
                let mut init_vals: Vec<(usize, String)> = Vec::new();
                for (pos, actual_col) in nr.non_structure_columns.iter().enumerate() {
                    if let Some(val) = nr.ai.get(pos).cloned() {
                        init_vals.push((*actual_col, val));
                    }
                }
                
                // Resolve parent_key for structure tables (both virtual and real)
                if let Some((col, val)) = resolve_parent_key_for_new_row(
                    state,
                    registry,
                    selected_category,
                    active_sheet_name,
                    &nr.key_overrides,
                    &nr.ancestor_key_values,
                ) {
                    init_vals.push((col, val));
                }
                
                // Structures - serialize using shared helper
                for entry in structure_entries {
                    if entry.rejected { continue; }
                    let rows_to_serialize = get_rows_to_serialize(&entry);
                    let json_string = serialize_structure_rows_to_json(rows_to_serialize, &entry.schema_headers);
                    let col_index = *entry.structure_path.first().unwrap_or(&0);
                    init_vals.push((col_index, json_string));
                }
                batch_rows.push(init_vals);
            }

            // CRITICAL: Update all structure entries with higher parent_new_row_index
            // to account for the removed index shift
            adjust_parent_indices_after_removal(state, idx);
        }
    }
    
    // Send all new rows as a batch to avoid race conditions
    if !batch_rows.is_empty() {
        info!("Queuing batch of {} new rows for addition", batch_rows.len());
        state.ai_throttled_batch_add_queue.push_back((
            selected_category.clone(),
            active_sheet_name.to_string(),
            batch_rows,
        ));
    }
}

/// Build initial values for a new row (legacy helper - kept for potential future use)
#[allow(dead_code)]
fn build_row_initial_values(
    review: &NewRowReview,
    structure_entries: Vec<StructureReviewEntry>,
) -> Vec<(usize, String)> {
    let mut init_vals: Vec<(usize, String)> = Vec::new();

    // Add non-structure column values
    for (pos, actual_col) in review.non_structure_columns.iter().enumerate() {
        if let Some(val) = review.ai.get(pos).cloned() {
            init_vals.push((*actual_col, val));
        }
    }

    // Add structure column values (serialized as JSON)
    for entry in structure_entries {
        // Skip rejected structures
        if entry.rejected {
            continue;
        }

        // Get the appropriate rows and serialize using shared helper
        let rows_to_serialize = get_rows_to_serialize(&entry);
        let json_string = serialize_structure_rows_to_json(rows_to_serialize, &entry.schema_headers);

        let col_index = *entry.structure_path.first().unwrap_or(&0);
        init_vals.push((col_index, json_string));
    }

    init_vals
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
            adjust_parent_indices_after_removal(state, idx);
        }
    }
}
