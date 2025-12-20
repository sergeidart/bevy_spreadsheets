// Integration layer connecting the unified processor to Bevy ECS.
//
// This module provides a THIN BRIDGE between the standalone `Director` and
// the Bevy runtime (TokioTasksRuntime, EditorWindowState, events).
//
// ## Architecture
//
// Integration is ONLY responsible for:
// 1. Spawning async tasks (Bevy can't do async directly)
// 2. Transferring results to Bevy resources (EditorWindowState)
//
// All orchestration logic lives in Director:
// - `prepare_step()` - builds payload JSON
// - `complete_step()` - parses result, stores in Storager
//
// Integration NEVER:
// - Parses AI responses (Director does this)
// - Builds payload manually (Director does this)
// - Manages processing status (Director does this)

use bevy::prelude::*;
use bevy_tokio_tasks::TokioTasksRuntime;

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::ai_review::ai_context_utils::build_lineage_prefixes;
use crate::ui::elements::editor::state::{
    AiModeState, EditorWindowState, RowReview, NewRowReview,
    ReviewChoice as StateReviewChoice,
};
use crate::SessionApiKey;

use super::director::{ChildJobBuilder, Director, PendingJob, PreparedStep, ProcessedParentInfo};
use super::messenger::{Messenger, MessengerResult, RequestConfig};
use crate::sheets::column_validator::ColumnValidator;

/// Resource to track the Director session across frames.
/// 
/// The Director is initialized when a v2 processing session starts,
/// and drives the async steps via Bevy's background task system.
#[derive(Resource, Default)]
pub struct DirectorSession {
    /// The active Director, if any. None means no AI session is running.
    pub director: Option<Director>,
    /// Session generation ID (increments each new session)
    pub generation_id: u64,
    /// Whether a step is currently in progress (async call running)
    pub step_in_progress: bool,
    /// Pending job being processed
    pub pending_job: Option<PendingJob>,
    /// Prepared step data (for complete_step)
    pub prepared_step: Option<PreparedStep>,
    /// Callback entity for receiving async results
    pub callback_entity: Option<Entity>,
    /// Lineage prefix values (ancestor display values) - for root table in structure navigation
    pub lineage_prefix_values: Vec<String>,
    /// Lineage prefix contexts (AI context per ancestor) - for root table in structure navigation
    pub lineage_prefix_contexts: Vec<Option<String>>,
    /// Grid column indices that were included in AI request (for building non_structure_columns in reviews)
    pub included_indices: Vec<usize>,
    /// Root parent table name - when first step is a child table (from navigation)
    pub root_parent_table_name: Option<String>,
    /// Root parent stable index - when first step is a child table (from navigation)
    pub root_parent_stable_index: Option<usize>,
}

impl DirectorSession {
    /// Start a new session with an initial job
    pub fn start(&mut self, root_job: PendingJob) {
        self.generation_id += 1;
        let mut director = Director::new();
        director.start_session(self.generation_id, root_job);
        self.director = Some(director);
        self.step_in_progress = false;
        self.pending_job = None;
        self.prepared_step = None;
        self.callback_entity = None;
    }

    /// Clear the session
    pub fn clear(&mut self) {
        self.director = None;
        self.step_in_progress = false;
        self.pending_job = None;
        self.prepared_step = None;
        self.callback_entity = None;
        self.lineage_prefix_values.clear();
        self.lineage_prefix_contexts.clear();
        self.included_indices.clear();
        self.root_parent_table_name = None;
        self.root_parent_stable_index = None;
    }
}

/// Marker component for async AI result callback.
/// 
/// Contains the raw MessengerResult from the Python call.
/// Director.complete_step() will parse and process this.
#[derive(Component)]
pub struct DirectorStepCallback {
    /// MessengerResult from the async Python call
    pub messenger_result: MessengerResult,
}

// ============================================================================
// V2 Entry Point and Wiring
// ============================================================================

