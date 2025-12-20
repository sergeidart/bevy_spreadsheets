// src/sheets/systems/ai/structure_processor/task_executor.rs
//! Main orchestration logic for structure AI processing tasks

use super::{data_preparation, python_executor};
use crate::sheets::definitions::default_ai_model_id;
use crate::sheets::events::{AiBatchResultKind, AiBatchTaskResult, StructureProcessingContext};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai::control_handler::{BatchPayload, ParentGroup};
use crate::sheets::systems::ai::utils::{build_nested_field_path, extract_structure_settings};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::systems::SendEvent;
use crate::SessionApiKey;
use bevy::prelude::*;
use bevy_tokio_tasks::TokioTasksRuntime;

pub fn process_structure_ai_jobs(
    mut state: ResMut<EditorWindowState>,
    registry: Res<SheetRegistry>,
    runtime: Res<TokioTasksRuntime>,
    commands: Commands,
    session_api_key: Res<SessionApiKey>,
) {
    let Some(job) = state.ai_pending_structure_jobs.pop_front() else {
        return;
    };

    info!(
        "Processing structure AI job for {:?}/{} path {:?} with {} target rows: {:?}",
        job.root_category, job.root_sheet, job.structure_path, job.target_rows.len(), job.target_rows
    );

    let Some(root_sheet) = registry.get_sheet(&job.root_category, &job.root_sheet) else {
        error!("Structure AI job failed: root sheet {:?}/{} not found", job.root_category, job.root_sheet);
        state.mark_structure_result_received();
        return;
    };

    let Some(root_meta) = root_sheet.metadata.as_ref() else {
        error!("Structure AI job failed: metadata missing for {:?}/{}", job.root_category, job.root_sheet);
        state.mark_structure_result_received();
        return;
    };

    let Some(structure_fields) = root_meta.structure_fields_for_path(&job.structure_path) else {
        error!(
            "Structure AI job failed: no schema fields for path {:?} in {:?}/{}",
            job.structure_path, job.root_category, job.root_sheet
        );
        state.mark_structure_result_received();
        return;
    };

    let all_structure_headers: Vec<String> =
        structure_fields.iter().map(|f| f.header.clone()).collect();

    let nested_field_path = build_nested_field_path(&job.structure_path, root_meta);
    let (key_col_index, allow_row_additions) =
        extract_structure_settings(&job.structure_path, root_meta, Some(&registry), &job.root_category);
    
    info!(
        "Structure settings extracted for path {:?}: key_col_index={:?}, allow_row_additions={}",
        job.structure_path, key_col_index, allow_row_additions
    );
    
    let key_header = key_col_index
        .and_then(|idx| root_meta.columns.get(idx))
        .map(|col| col.header.clone());

    let (included_indices, column_contexts) = data_preparation::build_column_contexts(&structure_fields);

    // Get the structure column name for database queries
    let structure_col_name = if let Some(&first_col_idx) = job.structure_path.first() {
        root_meta
            .columns
            .get(first_col_idx)
            .map(|col| col.header.clone())
            .unwrap_or_else(|| format!("Column_{}", first_col_idx))
    } else {
        error!("Structure path is empty!");
        state.mark_structure_result_received();
        return;
    };

    // Build parent groups from target rows
    let (parent_groups, row_partitions) = data_preparation::build_parent_groups(
        &job.target_rows,
        &state,
        root_sheet,
        &structure_fields,
        &all_structure_headers,
        &included_indices,
        &nested_field_path,
        &job.structure_path,
        key_col_index,
        &key_header,
        root_meta,
        &job.root_category,
        &job.root_sheet,
        &structure_col_name,
    );

    if parent_groups.is_empty() {
        warn!("Structure AI job for {:?}/{} path {:?} has no valid parent groups",
            job.root_category, job.root_sheet, job.structure_path);
        state.mark_structure_result_received();
        return;
    }

    info!("Built {} parent groups with {} total rows for structure batch",
        parent_groups.len(), parent_groups.iter().map(|g| g.rows.len()).sum::<usize>());

    // Build ancestor lineage from navigation stack (for deep hierarchy support)
    // Order: root-to-leaf, e.g., ["Su-7B", "Underfuselage Pylon"] when at grandchild level
    let ancestor_display_values: Vec<String> = state.ai_navigation_stack
        .iter()
        .filter_map(|ctx| ctx.parent_display_name.clone())
        .collect();
    
    info!("Ancestor lineage from navigation stack: {:?}", ancestor_display_values);

    // Build and send AI payload
    let payload = build_payload(
        &job,
        root_meta,
        column_contexts,
        parent_groups,
        allow_row_additions,
        &ancestor_display_values,
    );

    let payload_json = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to serialize structure payload: {}", e);
            state.mark_structure_result_received();
            return;
        }
    };

    // Log the request
    if let Ok(pretty_payload) = serde_json::to_string_pretty(&payload) {
        let status = format!("Sending structure request for {} row(s)...", job.target_rows.len());
        state.add_ai_call_log(status, None, Some(pretty_payload), false);
    }

    python_executor::rewrite_python_processor();

    // Spawn background AI task
    let ancestor_count = ancestor_display_values.len();
    spawn_ai_task(
        runtime,
        commands,
        session_api_key.0.clone(),
        payload_json,
        job,
        included_indices,
        row_partitions,
        ancestor_count,
    );
}




