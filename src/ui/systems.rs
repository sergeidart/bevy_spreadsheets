// src/ui/systems.rs
use crate::{
    sheets::events::{AiBatchTaskResult, AiTaskResult, SheetOperationFeedback},
    sheets::resources::SheetRegistry,
    sheets::systems::io::save::save_single_sheet,
    ui::{
        elements::editor::state::{AiModeState, EditorWindowState},
        UiFeedbackState,
    },
};
use bevy::prelude::*;
use std::any;
use std::collections::HashMap;

use crate::ui::elements::editor::state::{NewRowReview, ReviewChoice, RowReview}; // retained for batch result construction

pub fn handle_ui_feedback(
    mut feedback_events: EventReader<SheetOperationFeedback>,
    mut ui_feedback_state: ResMut<UiFeedbackState>,
    mut state: ResMut<crate::ui::elements::editor::state::EditorWindowState>,
) {
    let mut last_message = None;
    for event in feedback_events.read() {
        last_message = Some((event.message.clone(), event.is_error));
        // Append to bottom log buffer
        if !state.ai_raw_output_display.is_empty() {
            state.ai_raw_output_display.push('\n');
        }
        state.ai_raw_output_display.push_str(&event.message);
        if event.is_error {
            state.ai_output_panel_visible = true; // open log for errors
        }
        // Prioritize showing the first non-error, or the last error
        if !event.is_error {
            break;
        }
    }
    if let Some((msg, is_error)) = last_message {
        ui_feedback_state.last_message = msg;
        ui_feedback_state.is_error = is_error;
        if is_error {
            warn!("UI Feedback (Error): {}", ui_feedback_state.last_message);
        } else {
            info!("UI Feedback: {}", ui_feedback_state.last_message);
        }
    }
}