/// Start a new Director-based AI processing session.
/// 
/// This is the v2 replacement for `send_selected_rows()`. It:
/// 1. Creates a Director with the root job
/// 2. Dispatches the first async step
/// 3. Returns immediately (processing continues via `poll_director_results`)
#[allow(clippy::too_many_arguments)]
pub fn start_director_session_v2(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    api_key: &SessionApiKey,
    runtime: &TokioTasksRuntime,
    commands: &mut Commands,
    session: &mut DirectorSession,
    user_prompt: Option<String>,
) {
    // Extract selected rows
    let selection: Vec<usize> = state.ai_selected_rows.iter().copied().collect();
    if selection.is_empty() && user_prompt.is_none() {
        return;
    }

    let category = state.selected_category.clone();
    let sheet_name = match &state.selected_sheet_name {
        Some(name) => name.clone(),
        None => return,
    };

    // Validate sheet exists with metadata
    let sheet_opt = registry.get_sheet(&category, &sheet_name);
    if sheet_opt.and_then(|s| s.metadata.as_ref()).is_none() {
        warn!("No metadata for sheet '{}', cannot start Director session", sheet_name);
        return;
    }

    // Validate API key
    let api_key_str = match &api_key.0 {
        Some(k) if !k.is_empty() => k.clone(),
        _ => {
            state.ai_raw_output_display = "API Key not set".to_string();
            return;
        }
    };

    // Create root job
    let root_job = PendingJob::root(sheet_name.clone(), category.clone(), selection.clone());

    // Build lineage prefixes from structure navigation stack (for when user navigated into a child table)
    // This provides ancestor context to the AI for the root table of this session
    let lineage_prefixes = build_lineage_prefixes(state, registry, &selection);
    
    // Store lineage prefix pairs for review UI
    if !lineage_prefixes.prefix_values.is_empty() {
        state.ai_context_prefix_by_row = lineage_prefixes.prefix_pairs_by_row.clone();
    }

    // Update UI state
    state.ai_mode = AiModeState::Submitting;
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    state.ai_last_send_root_rows = selection;
    state.ai_last_send_root_category = category.clone();
    state.ai_last_send_root_sheet = Some(sheet_name);

    // Start the session with lineage info
    session.start(root_job.clone());
    session.lineage_prefix_values = lineage_prefixes.prefix_values;
    session.lineage_prefix_contexts = lineage_prefixes.prefix_contexts;
    
    // Extract immediate parent info from navigation context (for first step as child table)
    if let Some(nav_ctx) = state.structure_navigation_stack.last() {
        session.root_parent_table_name = Some(nav_ctx.parent_sheet_name.clone());
        if let Ok(parent_idx) = nav_ctx.parent_row_key.parse::<usize>() {
            session.root_parent_stable_index = Some(parent_idx);
        }
    }

    // Dispatch first step
    dispatch_next_step(
        session,
        state,
        registry,
        &api_key_str,
        runtime,
        commands,
        user_prompt,
    );
}

/// Dispatch the next processing step (internal).
/// 
/// This is a THIN wrapper that:
/// 1. Calls Director.prepare_step() to get payload
/// 2. Spawns async Python call (Messenger.execute())
/// 3. Returns immediately - poll_director_results handles completion
fn dispatch_next_step(
    session: &mut DirectorSession,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    api_key: &str,
    runtime: &TokioTasksRuntime,
    commands: &mut Commands,
    user_prompt: Option<String>,
) {
    let director = match session.director.as_mut() {
        Some(d) => d,
        None => return,
    };

    // Don't dispatch if Director is already finished (Complete, Error, or Cancelled)
    if director.state().status.is_finished() {
        info!("Director already finished with status {:?}, skipping dispatch", director.state().status);
        return;
    }

    // Take next job from queue
    let job = match director.take_next_job() {
        Some(j) => j,
        None => {
            session.step_in_progress = false;
            return;
        }
    };

    // Get sheet data for Director.prepare_step()
    let sheet = match registry.get_sheet(&job.category, &job.table_name) {
        Some(s) => s,
        None => {
            warn!("Sheet '{}' not found", job.table_name);
            dispatch_next_step(session, state, registry, api_key, runtime, commands, user_prompt);
            return;
        }
    };

    let meta = match &sheet.metadata {
        Some(m) => m,
        None => {
            warn!("No metadata for '{}'", job.table_name);
            dispatch_next_step(session, state, registry, api_key, runtime, commands, user_prompt);
            return;
        }
    };

    // Build request config (this is just metadata extraction, OK in integration)
    let mut config = match build_request_config(meta, user_prompt.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to build config for '{}': {}", job.table_name, e);
            dispatch_next_step(session, state, registry, api_key, runtime, commands, user_prompt);
            return;
        }
    };

    // For first step only: add lineage prefix from structure navigation stack
    // Child table steps get their lineage from Director's genealogist instead
    if job.is_first_step() && !session.lineage_prefix_values.is_empty() {
        config.lineage_prefix_values = session.lineage_prefix_values.clone();
        config.lineage_prefix_contexts = session.lineage_prefix_contexts.clone();
    }
    
    // Pass root parent info for first step child tables
    if job.is_first_step() {
        config.root_parent_table_name = session.root_parent_table_name.clone();
        config.root_parent_stable_index = session.root_parent_stable_index;
    }

    // Get grid data
    let grid: Vec<Vec<String>> = sheet.grid.iter().map(|row| row.to_vec()).collect();
    let row_indices: Vec<i64> = sheet.row_indices.clone();

    // Director prepares the step (builds payload, registers with Navigator)
    let prepared = match director.prepare_step(&job, &grid, &row_indices, registry, config) {
        Ok(p) => p,
        Err(e) => {
            warn!("Director.prepare_step failed for '{}': {}", job.table_name, e);
            dispatch_next_step(session, state, registry, api_key, runtime, commands, user_prompt);
            return;
        }
    };

    // Log request to AI call log for debugging
    let total_rows: usize = prepared.batches.iter().map(|b| b.rows.len()).sum();
    let status = format!(
        "Sending AI request for '{}' ({} rows)...",
        job.table_name,
        total_rows
    );
    // Pretty-print payload for log display
    if let Ok(pretty_payload) = serde_json::from_str::<serde_json::Value>(&prepared.payload_json)
        .and_then(|v| serde_json::to_string_pretty(&v))
    {
        state.add_ai_call_log(status, None, Some(pretty_payload), false);
    } else {
        state.add_ai_call_log(status, None, Some(prepared.payload_json.clone()), false);
    }

    // Store state for complete_step
    session.pending_job = Some(job);
    session.prepared_step = Some(prepared.clone());
    session.step_in_progress = true;

    // Spawn async Python call
    let callback_entity = commands.spawn_empty().id();
    session.callback_entity = Some(callback_entity);

    let api_key_owned = api_key.to_string();
    let payload_json = prepared.payload_json;

    // Use Messenger.execute() for the async call
    runtime.spawn_background_task(move |mut ctx| async move {
        // Ensure Python script exists
        Messenger::ensure_python_script();
        
        // Create messenger and execute
        let messenger = Messenger::new();
        let result = messenger.execute(api_key_owned, payload_json).await;
        
        ctx.run_on_main_thread(move |world_ctx| {
            world_ctx
                .world
                .commands()
                .entity(callback_entity)
                .insert(DirectorStepCallback {
                    messenger_result: result,
                });
        })
        .await;
    });
}

