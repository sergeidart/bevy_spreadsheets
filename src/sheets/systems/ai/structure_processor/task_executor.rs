// src/sheets/systems/ai/structure_processor/task_executor.rs
//! Main orchestration logic for structure AI processing tasks

use super::{new_row_extractor, python_executor};
use crate::sheets::definitions::default_ai_model_id;
use crate::sheets::events::{AiBatchResultKind, AiBatchTaskResult, StructureProcessingContext};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai::control_handler::{BatchPayload, ParentGroup};
use crate::sheets::systems::ai::utils::{build_nested_field_path, extract_structure_settings};
use crate::ui::elements::ai_review::ai_context_utils::decorate_context_with_type;
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
        extract_structure_settings(&job.structure_path, root_meta);
    let key_header = key_col_index
        .and_then(|idx| root_meta.columns.get(idx))
        .map(|col| col.header.clone());

    let (included_indices, column_contexts) = build_column_contexts(&structure_fields);

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
    let (parent_groups, row_partitions) = build_parent_groups(
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

    // Build and send AI payload
    let payload = build_payload(
        &job,
        root_meta,
        column_contexts,
        parent_groups,
        allow_row_additions,
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
    spawn_ai_task(
        runtime,
        commands,
        session_api_key.0.clone(),
        payload_json,
        job,
        included_indices,
        row_partitions,
    );
}

/// Build column contexts and included indices
fn build_column_contexts(
    structure_fields: &[crate::sheets::definitions::StructureFieldDefinition],
) -> (Vec<usize>, Vec<Option<String>>) {
    let mut included_indices = Vec::new();
    let mut column_contexts = Vec::new();

    for (idx, field) in structure_fields.iter().enumerate() {
        // Skip structure columns (nested structures)
        if matches!(
            field.validator,
            Some(crate::sheets::definitions::ColumnValidator::Structure)
        ) {
            continue;
        }
        // Skip columns explicitly excluded
        if matches!(field.ai_include_in_send, Some(false)) {
            continue;
        }
        included_indices.push(idx);
        column_contexts.push(decorate_context_with_type(
            field.ai_context.as_ref(),
            field.data_type,
        ));
    }

    (included_indices, column_contexts)
}

/// Read structure child rows from database for existing grid rows
fn read_structure_data_from_db(
    category: &Option<String>,
    parent_table_name: &str,
    structure_col_name: &str,
    target_row: usize,
    root_sheet: &crate::sheets::definitions::SheetGridData,
    all_structure_headers: &[String],
) -> Vec<Vec<String>> {
    // Get database path
    let Some(cat) = category.as_ref() else {
        error!("No category for structure data extraction");
        return vec![vec![String::new(); all_structure_headers.len()]];
    };
    
    let base = crate::sheets::systems::io::get_default_data_base_path();
    let db_path = base.join(format!("{}.db", cat));
    
    if !db_path.exists() {
        warn!("Database {:?} does not exist for structure data extraction", db_path);
        return vec![vec![String::new(); all_structure_headers.len()]];
    }
    
    // Open database connection
    let conn = match crate::sheets::database::connection::DbConnection::open_existing(&db_path) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to open database {:?}: {}", db_path, e);
            return vec![vec![String::new(); all_structure_headers.len()]];
        }
    };
    
    // Get parent row's row_index value from grid
    let parent_row = match root_sheet.grid.get(target_row) {
        Some(r) => r,
        None => {
            error!("Target row {} not found in grid", target_row);
            return vec![vec![String::new(); all_structure_headers.len()]];
        }
    };
    
    // row_index is always at index 0
    let parent_row_index: i64 = match parent_row.get(0).and_then(|s| s.parse().ok()) {
        Some(idx) => idx,
        None => {
            error!("Failed to parse row_index from target row {}", target_row);
            return vec![vec![String::new(); all_structure_headers.len()]];
        }
    };
    
    info!(
        "Reading structure data: table={}, parent_row_index={}, headers={}",
        format!("{}_{}", parent_table_name, structure_col_name),
        parent_row_index,
        all_structure_headers.len()
    );
    
    // Read structure child rows using parent_row_index (structure tables use parent_key = row_index)
    match crate::sheets::database::reader::queries::read_structure_child_rows(
        &conn,
        parent_table_name,
        structure_col_name,
        parent_row_index,
        all_structure_headers,
    ) {
        Ok(rows) => {
            if rows.is_empty() {
                info!(
                    "No structure child rows found for parent_row_index={} in {}",
                    parent_row_index,
                    format!("{}_{}", parent_table_name, structure_col_name)
                );
                vec![vec![String::new(); all_structure_headers.len()]]
            } else {
                info!(
                    "Loaded {} structure child rows from database for parent_row_index={}",
                    rows.len(),
                    parent_row_index
                );
                rows
            }
        }
        Err(e) => {
            error!(
                "Failed to read structure child rows for parent_row_index={}: {}",
                parent_row_index, e
            );
            vec![vec![String::new(); all_structure_headers.len()]]
        }
    }
}