/// Build AI batch payload
fn build_payload(
    job: &crate::ui::elements::editor::state::StructureSendJob,
    root_meta: &crate::sheets::sheet_metadata::SheetMetadata,
    column_contexts: Vec<Option<String>>,
    parent_groups: Vec<ParentGroup>,
    allow_row_additions: bool,
    ancestor_display_values: &[String],
) -> BatchPayload {
    let structure_label = job.label.as_deref().unwrap_or("structure");
    
    // Build rows_data by prepending ancestor lineage + parent key to each child row
    // Format: [ancestor1, ancestor2, ..., parent_key, child_col1, child_col2, ...]
    // This provides full context for deep hierarchy (e.g., "Su-7B" > "Underfuselage Pylon" > grandchild data)
    let mut rows_data = Vec::new();
    for group in &parent_groups {
        let parent_key_value = &group.parent_key.key;
        if group.rows.is_empty() {
            // No existing child rows - send one row with ancestors + parent key
            let schema_len = column_contexts.len();
            let mut empty_row: Vec<String> = ancestor_display_values.iter().cloned().collect();
            empty_row.push(parent_key_value.clone());
            empty_row.extend(vec![String::new(); schema_len]);
            rows_data.push(empty_row);
        } else {
            // Send all existing child rows with ancestors + parent key prepended
            for child_row in &group.rows {
                let mut full_row: Vec<String> = ancestor_display_values.iter().cloned().collect();
                full_row.push(parent_key_value.clone());
                full_row.extend(child_row.iter().cloned());
                rows_data.push(full_row);
            }
        }
    }
    
    // Build column contexts with ancestor contexts + parent key context + data columns
    // Ancestor contexts are None for now (could be enhanced later with actual AI contexts)
    let ancestor_contexts: Vec<Option<String>> = ancestor_display_values
        .iter()
        .map(|_| None) // TODO: Could walk lineage to get AI contexts for ancestors
        .collect();
    
    let parent_key_context = parent_groups
        .first()
        .and_then(|g| g.parent_key.context.clone());
    
    let mut full_column_contexts = ancestor_contexts;
    full_column_contexts.push(parent_key_context);
    full_column_contexts.extend(column_contexts);
    
    let ancestor_count = ancestor_display_values.len();
    let user_prompt = if ancestor_count > 0 {
        format!(
            "Fill in or correct the '{}' rows. The first {} columns are the ancestor lineage, followed by the parent identifier - do not modify them. Fill in the remaining columns for each row.",
            structure_label, ancestor_count + 1
        )
    } else {
        format!(
            "Fill in or correct the '{}' rows. The first column is the parent identifier - do not modify it. Fill in the remaining columns for each row.",
            structure_label
        )
    };

    // Use exact same format as regular AI calls - no special metadata
    BatchPayload {
        ai_model_id: if root_meta.ai_model_id.is_empty() {
            default_ai_model_id()
        } else {
            root_meta.ai_model_id.clone()
        },
        general_sheet_rule: root_meta.ai_general_rule.clone(),
        column_contexts: full_column_contexts,
        rows_data,
        requested_grounding_with_google_search: root_meta
            .requested_grounding_with_google_search
            .unwrap_or(false),
        allow_row_additions,
        key_prefix_count: None,
        key_prefix_headers: None,
        parent_groups: None,
        user_prompt,
    }
}