/// Build request configuration from metadata.
/// 
/// Uses the same column filtering logic as the existing single-step AI system
/// via `collect_ai_included_columns` to ensure compatibility.
fn build_request_config(
    meta: &crate::sheets::sheet_metadata::SheetMetadata,
    _user_prompt: Option<&str>,
) -> Result<RequestConfig, String> {
    use crate::sheets::definitions::default_ai_model_id;
    use crate::ui::elements::ai_review::ai_context_utils::collect_ai_included_columns;

    let model_id = if meta.ai_model_id.is_empty() {
        default_ai_model_id()
    } else {
        meta.ai_model_id.clone()
    };

    // Use the same column inclusion logic as the working single-step system
    let is_child_table = meta.is_structure_table();
    let inclusion = collect_ai_included_columns(meta, is_child_table);
    
    // Build column names from included indices for response parsing
    let column_names: Vec<String> = inclusion.included_indices.iter()
        .filter_map(|&idx| meta.columns.get(idx).map(|c| c.header.clone()))
        .collect();

    Ok(RequestConfig {
        included_indices: inclusion.included_indices,
        column_names,
        column_contexts: inclusion.column_contexts,
        ai_context: meta.ai_general_rule.clone(),
        model_id,
        allow_row_generation: meta.ai_enable_row_generation,
        grounding_with_google_search: meta.requested_grounding_with_google_search.unwrap_or(false),
        lineage_prefix_values: Vec::new(),
        lineage_prefix_contexts: Vec::new(),
        prefix_column_names: Vec::new(), // Will be populated in Director.prepare_step from ancestry
        is_child_table,
        root_parent_table_name: None,
        root_parent_stable_index: None,
    })
}