pub fn handle_ai_task_results(
    mut ev_ai_results: EventReader<AiTaskResult>,
    mut state: ResMut<EditorWindowState>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    // Early-out when there are no events to process to avoid per-frame log spam
    if ev_ai_results.is_empty() {
        return;
    }
    debug!(
        "handle_ai_task_results: processing {} event(s). Current AI Mode: {:?}",
        ev_ai_results.len(),
        state.ai_mode
    );
    if state.ai_mode != AiModeState::Submitting && state.ai_mode != AiModeState::ResultsReady {
        if !ev_ai_results.is_empty() {
            let event_count = ev_ai_results.len();
            info!(
                "Ignoring {} AI result(s) received while not in Submitting/ResultsReady state (current: {:?})",
                event_count, state.ai_mode
            );
            ev_ai_results.clear(); // Consume events
        }
        return;
    }

    let mut received_at_least_one_result = false;
    let mut all_tasks_successful_this_batch = true; // Track success for this batch of events

    for ev in ev_ai_results.read() {
        received_at_least_one_result = true;
        info!(
            "Received AI task result for row {}. Raw response present: {}",
            ev.original_row_index,
            ev.raw_response.is_some()
        );

        // Update the raw output display first
        if let Some(raw) = &ev.raw_response {
            state.ai_raw_output_display = raw.clone();
        } else if let Err(e) = &ev.result {
            // If no raw response but there's an error, display the error
            state.ai_raw_output_display = format!(
                "Error processing AI result for row {}: {}",
                ev.original_row_index, e
            );
        }

        match &ev.result {
            Ok(suggestion) => {
                info!(
                    "  AI Task Success for row {}: {:?}",
                    ev.original_row_index, suggestion
                );
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
                // Convert single-row suggestion directly into snapshot model entry for consistency
                let included = ev.included_non_structure_columns.clone();
                let mut original_snapshot: Vec<String> = Vec::with_capacity(included.len());
                let mut ai_snapshot: Vec<String> = Vec::with_capacity(included.len());
                for (logical_i, _actual_col) in included.iter().enumerate() {
                    original_snapshot.push(String::new()); // original unknown in this lightweight path
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
                error!(
                    "  AI Task Failure for row {}: {}",
                    ev.original_row_index, err_msg
                );
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
                // The raw_output_display is already set with the error or raw response
                all_tasks_successful_this_batch = false;
            }
        }
    }

    if received_at_least_one_result && state.ai_mode == AiModeState::Submitting {
        if all_tasks_successful_this_batch {
            info!("All AI results in this batch processed successfully, moving to ResultsReady state.");
            state.ai_mode = AiModeState::ResultsReady;
            // state.ai_prompt_display can remain to show what was sent
        } else {
            error!("One or more AI tasks failed in this batch. Reverting to Preparing state.");
            state.ai_mode = AiModeState::Preparing; // Or Idle, depending on desired flow
            state.ai_row_reviews.clear();
            // state.ai_raw_output_display will show the last error/raw output
        }
    }
}

#[derive(Component)]
pub struct SendEvent<E: Event> {
    pub event: E,
}

pub fn forward_events<E: Event + Clone + std::fmt::Debug>(
    mut commands: Commands,
    mut writer: EventWriter<E>,
    query: Query<(Entity, &SendEvent<E>)>,
    mut event_type_name: Local<String>,
) {
    if event_type_name.is_empty() {
        *event_type_name = any::type_name::<E>()
            .split("::")
            .last()
            .unwrap_or("UnknownEvent")
            .to_string();
    }

    let mut count = 0;
    for (entity, send_event_component) in query.iter() {
        count += 1;
        debug!(
            "Forwarding event type '{}' #{}: {:?}",
            *event_type_name, count, send_event_component.event
        );
        writer.write(send_event_component.event.clone());
        commands.entity(entity).remove::<SendEvent<E>>();
        commands.entity(entity).despawn(); // Despawn entity after forwarding
    }

    if count > 0 {
        info!(
            "Forwarded {} instance(s) of event type '{}'.",
            count, *event_type_name
        );
    }
}

/// Clears transient UI feedback when a sheet's data is modified in the registry.
/// This ensures the "service row" (status/error message) is hidden after switching or
/// changing sheets and will reappear if a new feedback event is emitted afterwards.
pub fn clear_ui_feedback_on_sheet_change(
    state: Res<EditorWindowState>,
    mut ui_feedback_state: ResMut<UiFeedbackState>,
    mut last_selection: Local<Option<(Option<String>, Option<String>)>>,
) {
    // Determine current selection tuple
    let current_sel = (
        state.selected_category.clone(),
        state.selected_sheet_name.clone(),
    );

    // If we have a last selection and it's different -> user switched sheets (open another one)
    if let Some(prev) = last_selection.as_ref() {
        if prev != &current_sel {
            // Only clear when the selection actually changed (not on initial startup)
            ui_feedback_state.last_message.clear();
            ui_feedback_state.is_error = false;
            trace!("Cleared UI feedback due to sheet selection change.");
        }
    }

    // Update last selection stored locally
    *last_selection = Some(current_sel);
}

// Apply pending structure key column selection (if any) and compute ancestor chain
pub fn apply_pending_structure_key_selection(
    mut state: ResMut<EditorWindowState>,
    mut registry: ResMut<SheetRegistry>,
) {
    if let Some((cat, sheet, structure_col_index, new_key_opt)) =
        state.pending_structure_key_apply.take()
    {
        let mut root_parent_link: Option<crate::sheets::definitions::StructureParentLink> = None;
        let mut changed = false;
        let mut is_virtual = false;
        if let Some(sheet_data) = registry.get_sheet(&cat, &sheet) {
            if let Some(meta_ro) = &sheet_data.metadata {
                is_virtual = meta_ro.structure_parent.is_some();
                root_parent_link = meta_ro.structure_parent.clone();
            }
        }
        if is_virtual {
            // Edit from virtual sheet: update parent field only; do not modify parent column-level key
            if let Some(parent_link) = &root_parent_link {
                if let Some(parent_sheet) =
                    registry.get_sheet_mut(&parent_link.parent_category, &parent_link.parent_sheet)
                {
                    if let Some(parent_meta) = &mut parent_sheet.metadata {
                        if let Some(parent_col) =
                            parent_meta.columns.get_mut(parent_link.parent_column_index)
                        {
                            if let Some(fields) = parent_col.structure_schema.as_mut() {
                                if let Some(field) = fields.get_mut(structure_col_index) {
                                    if field.structure_key_parent_column_index != new_key_opt {
                                        changed = true;
                                    }
                                    field.structure_key_parent_column_index = new_key_opt;
                                }
                            }
                        }
                        if changed {
                            let meta_clone = parent_meta.clone();
                            save_single_sheet(registry.as_ref(), &meta_clone);
                        }
                    }
                }
            }
            // Also mirror the selection into the virtual sheet's ColumnDefinition so UI reflects it immediately
            if let Some(vsheet) = registry.get_sheet_mut(&cat, &sheet) {
                if let Some(vmeta) = &mut vsheet.metadata {
                    if let Some(vcol) = vmeta.columns.get_mut(structure_col_index) {
                        vcol.structure_key_parent_column_index = new_key_opt;
                    }
                }
            }
        } else {
            // Edit from parent sheet: update parent column-level key and clear ancestor (recomputed below)
            if let Some(sheet_data) = registry.get_sheet_mut(&cat, &sheet) {
                if let Some(meta) = &mut sheet_data.metadata {
                    if let Some(col) = meta.columns.get_mut(structure_col_index) {
                        if col.structure_key_parent_column_index != new_key_opt {
                            changed = true;
                        }
                        col.structure_key_parent_column_index = new_key_opt;
                        col.structure_ancestor_key_parent_column_indices = Some(Vec::new());
                    }
                }
            }
        }
        let mut collected: Vec<usize> = Vec::new();
        let mut current_parent = root_parent_link;
        let mut safety = 0;
        while let Some(parent_link) = current_parent.clone() {
            if safety > 32 {
                break;
            }
            safety += 1;
            if let Some(parent_sheet) =
                registry.get_sheet(&parent_link.parent_category, &parent_link.parent_sheet)
            {
                if let Some(parent_meta) = &parent_sheet.metadata {
                    if let Some(parent_col) =
                        parent_meta.columns.get(parent_link.parent_column_index)
                    {
                        if let Some(kidx) = parent_col.structure_key_parent_column_index {
                            collected.push(kidx);
                        }
                    }
                    current_parent = parent_meta.structure_parent.clone();
                    continue;
                }
            }
            break;
        }
        collected.reverse();
        if !is_virtual {
            if let Some(sheet_data) = registry.get_sheet_mut(&cat, &sheet) {
                if let Some(meta) = &mut sheet_data.metadata {
                    if let Some(col) = meta.columns.get_mut(structure_col_index) {
                        let existing = col
                            .structure_ancestor_key_parent_column_indices
                            .clone()
                            .unwrap_or_default();
                        if existing != collected {
                            changed = true;
                        }
                        col.structure_ancestor_key_parent_column_indices = Some(collected);
                    }
                    let meta_clone_for_save = if changed { Some(meta.clone()) } else { None };
                    if let Some(meta_clone) = meta_clone_for_save {
                        save_single_sheet(registry.as_ref(), &meta_clone);
                    }
                }
            }
        }
    }
}

pub fn handle_ai_batch_results(
    mut ev_batch: EventReader<AiBatchTaskResult>,
    mut state: ResMut<EditorWindowState>,
    registry: Res<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    if ev_batch.is_empty() {
        return;
    }
    // Currently only process first batch event per frame
    for ev in ev_batch.read() {
        match &ev.result {
            Ok(rows) => {
                let originals = ev.original_row_indices.len();
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
                }
                // Legacy ai_suggestions map removed
                let (orig_slice, extra_slice) = if originals == 0 {
                    (&[][..], &rows[..])
                } else {
                    rows.split_at(originals)
                };
                // Record prefix count so review UI can lock those cells (keys no longer prefixed; keep 0)
                state.ai_context_only_prefix_count = ev.key_prefix_count;
                // Populate read-only key context (headers + values) for each original row, based on current
                // virtual ancestry (same logic as send payload "keys" block). This is for display only.
                state.ai_context_prefix_by_row.clear();
                if !state.virtual_structure_stack.is_empty() {
                    // Build ancestor key spec: (cat, sheet, fixed row index, key col index) and headers
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
                                    ancestors_with_keys.push((anc_cat, anc_sheet, anc_row_idx, key_col_index));
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

                // Build new unified review model (dual snapshots) including structure placeholders
                state.ai_row_reviews.clear();
                state.ai_new_row_reviews.clear();
                // Determine full column span needed (sheet metadata + included structure columns)
                // We infer max column index from included_non_structure_columns; structure columns are everything not listed.
                let included = ev.included_non_structure_columns.clone();
                let (cat_ctx, sheet_ctx) = state.current_sheet_context();
                // Build per-original row reviews
                for (i, &row_index) in ev.original_row_indices.iter().enumerate() {
                    let suggestion_full = &orig_slice[i];
                    let suggestion = if ev.key_prefix_count > 0
                        && suggestion_full.len() >= ev.key_prefix_count
                    {
                        &suggestion_full[ev.key_prefix_count..]
                    } else {
                        suggestion_full
                    };
                    // Prepare original snapshot (editable subset only, but we keep Vec sized to included.len())
                    let mut original_snapshot: Vec<String> = Vec::with_capacity(included.len());
                    let mut ai_snapshot: Vec<String> = Vec::with_capacity(included.len());
                    // Fetch original row from registry (non-structure columns only)
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
                            // Default to AI only if different; else Original
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
                // New rows: strip key prefix (if any) then map included indices
                // Precompute map from normalized first included column values across the entire active sheet (strip CR/LF, trim, lowercase)
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
                    let new_row =
                        if ev.key_prefix_count > 0 && new_row_full.len() >= ev.key_prefix_count {
                            &new_row_full[ev.key_prefix_count..]
                        } else {
                            new_row_full
                        };
                    let mut ai_snapshot: Vec<String> = Vec::with_capacity(included.len());
                    for (logical_i, _actual_col) in included.iter().enumerate() {
                        ai_snapshot.push(new_row.get(logical_i).cloned().unwrap_or_default());
                    }

                    // Detect duplicate by first column value
                    let mut duplicate_match_row: Option<usize> = None;
                    let mut original_for_merge: Option<Vec<String>> = None;
                    let mut choices: Option<Vec<ReviewChoice>> = None;
                    let mut merge_selected = false;
                    let merge_decided = false;
                    if let Some(first_val) = ai_snapshot.get(0) {
                        let normalized_first =
                            first_val.replace(['\r', '\n'], "").trim().to_lowercase();
                        if let Some(matched_row_index) =
                            first_col_value_to_row.get(&normalized_first)
                        {
                            duplicate_match_row = Some(*matched_row_index);
                            // Build original snapshot from registry for the matched row
                            if let Some(sheet_name) = &sheet_ctx {
                                if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
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
                                        original_for_merge = Some(orig_vec.clone());
                                        // Default choices: prefer AI where different
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
                                        merge_selected = true; // default to Merge when duplicate detected
                                    }
                                }
                            }
                        }
                    }
                    state.ai_new_row_reviews.push(NewRowReview {
                        ai: ai_snapshot,
                        non_structure_columns: included.clone(),
                        accept: true,
                        duplicate_match_row,
                        original_for_merge,
                        choices,
                        merge_selected,
                        merge_decided,
                    });
                }
                // Precompute undecided merge flag once
                state.ai_batch_has_undecided_merge = state
                    .ai_new_row_reviews
                    .iter()
                    .any(|nr| nr.duplicate_match_row.is_some() && !nr.merge_decided);
                state.ai_batch_review_active = true;
                state.ai_mode = AiModeState::ResultsReady;
                if ev.prompt_only {
                    state.last_ai_prompt_only = true;
                }
                info!(
                    "Processed AI batch result: {} originals, {} new rows (prompt_only={})",
                    originals,
                    state.ai_new_row_reviews.len(),
                    ev.prompt_only
                );
            }
            Err(err) => {
                if let Some(raw) = &ev.raw_response {
                    state.ai_raw_output_display =
                        format!("Batch Error: {}\n--- Raw Model Output ---\n{}", err, raw);
                } else {
                    state.ai_raw_output_display =
                        format!("Batch Error: {} (no raw output returned)", err);
                }
                state.ai_output_panel_visible = true; // ensure visible
                feedback_writer.write(SheetOperationFeedback {
                    message: format!("AI batch error: {}", err),
                    is_error: true,
                });
                state.ai_mode = AiModeState::Preparing;
            }
        }
        break;
    }
}