/// Build parent groups from target rows
#[allow(clippy::too_many_arguments)]
fn build_parent_groups(
    target_rows: &[usize],
    state: &EditorWindowState,
    root_sheet: &crate::sheets::definitions::SheetGridData,
    structure_fields: &[crate::sheets::definitions::StructureFieldDefinition],
    all_structure_headers: &[String],
    included_indices: &[usize],
    nested_field_path: &[String],
    job_structure_path: &[usize],
    key_col_index: Option<usize>,
    key_header: &Option<String>,
    root_meta: &crate::sheets::sheet_metadata::SheetMetadata,
    category: &Option<String>,
    parent_table_name: &str,
    structure_col_name: &str,
) -> (Vec<ParentGroup>, Vec<usize>) {
    let mut parent_groups = Vec::new();
    let mut row_partitions = Vec::new();

    for &target_row in target_rows {
        // Check if this is a new row context or existing row
        if let Some(context) = state.ai_structure_new_row_contexts.get(&target_row) {
            let (parent_key, group_rows, partition_size) = new_row_extractor::extract_from_new_row_context(
                target_row,
                context,
                state,
                root_sheet,
                structure_fields,
                included_indices,
                nested_field_path,
                job_structure_path,
                key_col_index,
                key_header,
                root_meta,
            );

            row_partitions.push(partition_size);
            parent_groups.push(ParentGroup { parent_key, rows: group_rows });
        } else {
            // Existing row - read from database directly
            let root_row = match root_sheet.grid.get(target_row) {
                Some(r) => r,
                None => {
                    error!("Target row {} not found in grid", target_row);
                    continue;
                }
            };
            
            // Get the key column value for this row - use first data column (typically Name at index 1)
            let key_value = root_row.get(1).cloned().unwrap_or_default();
            
            info!(
                "Existing row {}: extracted parent key_value='{}' from column 1 (Name)",
                target_row, key_value
            );
            
            // Build parent key info
            let parent_key = crate::sheets::systems::ai::control_handler::ParentKeyInfo {
                context: if key_header.is_some() && key_col_index.is_some() {
                    root_meta
                        .columns
                        .get(key_col_index.unwrap())
                        .and_then(|col| col.ai_context.clone())
                } else {
                    None
                },
                key: key_value.clone(),
            };
            
            // Read structure data from database (not from grid count strings!)
            let all_rows = read_structure_data_from_db(
                category,
                parent_table_name,
                structure_col_name,
                target_row,
                root_sheet,
                all_structure_headers,
            );
            
            // Filter rows to only include columns that match included_indices
            let filtered_rows: Vec<Vec<String>> = all_rows
                .into_iter()
                .map(|row| {
                    included_indices
                        .iter()
                        .map(|&idx| row.get(idx).cloned().unwrap_or_default())
                        .collect()
                })
                .collect();
            
            let partition_size = filtered_rows.len();
            row_partitions.push(partition_size);
            parent_groups.push(ParentGroup {
                parent_key,
                rows: filtered_rows,
            });
        }
    }

    (parent_groups, row_partitions)
}

/// Build AI batch payload
fn build_payload(
    job: &crate::ui::elements::editor::state::StructureSendJob,
    root_meta: &crate::sheets::sheet_metadata::SheetMetadata,
    column_contexts: Vec<Option<String>>,
    parent_groups: Vec<ParentGroup>,
    allow_row_additions: bool,
) -> BatchPayload {
    let structure_label = job.label.as_deref().unwrap_or("structure");
    
    // Build rows_data by prepending parent key to each child row
    // Format: [parent_key, child_col1, child_col2, ...]
    // This matches the regular AI call format - parent key is just another visible column
    let mut rows_data = Vec::new();
    for group in &parent_groups {
        let parent_key_value = &group.parent_key.key;
        for child_row in &group.rows {
            let mut full_row = vec![parent_key_value.clone()];
            full_row.extend(child_row.iter().cloned());
            rows_data.push(full_row);
        }
    }
    
    // Add parent key column context as first element
    let parent_key_context = parent_groups
        .first()
        .and_then(|g| g.parent_key.context.clone());
    
    let mut full_column_contexts = vec![parent_key_context];
    full_column_contexts.extend(column_contexts);
    
    let user_prompt = format!(
        "Fill in or correct the '{}' rows. The first column is the parent identifier - do not modify it. Fill in the remaining columns for each row.",
        structure_label
    );

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
                    &job_clone, &included_cols_clone, &row_partitions_clone).await;
                return;
            }
        };

        let (result, raw_response, updated_partitions) =
            python_executor::execute_python_ai_query(api_key_value, payload_json).await;

        let final_partitions = updated_partitions.unwrap_or_else(|| row_partitions_clone.clone());
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
                            row_partitions: final_partitions,
                            generation_id: job_clone.generation_id,
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
                        row_partitions: row_partitions_clone,
                        generation_id: job_clone.generation_id,
                    }),
                },
            },
        });
    }).await;
}