/// Bevy system to poll for async Director results.
/// 
/// Add to Update schedule to drive processing forward.
/// This is a THIN wrapper that delegates to Director.complete_step().
pub fn poll_director_results(
    mut session: ResMut<DirectorSession>,
    mut state: ResMut<EditorWindowState>,
    registry: Res<SheetRegistry>,
    api_key: Res<SessionApiKey>,
    runtime: Res<TokioTasksRuntime>,
    mut commands: Commands,
    query: Query<(Entity, &DirectorStepCallback)>,
) {
    // Only process if we have a pending step
    if !session.step_in_progress {
        return;
    }

    // Check if Director is in an active processing state
    // If finished (Complete/Error/Cancelled), skip polling
    if let Some(director) = session.director.as_ref() {
        if director.state().status.is_finished() {
            session.step_in_progress = false;
            return;
        }
        // Verify we're in an active state (Preparing, SendingRequest, etc.)
        if !director.state().status.is_active() {
            // In Idle state with step_in_progress true is inconsistent - reset
            session.step_in_progress = false;
            return;
        }
    }

    let callback_entity = match session.callback_entity {
        Some(e) => e,
        None => return,
    };

    // Check if callback result is ready
    let callback_result = query.iter().find(|(e, _)| *e == callback_entity);
    
    let (entity, callback) = match callback_result {
        Some(r) => r,
        None => return, // Not ready yet
    };

    // Extract data before mutable borrow
    let job = session.pending_job.take();
    let prepared = session.prepared_step.take();
    session.step_in_progress = false;
    session.callback_entity = None;

    // Call Director.complete_step() to process the result
    // Then detect and queue child jobs for Structure columns
    if let (Some(job), Some(prepared)) = (job, prepared) {
        process_step_with_director(&mut session, &mut state, &registry, &job, &prepared, &callback.messenger_result);
        
        // After processing, detect Structure columns and queue child table jobs
        detect_and_queue_child_jobs(&mut session, &registry, &job);
    }

    // Clean up callback entity
    commands.entity(entity).despawn();

    // Continue to next step or complete
    let should_continue = if let Some(director) = session.director.as_ref() {
        let status = director.state().status;
        // Stop on error
        if status == crate::sheets::systems::ai::processor::director::ProcessingStatus::Error {
            false
        } else {
            director.has_pending_jobs()
        }
    } else {
        false
    };

    if should_continue {
        let api_key_str = api_key.0.clone().unwrap_or_default();
        dispatch_next_step(
            &mut session,
            &mut state,
            &registry,
            &api_key_str,
            &runtime,
            &mut commands,
            None,
        );
    } else {
        // Complete session (or stop on error)
        complete_session(&mut session, &mut state);
    }
}
/// Process step result using Director.complete_step().
/// 
/// This delegates all parsing and storage to Director, then transfers
/// results to EditorWindowState for display.
fn process_step_with_director(
    session: &mut DirectorSession,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    job: &PendingJob,
    prepared: &PreparedStep,
    messenger_result: &MessengerResult,
) {
    let director = match session.director.as_mut() {
        Some(d) => d,
        None => return,
    };

    // Director handles parsing, Navigator registration, and storage
    let step_result = director.complete_step(job, prepared, messenger_result.clone(), registry);

    // Store raw response from StepResult for display
    if let Some(raw) = &step_result.raw_response {
        state.ai_raw_output_display = raw.clone();
    }

    if step_result.success {
        // Log with progress info
        let proc_state = director.state();
        info!(
            "Director.complete_step '{}' [{} ({:.0}%)]: {} rows, {} AI-added, {} lost",
            job.table_name,
            proc_state.progress_string(),
            proc_state.progress() * 100.0,
            step_result.rows_processed,
            step_result.ai_added_count,
            step_result.lost_count
        );

        // Add to AI call log with response
        let status = format!(
            "Step {}: Received '{}': {} rows, {} AI-added, {} lost",
            proc_state.progress_string(),
            job.table_name,
            step_result.rows_processed,
            step_result.ai_added_count,
            step_result.lost_count
        );
        state.add_ai_call_log(
            status,
            step_result.raw_response.clone(),
            None,
            false,
        );

        // Transfer results from Director's storage to EditorWindowState
        // For now, we'll extract from storage in complete_session
        // The key results are in director.storage()
    } else {
        // Log error with status message
        let status_msg = director.state().status_message();
        warn!(
            "Director.complete_step failed for '{}': {} - {:?}. Raw response: {:?}",
            job.table_name,
            status_msg,
            step_result.error,
            step_result.raw_response
        );
        
        // Add error to AI call log
        let error_msg = step_result.error.clone().unwrap_or_else(|| "Unknown error".to_string());
        state.add_ai_call_log(
            format!("Error for '{}': {}", job.table_name, error_msg),
            step_result.raw_response.clone(),
            None,
            true,
        );
        
        if let Some(err) = &step_result.error {
            state.ai_raw_output_display = format!("Error: {}", err);
        }
    }
}