/// Applies at most one queued AI change per frame to avoid contention and match per-row timing.
pub fn apply_throttled_ai_changes(
    mut state: ResMut<EditorWindowState>,
    mut cell_update_writer: EventWriter<crate::sheets::events::UpdateCellEvent>,
    mut add_row_writer: EventWriter<crate::sheets::events::AddSheetRowRequest>,
) {
    if let Some(action) = state.ai_throttled_apply_queue.pop_front() {
        let (cat, sheet_opt) = state.current_sheet_context();
        if let Some(sheet) = sheet_opt.clone() {
            match action {
                crate::ui::elements::editor::state::ThrottledAiAction::UpdateCell {
                    row_index,
                    col_index,
                    value,
                } => {
                    cell_update_writer.write(crate::sheets::events::UpdateCellEvent {
                        category: cat.clone(),
                        sheet_name: sheet,
                        row_index,
                        col_index,
                        new_value: value,
                    });
                }
                crate::ui::elements::editor::state::ThrottledAiAction::AddRow {
                    initial_values,
                } => {
                    add_row_writer.write(crate::sheets::events::AddSheetRowRequest {
                        category: cat.clone(),
                        sheet_name: sheet,
                        initial_values: if initial_values.is_empty() {
                            None
                        } else {
                            Some(initial_values)
                        },
                    });
                }
            }
        }
    }
}