/// Spawn async AI processing task
#[allow(clippy::too_many_arguments)]
fn spawn_ai_task(
    runtime: Res<TokioTasksRuntime>,
    mut commands: Commands,
    api_key: Option<String>,
    payload_json: String,
    job: crate::ui::elements::editor::state::StructureSendJob,
    included_indices: Vec<usize>,
    row_partitions: Vec<usize>,
    ancestor_count: usize,
) {
    let commands_entity = commands.spawn_empty().id();
    let api_key_for_task = api_key;
    let included_cols_clone = included_indices;
    let job_clone = job.clone();
    let row_partitions_clone = row_partitions;

    runtime.spawn_background_task(move |mut ctx| async move {
        let api_key_value = match api_key_for_task {
            Some(k) if !k.is_empty() => k,
            _ => {
                send_error_result(ctx, commands_entity, "API Key not set".to_string(),
                    &job_clone, &included_cols_clone, &row_partitions_clone, ancestor_count).await;
                return;
            }
        };

        let (result, raw_response, _updated_partitions) =
            python_executor::execute_python_ai_query(api_key_value, payload_json).await;

        let original_partitions = row_partitions_clone;

        ctx.run_on_main_thread(move |world_ctx| {
            world_ctx.world.commands().entity(commands_entity).insert(SendEvent::<AiBatchTaskResult> {
                event: AiBatchTaskResult {
                    original_row_indices: job_clone.target_rows.clone(),
                    result,
                    raw_response,
                    included_non_structure_columns: included_cols_clone,
                    key_prefix_count: 0,
                    kind: AiBatchResultKind::Root {
                        structure_context: Some(StructureProcessingContext {
                            root_category: job_clone.root_category.clone(),
                            root_sheet: job_clone.root_sheet.clone(),
                            structure_path: job_clone.structure_path.clone(),
                            target_rows: job_clone.target_rows.clone(),
                            original_row_partitions: original_partitions,
                            generation_id: job_clone.generation_id,
                            ancestor_count,
                        }),
                    },
                },
            });
        }).await;
    });
}

/// Send error result back to main thread
async fn send_error_result(
    mut ctx: bevy_tokio_tasks::TaskContext,
    commands_entity: bevy::ecs::entity::Entity,
    error_msg: String,
    job: &crate::ui::elements::editor::state::StructureSendJob,
    included_cols: &[usize],
    row_partitions: &[usize],
    ancestor_count: usize,
) {
    let job_clone = job.clone();
    let included_cols_clone = included_cols.to_vec();
    let row_partitions_clone = row_partitions.to_vec();

    ctx.run_on_main_thread(move |world_ctx| {
        world_ctx.world.commands().entity(commands_entity).insert(SendEvent::<AiBatchTaskResult> {
            event: AiBatchTaskResult {
                original_row_indices: job_clone.target_rows.clone(),
                result: Err(error_msg),
                raw_response: None,
                included_non_structure_columns: included_cols_clone,
                key_prefix_count: 0,
                kind: AiBatchResultKind::Root {
                    structure_context: Some(StructureProcessingContext {
                        root_category: job_clone.root_category.clone(),
                        root_sheet: job_clone.root_sheet.clone(),
                        structure_path: job_clone.structure_path.clone(),
                        target_rows: job_clone.target_rows.clone(),
                        original_row_partitions: row_partitions_clone.clone(),
                        generation_id: job_clone.generation_id,
                        ancestor_count,
                    }),
                },
            },
        });
    }).await;
}