/// Detect Structure columns in a processed table and queue child jobs.
/// 
/// This enables multi-step processing: after processing a parent table,
/// we detect which Structure columns exist and queue jobs for their child tables.
/// 
/// Child table naming convention: `{ParentSheet}_{ColumnHeader}`
/// Child table column 1 is always `parent_key` pointing to parent's row_index.
fn detect_and_queue_child_jobs(
    session: &mut DirectorSession,
    registry: &SheetRegistry,
    job: &PendingJob,
) {
    let director = match session.director.as_mut() {
        Some(d) => d,
        None => return,
    };

    // Get parent sheet metadata
    let parent_sheet = match registry.get_sheet(&job.category, &job.table_name) {
        Some(s) => s,
        None => return,
    };

    let parent_meta = match &parent_sheet.metadata {
        Some(m) => m,
        None => return,
    };

    // Build ChildJobBuilder with Structure columns
    let mut builder = ChildJobBuilder::new(job.table_name.clone(), job.category.clone());

    for (idx, col) in parent_meta.columns.iter().enumerate() {
        if matches!(col.validator, Some(ColumnValidator::Structure)) {
            let ai_include = col.ai_include_in_send.unwrap_or(false);
            builder.add_structure_column(idx, col.header.clone(), ai_include);
        }
    }

    // No structure columns? Nothing to do
    if builder.included_columns().is_empty() {
        return;
    }

    // Get ALL processed rows (original + AI added) from Director storage
    // This ensures we generate child jobs for AI-added rows too.
    // Also extract AI column values for propagating content to child steps.
    let processed_parents: Vec<ProcessedParentInfo> = director.storage()
        .get_results_for_table(&job.table_name, job.category.as_deref(), &job.step_path)
        .map(|results| results.iter().map(|r| {
            // Build ai_column_values map from stored columns
            let ai_column_values: std::collections::HashMap<usize, String> = r.columns
                .iter()
                .map(|(&col_idx, col_result)| (col_idx, col_result.ai_value.clone()))
                .collect();
            
            ProcessedParentInfo {
                stable_index: r.stable_id.stable_index,
                display_value: r.stable_id.display_value.clone(),
                is_ai_added: r.stable_id.is_ai_added(),
                ai_column_values,
            }
        }).collect())
        .unwrap_or_default();

    if processed_parents.is_empty() {
        return;
    }

    // Build child_row_map: (parent_stable_index, metadata_col_index) -> child grid row indices
    // For AI-added parents, no DB children exist, so their entries will be empty
    let mut child_row_map = std::collections::HashMap::new();

    for col_info in builder.included_columns() {
        // Child table name: {ParentSheet}_{ColumnHeader}
        let child_table_name = format!("{}_{}", job.table_name, col_info.column_header);

        let child_sheet = match registry.get_sheet(&job.category, &child_table_name) {
            Some(s) => s,
            None => {
                info!("Child table '{}' not found, skipping", child_table_name);
                continue;
            }
        };

        // For each processed parent row, find matching child rows
        // Only Original parents can have DB children (via parent_key column)
        // AI-added parents have no children in DB yet
        for parent_info in &processed_parents {
            // Skip DB lookup for AI-added parents (they have no DB row_index)
            if parent_info.is_ai_added {
                // AI-added parents get empty target_rows - AI will generate their children
                continue;
            }
            
            // Original parents: look up children by parent_key
            let parent_key_str = parent_info.stable_index.to_string();

            let child_rows: Vec<usize> = child_sheet.grid
                .iter()
                .enumerate()
                .filter(|(_, row)| {
                    row.get(1).map(|v| v == &parent_key_str).unwrap_or(false)
                })
                .map(|(grid_idx, _)| grid_idx)
                .collect();

            if !child_rows.is_empty() {
                child_row_map.insert(
                    (parent_info.stable_index, col_info.metadata_column_index),
                    child_rows,
                );
            }
        }
    }

    // Build child jobs - includes both Original and AI-added parents
    let child_jobs = builder.build_child_jobs(&processed_parents, &child_row_map);

    if !child_jobs.is_empty() {
        info!(
            "Queuing {} child jobs for parent '{}' ({} original, {} AI-added parents)",
            child_jobs.len(),
            job.table_name,
            processed_parents.iter().filter(|p| !p.is_ai_added).count(),
            processed_parents.iter().filter(|p| p.is_ai_added).count(),
        );
        for parent in &processed_parents {
            info!(
                "  Parent: stable_idx={}, display='{}', is_ai_added={}",
                parent.stable_index, parent.display_value, parent.is_ai_added
            );
        }
        director.queue_jobs(child_jobs);
    }
}

/// Complete the session and transfer results from Director to EditorWindowState.
fn complete_session(
    session: &mut DirectorSession,
    state: &mut EditorWindowState,
) {
    use crate::ui::elements::editor::state::StructureReviewEntry;
    use std::collections::HashMap;
    
    // Transfer results from Storager to EditorWindowState
    // Storager organizes results by TableKey (table_name, category, step_path)
    if let Some(director) = session.director.as_ref() {
        let storage = director.storage();
        let navigator = director.navigator();
        
        // Debug: log all table keys in storage
        info!("complete_session: Storager contents:");
        for (table_key, results) in storage.iter_by_table() {
            info!(
                "  TableKey: table='{}' category={:?} step_path={:?} -> {} results",
                table_key.table_name,
                table_key.category,
                table_key.step_path,
                results.len()
            );
            for (i, r) in results.iter().enumerate() {
                info!(
                    "    [{}] stable_idx={} parent_table={:?} parent_idx={:?} category={:?}",
                    i,
                    r.stable_id().stable_index,
                    r.stable_id().parent_table_name,
                    r.stable_id().parent_stable_index,
                    r.category()
                );
            }
        }
        
        // Group structure results by (parent_table_name, parent_stable_index, step_path)
        // This handles arbitrary nesting: step_path=[3] is level 1, [3,2] is level 2, etc.
        let mut structure_groups: HashMap<
            (String, usize, Vec<usize>), // (parent_table, parent_idx, step_path)
            Vec<&super::storager::StoredRowResult>
        > = HashMap::new();
        
        // Collect orphaned child rows per step_path for duplication into all StructureReviewEntry instances
        // Key: (child_table_name, step_path), Value: Vec<(ai_row_values, claimed_ancestry)>
        let mut orphaned_by_child_table: HashMap<
            (String, Vec<usize>),
            Vec<(Vec<String>, Vec<String>)>
        > = HashMap::new();
        
        // Iterate by table to properly separate root vs child step results
        for (table_key, results) in storage.iter_by_table() {
            if table_key.step_path.is_empty() {
                // Root step results - create flat RowReview/NewRowReview
                for result in results {
                    build_flat_review(result, session, state);
                }
            } else {
                // Child step results - group for StructureReviewEntry creation
                for result in results {
                    // Check if this is an orphaned row (no valid parent)
                    if result.category() == super::storager::RowCategory::Orphaned {
                        // Collect orphaned rows separately - they'll be duplicated into all entries
                        let ai_values: Vec<String> = result.columns()
                            .iter()
                            .map(|c| c.ai_value.clone())
                            .collect();
                        let claimed_ancestry = result.claimed_ancestry.clone();
                        
                        let key = (table_key.table_name.clone(), table_key.step_path.clone());
                        orphaned_by_child_table.entry(key).or_default().push((ai_values, claimed_ancestry));
                        continue;
                    }
                    
                    let parent_table = result.stable_id().parent_table_name.clone()
                        .unwrap_or_else(|| table_key.table_name.clone());
                    let parent_idx = result.stable_id().parent_stable_index
                        .unwrap_or(0);
                    
                    let key = (parent_table, parent_idx, table_key.step_path.clone());
                    structure_groups.entry(key).or_default().push(result);
                }
            }
        }
        
        // Create StructureReviewEntry for each group of child step results
        let root_category = state.ai_last_send_root_category.clone();
        
        for ((parent_table, parent_stable_idx, step_path), child_results) in &structure_groups {
            let parent_stable_idx = *parent_stable_idx;
            // Determine parent_row_index vs parent_new_row_index
            // Look up parent in Navigator to determine if it's original or AI-added
            let is_original_parent = navigator
                .get(parent_table, root_category.as_deref(), parent_stable_idx)
                .map(|id| id.is_original())
                .unwrap_or(true); // Default to original if not found
            
            let (parent_row_index, parent_new_row_index) = if is_original_parent {
                // For original parents: parent_row_index = stable_index = db_row_index
                (parent_stable_idx, None)
            } else {
                // For AI-added parents: need to find the array index in ai_new_row_reviews
                // where projected_row_index == parent_stable_idx
                let array_idx = state.ai_new_row_reviews
                    .iter()
                    .position(|nr| nr.projected_row_index == parent_stable_idx);
                
                if let Some(idx) = array_idx {
                    (0, Some(idx))
                } else {
                    // Fallback: parent not found in reviews, use stable_idx as row_index
                    warn!(
                        "AI-added parent with stable_idx={} not found in ai_new_row_reviews, using as row_index",
                        parent_stable_idx
                    );
                    (parent_stable_idx, None)
                }
            };
            
            // Build AI rows from child results (skip Lost rows)
            let ai_rows: Vec<Vec<String>> = child_results
                .iter()
                .filter(|r| r.category() != super::storager::RowCategory::Lost)
                .map(|r| {
                    r.columns().iter().map(|c| c.ai_value.clone()).collect()
                })
                .collect();
            
            // Schema headers from column names
            let schema_headers: Vec<String> = child_results
                .first()
                .map(|r| r.columns().iter().map(|c| c.column_name.clone()).collect())
                .unwrap_or_default();
            
            let merged_rows = ai_rows.clone();
            let differences: Vec<Vec<bool>> = ai_rows
                .iter()
                .map(|row| row.iter().map(|_| true).collect())
                .collect();
            
            let has_changes = !ai_rows.is_empty();
            let ai_rows_len = ai_rows.len();
            
            state.ai_structure_reviews.push(StructureReviewEntry {
                root_category: root_category.clone(),
                root_sheet: parent_table.clone(),
                parent_row_index,
                parent_new_row_index,
                structure_path: step_path.clone(),
                has_changes,
                accepted: false,
                rejected: false,
                decided: false,
                original_rows: Vec::new(),
                ai_rows,
                merged_rows,
                differences,
                schema_headers,
                original_rows_count: 0,
                orphaned_ai_rows: Vec::new(),
                orphaned_claimed_ancestries: Vec::new(),
                orphaned_decided: Vec::new(),
            });
            
            info!(
                "Created StructureReviewEntry: parent='{}' stable_idx={} row_idx={} new_row_idx={:?} path={:?} rows={}",
                parent_table, parent_stable_idx, parent_row_index, parent_new_row_index, step_path, ai_rows_len
            );
        }
        
        // Create empty StructureReviewEntry for parents that were processed but got 0 results
        // This ensures clicking structure buttons on AI-added rows shows "(empty)" instead of database fallback
        for (table_key, _results) in storage.iter_by_table() {
            if table_key.step_path.is_empty() {
                continue; // Skip root step
            }
            
            // Get processed parents for this child step
            if let Some(processed_parents) = storage.get_processed_parents(
                &table_key.table_name,
                table_key.category.as_deref(),
                &table_key.step_path,
            ) {
                for pp in processed_parents {
                    // Check if we already created an entry for this parent
                    let already_exists = structure_groups.contains_key(&(
                        pp.parent_table.clone(),
                        pp.parent_stable_index,
                        table_key.step_path.clone(),
                    ));
                    
                    if already_exists {
                        continue; // Entry already created from actual results
                    }
                    
                    // Create empty entry for this parent
                    let (parent_row_index, parent_new_row_index) = if !pp.is_ai_added {
                        (pp.parent_stable_index, None)
                    } else {
                        let array_idx = state.ai_new_row_reviews
                            .iter()
                            .position(|nr| nr.projected_row_index == pp.parent_stable_index);
                        
                        if let Some(idx) = array_idx {
                            (0, Some(idx))
                        } else {
                            warn!(
                                "AI-added parent with stable_idx={} not found in ai_new_row_reviews for empty entry",
                                pp.parent_stable_index
                            );
                            continue; // Skip if we can't find the parent
                        }
                    };
                    
                    state.ai_structure_reviews.push(StructureReviewEntry {
                        root_category: root_category.clone(),
                        root_sheet: pp.parent_table.clone(),
                        parent_row_index,
                        parent_new_row_index,
                        structure_path: table_key.step_path.clone(),
                        has_changes: false,
                        accepted: false,
                        rejected: false,
                        decided: false,
                        original_rows: Vec::new(),
                        ai_rows: Vec::new(),
                        merged_rows: Vec::new(),
                        differences: Vec::new(),
                        schema_headers: Vec::new(),
                        original_rows_count: 0,
                        orphaned_ai_rows: Vec::new(),
                        orphaned_claimed_ancestries: Vec::new(),
                        orphaned_decided: Vec::new(),
                    });
                    
                    info!(
                        "Created empty StructureReviewEntry: parent='{}' stable_idx={} row_idx={} new_row_idx={:?} path={:?}",
                        pp.parent_table, pp.parent_stable_index, parent_row_index, parent_new_row_index, table_key.step_path
                    );
                }
            }
        }
        
        // Duplicate orphaned child rows into ALL StructureReviewEntry instances for that child table
        // This ensures orphans are visible from any parent's drill-down view
        for ((child_table, step_path), orphans) in &orphaned_by_child_table {
            if orphans.is_empty() {
                continue;
            }
            
            info!(
                "Distributing {} orphaned rows to all StructureReviewEntry instances for table='{}' path={:?}",
                orphans.len(), child_table, step_path
            );
            
            // Find all StructureReviewEntry instances that match this child table/step_path
            for entry in state.ai_structure_reviews.iter_mut() {
                if &entry.structure_path == step_path {
                    // Add orphans to this entry
                    for (ai_values, claimed_ancestry) in orphans {
                        entry.orphaned_ai_rows.push(ai_values.clone());
                        entry.orphaned_claimed_ancestries.push(claimed_ancestry.clone());
                        entry.orphaned_decided.push(false); // Not yet decided
                    }
                    
                    // Mark as having changes if there are orphans
                    if !orphans.is_empty() {
                        entry.has_changes = true;
                    }
                }
            }
        }
    }
    
    state.ai_mode = AiModeState::ResultsReady;
    
    // Log completion
    if let Some(director) = session.director.as_ref() {
        let proc_state = director.state();
        info!(
            "Session complete (gen {}): {} - {} reviews, {} new rows, {} structure reviews",
            proc_state.generation_id,
            proc_state.status_message(),
            state.ai_row_reviews.len(),
            state.ai_new_row_reviews.len(),
            state.ai_structure_reviews.len()
        );
    }

    session.clear();
}

/// Build a flat RowReview or NewRowReview from a root step result
fn build_flat_review(
    result: &super::storager::StoredRowResult,
    session: &DirectorSession,
    state: &mut EditorWindowState,
) {
    match result.category() {
        super::storager::RowCategory::Original => {
            let ai_values: Vec<String> = result.columns()
                .iter()
                .map(|c| c.ai_value.clone())
                .collect();
            let original_values: Vec<String> = result.columns()
                .iter()
                .map(|c| c.original_value.clone())
                .collect();
            
            let choices = generate_choices_for_review(&original_values, &ai_values);
            let non_structure_cols: Vec<usize> = result.columns()
                .iter()
                .map(|c| c.column_index)
                .collect();
            
            let row_index = result.stable_id().original_row_index().unwrap_or(0);
            let ancestor_key_values = if !session.lineage_prefix_values.is_empty() {
                session.lineage_prefix_values.clone()
            } else {
                Vec::new()
            };
            
            state.ai_row_reviews.push(RowReview {
                row_index,
                original: original_values,
                ai: ai_values,
                choices,
                non_structure_columns: non_structure_cols,
                key_overrides: std::collections::HashMap::new(),
                ancestor_key_values,
                ancestor_dropdown_cache: std::collections::HashMap::new(),
                is_orphan: false,
            });
        }
        super::storager::RowCategory::AiAdded => {
            let ai_values: Vec<String> = result.columns()
                .iter()
                .map(|c| c.ai_value.clone())
                .collect();
            let non_structure_cols: Vec<usize> = result.columns()
                .iter()
                .map(|c| c.column_index)
                .collect();
            
            let projected_index = result.stable_id().ai_sequence().unwrap_or(0);
            let mut ancestor_dropdown_cache = std::collections::HashMap::new();
            
            let ancestor_key_values = if !session.lineage_prefix_values.is_empty() {
                session.lineage_prefix_values.clone()
            } else {
                Vec::new()
            };
            
            if !result.parent_valid {
                ancestor_dropdown_cache.insert(0, (vec![], result.parent_suggestions.clone()));
            }
            
            state.ai_new_row_reviews.push(NewRowReview {
                ai: ai_values,
                non_structure_columns: non_structure_cols,
                duplicate_match_row: None,
                choices: None,
                merge_selected: false,
                merge_decided: false,
                original_for_merge: None,
                key_overrides: std::collections::HashMap::new(),
                ancestor_key_values,
                ancestor_dropdown_cache,
                projected_row_index: projected_index,
                is_orphan: false,
            });
        }
        super::storager::RowCategory::Lost => {
            warn!("Lost row: {:?}", result.stable_id());
        }
        super::storager::RowCategory::Orphaned => {
            // Orphaned rows are AI-added rows with unmatched parent prefixes
            // They need to be displayed for re-parenting via drag-drop/dropdown
            // We show them with their claimed_ancestry and is_orphan: true for red rendering
            let ai_values: Vec<String> = result.columns()
                .iter()
                .map(|c| c.ai_value.clone())
                .collect();
            let non_structure_cols: Vec<usize> = result.columns()
                .iter()
                .map(|c| c.column_index)
                .collect();
            
            let projected_index = result.stable_id().ai_sequence().unwrap_or(0);
            
            // Use claimed_ancestry as the ancestor values (they'll be rendered in red)
            let ancestor_key_values = result.claimed_ancestry.clone();
            
            warn!(
                "Creating orphaned NewRowReview: display_value='{}', claimed_ancestry={:?}",
                &result.stable_id.display_value,
                ancestor_key_values
            );
            
            state.ai_new_row_reviews.push(NewRowReview {
                ai: ai_values,
                non_structure_columns: non_structure_cols,
                duplicate_match_row: None,
                choices: None,
                merge_selected: false,
                merge_decided: false,
                original_for_merge: None,
                key_overrides: std::collections::HashMap::new(),
                ancestor_key_values,
                ancestor_dropdown_cache: std::collections::HashMap::new(),
                projected_row_index: projected_index,
                is_orphan: true, // Mark as orphan for red ancestor rendering
            });
        }
    }
}

/// Generate review choices comparing original and AI values.
fn generate_choices_for_review(original: &[String], ai: &[String]) -> Vec<StateReviewChoice> {
    original
        .iter()
        .zip(ai.iter())
        .map(|(orig, ai_val)| {
            if orig == ai_val {
                StateReviewChoice::Original
            } else if orig.is_empty() && !ai_val.is_empty() {
                StateReviewChoice::AI
            } else {
                StateReviewChoice::Original
            }
        })
        .collect()
}

/// Cancel the current AI processing session.
/// 
/// This properly cancels the Director session and resets UI state.
pub fn cancel_director_session(
    session: &mut DirectorSession,
    state: &mut EditorWindowState,
) {
    if let Some(director) = session.director.as_mut() {
        director.cancel();
    }
    state.ai_mode = AiModeState::Idle;
    state.ai_raw_output_display = "Cancelled".to_string();
    session.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_director_session_default() {
        let session = DirectorSession::default();
        assert!(session.director.is_none());
        assert_eq!(session.generation_id, 0);
    }

    #[test]
    fn test_director_session_start() {
        let mut session = DirectorSession::default();
        let job = PendingJob::root("TestTable".to_string(), Some("cat".to_string()), vec![0, 1, 2]);
        
        session.start(job);
        
        assert!(session.director.is_some());
        assert_eq!(session.generation_id, 1);
        assert!(!session.step_in_progress);
    }

    #[test]
    fn test_director_session_clear() {
        let mut session = DirectorSession::default();
        let job = PendingJob::root("TestTable".to_string(), None, vec![0]);
        session.start(job);
        assert!(session.director.is_some());

        session.clear();
        
        assert!(session.director.is_none());
    }

    #[test]
    fn test_director_cancel() {
        use super::super::director::ProcessingStatus;
        
        let mut session = DirectorSession::default();
        let job = PendingJob::root("TestTable".to_string(), None, vec![0, 1]);
        session.start(job);
        
        // Director should have pending job
        assert!(session.director.as_ref().unwrap().has_pending_jobs());
        
        // Cancel via director
        session.director.as_mut().unwrap().cancel();
        
        // Director should be cancelled
        assert_eq!(session.director.as_ref().unwrap().state().status, ProcessingStatus::Cancelled);
        assert!(!session.director.as_ref().unwrap().has_pending_jobs());
    }
}
