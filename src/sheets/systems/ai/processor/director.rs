// src/sheets/systems/ai/processor/director.rs
//! Director - Flow Orchestration
//!
//! This module orchestrates the complete AI processing flow.
//! It manages step progression and coordinates all components.
//!
//! ## Responsibilities
//!
//! - Manage step progression (root → structure children)
//! - Coordinate all processor components
//! - Track progress (current step, remaining)
//! - Handle cancel/complete
//! - Queue child table jobs for multi-step processing
//!
//! ## Flow Per Step
//!
//! 1. Pre-Processor: Prepare data, register original indexes
//! 2. Genealogist: Build ancestry (structure tables only)
//! 3. Messenger: Send AI request
//! 4. Parser: Parse response, categorize rows
//! 5. Navigator: Assign indexes to AI-added rows
//! 6. Storager: Persist results with stable IDs
//! 7. If more children → queue next step, goto 1
//! 8. If done → emit ReviewReady

use std::collections::{VecDeque, HashMap};

use super::genealogist::{Ancestry, Genealogist};
use super::messenger::{Messenger, MessengerResult, RequestConfig};
use super::navigator::IndexMapper;
use super::parser::{ParseResult, ParsedRow, ResponseParser};
use super::pre_processor::{PreProcessConfig, PreProcessor, PreparedBatch};
use super::storager::{ColumnResult, ResultStorage, StoredRowResult};
use crate::sheets::resources::SheetRegistry;

/// Status of the processing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingStatus {
    /// Not started yet
    Idle,
    /// Preparing data for AI call
    Preparing,
    /// Sending request to AI
    SendingRequest,
    /// Parsing AI response
    ParsingResponse,
    /// Storing results
    StoringResults,
    /// Queuing child table jobs
    QueueingChildren,
    /// All steps complete, ready for review
    Complete,
    /// Error occurred
    Error,
    /// Cancelled by user
    Cancelled,
}

impl ProcessingStatus {
    /// Check if processing is still active
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::SendingRequest
                | Self::ParsingResponse
                | Self::StoringResults
                | Self::QueueingChildren
        )
    }

    /// Check if processing is finished (complete or error)
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Complete | Self::Error | Self::Cancelled)
    }
}

/// A context for a single parent in a batched job
#[derive(Debug, Clone)]
pub struct ParentContext {
    /// Stable index of the parent row (DB row_index for Original, Navigator index for AI-added)
    pub parent_stable_index: usize,
    /// Table name of the parent
    pub parent_table_name: String,
    /// Target child rows in the database grid (empty for AI-added parents)
    pub target_rows: Vec<usize>,
    /// Display value of the parent (for AI-added parents, used for ancestry building)
    pub parent_display_value: String,
    /// Whether this parent is an AI-added row (not yet in database)
    pub is_ai_added: bool,
    /// AI-generated column values from parent step (for propagating content to child steps)
    /// Maps column_index -> ai_value. Only populated for AI-added parents.
    /// Reserved for future use when we need to pass specific column values beyond display_value.
    #[allow(dead_code)]
    pub ai_column_values: std::collections::HashMap<usize, String>,
}

/// A pending job for processing
#[derive(Debug, Clone)]
pub struct PendingJob {
    /// Table name to process
    pub table_name: String,
    /// Category (if any)
    pub category: Option<String>,
    /// Step path from session start (for multi-step tracking)
    pub step_path: Vec<usize>,
    /// Parent contexts (for batched child jobs)
    pub parents: Vec<ParentContext>,
    /// Target rows for root job (when parents is empty)
    pub root_target_rows: Vec<usize>,
}

impl PendingJob {
    /// Create a first step job (session start)
    pub fn root(table_name: String, category: Option<String>, target_rows: Vec<usize>) -> Self {
        Self {
            table_name,
            category,
            step_path: Vec::new(),
            parents: Vec::new(),
            root_target_rows: target_rows,
        }
    }

    /// Create a batched child step job
    pub fn child_batch(
        table_name: String,
        category: Option<String>,
        step_path: Vec<usize>,
        parents: Vec<ParentContext>,
    ) -> Self {
        Self {
            table_name,
            category,
            step_path,
            parents,
            root_target_rows: Vec::new(),
        }
    }

    /// Check if this is the first step in the session
    pub fn is_first_step(&self) -> bool {
        self.step_path.is_empty()
    }
}

/// Current processing state
#[derive(Debug, Clone)]
pub struct ProcessingState {
    /// Current step number (0-indexed)
    pub current_step: usize,
    /// Total number of steps (may grow as structure children are discovered)
    pub total_steps: usize,
    /// Current table being processed
    pub current_table: String,
    /// Current status
    pub status: ProcessingStatus,
    /// Current job (if any)
    pub current_job: Option<PendingJob>,
    /// Error message (if status is Error)
    pub error_message: Option<String>,
    /// Generation ID for this processing session
    pub generation_id: u64,
    /// Orphaned rows from multi-parent parsing (unmatched parent prefixes)
    /// These rows need to be displayed in AI Review for re-parenting
    pub orphaned_rows: Vec<ParsedRow>,
}

impl Default for ProcessingState {
    fn default() -> Self {
        Self {
            current_step: 0,
            total_steps: 0,
            current_table: String::new(),
            status: ProcessingStatus::Idle,
            current_job: None,
            error_message: None,
            generation_id: 0,
            orphaned_rows: Vec::new(),
        }
    }
}

impl ProcessingState {
    /// Create new state for a processing session
    pub fn new(generation_id: u64) -> Self {
        Self {
            generation_id,
            ..Default::default()
        }
    }

    /// Get progress as fraction (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        if self.total_steps == 0 {
            return 0.0;
        }
        (self.current_step as f32) / (self.total_steps as f32)
    }

    /// Get progress as string (e.g., "2/5")
    pub fn progress_string(&self) -> String {
        format!("{}/{}", self.current_step, self.total_steps)
    }

    /// Get status message for display
    pub fn status_message(&self) -> String {
        let prefix = if self.total_steps > 1 {
            format!("[{}/{}] ", self.current_step, self.total_steps)
        } else {
            String::new()
        };

        let msg = match self.status {
            ProcessingStatus::Idle => "Idle".to_string(),
            ProcessingStatus::Preparing => format!("Preparing {} data...", self.current_table),
            ProcessingStatus::SendingRequest => format!("Sending {} to AI...", self.current_table),
            ProcessingStatus::ParsingResponse => "Parsing AI response...".to_string(),
            ProcessingStatus::StoringResults => "Storing results...".to_string(),
            ProcessingStatus::QueueingChildren => "Queuing child tables...".to_string(),
            ProcessingStatus::Complete => "Complete - Ready for review".to_string(),
            ProcessingStatus::Error => {
                format!("Error: {}", self.error_message.as_deref().unwrap_or("Unknown"))
            }
            ProcessingStatus::Cancelled => "Cancelled".to_string(),
        };

        format!("{}{}", prefix, msg)
    }
}

/// Result of a single step
#[derive(Debug)]
pub struct StepResult {
    /// Whether the step succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Number of rows processed
    pub rows_processed: usize,
    /// Number of AI-added rows
    pub ai_added_count: usize,
    /// Number of lost rows (sent but not returned)
    pub lost_count: usize,
    /// Raw AI response (for logging/display)
    pub raw_response: Option<String>,
}

impl StepResult {
    /// Create a success result
    pub fn success(
        rows_processed: usize,
        ai_added_count: usize,
        lost_count: usize,
        raw_response: Option<String>,
    ) -> Self {
        Self {
            success: true,
            error: None,
            rows_processed,
            ai_added_count,
            lost_count,
            raw_response,
        }
    }

    /// Create an error result
    pub fn error(message: String, raw_response: Option<String>) -> Self {
        Self {
            success: false,
            error: Some(message),
            rows_processed: 0,
            ai_added_count: 0,
            lost_count: 0,
            raw_response,
        }
    }
}

/// Information about a structure column for child job detection
///
/// Structure columns exist ONLY in metadata - they have no physical grid column.
/// Each Structure validator points to a separate child table with naming convention:
/// `{ParentSheet}_{ColumnHeader}`.
///
/// This is passed from the caller (who has access to metadata) to help
/// the Director create child jobs.
#[derive(Debug, Clone)]
pub struct StructureColumnInfo {
    /// Column index in the parent table's METADATA (not grid)
    /// This is the index into SheetMetadata.columns where validator == Structure
    pub metadata_column_index: usize,
    /// Column header from metadata (used to build child table name)
    pub column_header: String,
    /// Whether this structure is included for AI processing (ai_include_in_send)
    pub ai_include: bool,
    /// Step path for multi-step processing
    /// First level: [metadata_column_index]
    /// Nested: [parent_col_idx, nested_field_idx, ...]
    pub step_path: Vec<usize>,
}

/// Helper to create child jobs for structure tables
///
/// Structure columns are metadata-only - they don't exist as physical grid columns.
/// Child tables are separate sheets with naming convention: `{ParentSheet}_{ColumnHeader}`.
///
/// ## Usage Flow (Option A - Caller-side detection):
///
/// 1. Caller reads parent sheet metadata for Structure columns
/// 2. Caller populates ChildJobBuilder with structure info
/// 3. Caller looks up each child table in SheetRegistry
/// 4. Caller filters child table rows by `parent_key` column (always column 1)
/// 5. Caller calls `build_child_jobs()` with the row mappings
///
/// ```ignore
/// // Example integration code:
/// let mut builder = ChildJobBuilder::new("Aircraft".to_string(), category);
///
/// // From parent metadata, find Structure columns
/// for (idx, col) in metadata.columns.iter().enumerate() {
///     if matches!(col.validator, Some(ColumnValidator::Structure)) {
///         builder.add_structure_column(
///             idx,
///             col.header.clone(),
///             col.ai_include_in_send.unwrap_or(false),
///         );
///     }
/// }
///
/// // For each processed parent row, look up child rows
/// let mut child_row_map = HashMap::new();
/// for parent_row_index in processed_parents {
///     for col_info in builder.included_columns() {
///         let child_table = format!("{}_{}", "Aircraft", col_info.column_header);
///         if let Some(child_sheet) = registry.get_sheet(&category, &child_table) {
///             // Child table column 1 is always parent_key (points to parent's row_index)
///             let child_rows: Vec<usize> = child_sheet.grid.iter()
///                 .enumerate()
///                 .filter(|(_, row)| row.get(1).map(|v| v == &parent_row_index.to_string()).unwrap_or(false))
///                 .map(|(i, _)| i)
///                 .collect();
///             child_row_map.insert((parent_row_index, col_info.metadata_column_index), child_rows);
///         }
///     }
/// }
///
/// let child_jobs = builder.build_child_jobs(&processed_parents, &child_row_map);
/// director.queue_jobs(child_jobs);
/// ```
#[derive(Debug, Clone, Default)]
pub struct ChildJobBuilder {
    /// Structure columns from parent metadata
    pub structure_columns: Vec<StructureColumnInfo>,
    /// Parent sheet name for child table name generation
    pub parent_sheet_name: String,
    /// Category
    pub category: Option<String>,
}

/// Info about a processed parent row (for child job building)
/// 
/// Used by `ChildJobBuilder::build_child_jobs` to create child jobs
/// for both Original (database) and AI-added parent rows.
#[derive(Debug, Clone)]
pub struct ProcessedParentInfo {
    /// Stable index (DB row_index for Original, Navigator index for AI-added)
    pub stable_index: usize,
    /// Display value for ancestry building
    pub display_value: String,
    /// Whether this is an AI-added parent (not yet in database)
    pub is_ai_added: bool,
    /// AI-generated column values from this row (for propagating to child steps)
    /// Maps column_index -> ai_value
    pub ai_column_values: std::collections::HashMap<usize, String>,
}

impl ChildJobBuilder {
    /// Create a new builder
    pub fn new(parent_sheet_name: String, category: Option<String>) -> Self {
        Self {
            structure_columns: Vec::new(),
            parent_sheet_name,
            category,
        }
    }

    /// Add a structure column from metadata
    ///
    /// # Arguments
    /// * `metadata_column_index` - Index in SheetMetadata.columns (NOT grid column)
    /// * `column_header` - Header name from metadata (used for child table naming)
    /// * `ai_include` - Whether ai_include_in_send is true for this structure
    pub fn add_structure_column(
        &mut self,
        metadata_column_index: usize,
        column_header: String,
        ai_include: bool,
    ) {
        self.structure_columns.push(StructureColumnInfo {
            metadata_column_index,
            column_header,
            ai_include,
            step_path: vec![metadata_column_index],
        });
    }

    /// Build child jobs for processed parent rows
    ///
    /// # Arguments
    /// * `processed_parents` - Info about processed parent rows (Original and AI-added)
    /// * `child_row_map` - Map of (parent_stable_index, metadata_col_index) -> child grid row indices
    ///   Note: For AI-added parents, this map will have empty entries (no DB children exist)
    ///
    /// The child_row_map is populated by the caller who:
    /// 1. Looks up child table: `{parent_sheet}_{column_header}`
    /// 2. Filters child grid rows where column 1 (parent_key) == parent_row_index
    ///    (Only for Original parents - AI-added parents have no DB children yet)
    ///
    /// # Returns
    /// Vector of PendingJobs for child tables
    pub fn build_child_jobs(
        &self,
        processed_parents: &[ProcessedParentInfo],
        child_row_map: &std::collections::HashMap<(usize, usize), Vec<usize>>,
    ) -> Vec<PendingJob> {
        let mut jobs = Vec::new();
        
        // Group parents by structure column (child table)
        // Map: child_table_name -> Vec<ParentContext>
        let mut table_groups: std::collections::HashMap<String, Vec<ParentContext>> = std::collections::HashMap::new();
        let mut table_step_paths: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();

        for parent_info in processed_parents {
            for col_info in &self.structure_columns {
                if !col_info.ai_include {
                    continue;
                }

                // Build child table name: {ParentSheet}_{ColumnHeader}
                let child_table_name = format!("{}_{}", self.parent_sheet_name, col_info.column_header);

                // Get target rows for this parent/column combo
                // For AI-added parents, this will be empty (no DB children exist)
                let key = (parent_info.stable_index, col_info.metadata_column_index);
                let target_rows = child_row_map
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();

                // Create parent context with full info
                let parent_ctx = ParentContext {
                    parent_stable_index: parent_info.stable_index,
                    parent_table_name: self.parent_sheet_name.clone(),
                    target_rows,
                    parent_display_value: parent_info.display_value.clone(),
                    is_ai_added: parent_info.is_ai_added,
                    ai_column_values: parent_info.ai_column_values.clone(),
                };
                
                table_groups.entry(child_table_name.clone())
                    .or_default()
                    .push(parent_ctx);
                    
                table_step_paths.entry(child_table_name)
                    .or_insert_with(|| col_info.step_path.clone());
            }
        }
        
        // Create one batched job per child table
        for (table_name, parents) in table_groups {
            if let Some(step_path) = table_step_paths.get(&table_name) {
                let job = PendingJob::child_batch(
                    table_name,
                    self.category.clone(),
                    step_path.clone(),
                    parents,
                );
                jobs.push(job);
            }
        }

        jobs
    }

    /// Get columns that are included for AI processing
    pub fn included_columns(&self) -> Vec<&StructureColumnInfo> {
        self.structure_columns.iter().filter(|c| c.ai_include).collect()
    }
}

/// Director - orchestrates the AI processing flow
#[derive(Debug, Default)]
pub struct Director {
    /// Job queue
    job_queue: VecDeque<PendingJob>,
    /// Current processing state
    state: ProcessingState,
    /// Navigator for index management
    navigator: IndexMapper,
    /// Result storage
    storage: ResultStorage,
    /// Pre-processor
    pre_processor: PreProcessor,
    /// Genealogist for lineage and ancestry gathering
    genealogist: Genealogist,
    /// Messenger
    messenger: Messenger,
}

impl Director {
    /// Create a new director
    pub fn new() -> Self {
        Self {
            job_queue: VecDeque::new(),
            state: ProcessingState::default(),
            navigator: IndexMapper::new(),
            storage: ResultStorage::new(),
            pre_processor: PreProcessor::new(),
            genealogist: Genealogist::new(),
            messenger: Messenger::new(),
        }
    }

    /// Start a new processing session
    ///
    /// # Arguments
    /// * `generation_id` - Unique ID for this session
    /// * `initial_job` - The first job to process (usually root table)
    pub fn start_session(&mut self, generation_id: u64, initial_job: PendingJob) {
        // Clear previous state
        self.job_queue.clear();
        self.navigator.clear();
        self.genealogist.clear();
        self.storage.start_session(generation_id);

        // Initialize state
        self.state = ProcessingState::new(generation_id);
        self.state.total_steps = 1;
        self.state.current_table = initial_job.table_name.clone();

        // Queue the initial job
        self.job_queue.push_back(initial_job);

        // Ensure Python script exists
        Messenger::ensure_python_script();
    }

    /// Cancel the current processing session
    pub fn cancel(&mut self) {
        self.state.status = ProcessingStatus::Cancelled;
        self.job_queue.clear();
    }

    /// Get current processing state
    pub fn state(&self) -> &ProcessingState {
        &self.state
    }

    /// Get the storage (for result extraction after completion)
    pub fn storage(&self) -> &ResultStorage {
        &self.storage
    }

    /// Get the navigator
    pub fn navigator(&self) -> &IndexMapper {
        &self.navigator
    }

    /// Get the genealogist
    #[allow(dead_code)]
    pub fn genealogist(&self) -> &Genealogist {
        &self.genealogist
    }

    /// Check if there are more jobs to process
    pub fn has_pending_jobs(&self) -> bool {
        !self.job_queue.is_empty()
    }

    /// Take the next job from the queue
    pub fn take_next_job(&mut self) -> Option<PendingJob> {
        self.job_queue.pop_front()
    }

    /// Queue additional jobs (for child tables)
    pub fn queue_jobs(&mut self, jobs: Vec<PendingJob>) {
        let new_count = jobs.len();
        if new_count > 0 {
            self.state.status = ProcessingStatus::QueueingChildren;
        }
        self.job_queue.extend(jobs);
        self.state.total_steps += new_count;
    }

    /// Build StoredRowResult objects from parse results
    ///
    /// For AI-added rows, validates the ancestry chain to ensure the row
    /// can be correctly mapped back to existing parent rows.
    fn build_stored_results(
        &mut self,
        job: &PendingJob,
        batch: &PreparedBatch,
        parse_result: &ParseResult,
        column_names: &[String],
        included_indices: &[usize],
        parent_stable_index: Option<usize>,
        parent_table_name: Option<String>,
        registry: &SheetRegistry,
        expected_ancestry: &Ancestry,
    ) -> Vec<StoredRowResult> {
        let mut results = Vec::new();

        // Original rows - matched by position (first N rows correspond to first N sent rows)
        for (idx, parsed_row) in parse_result.original_rows.iter().enumerate() {
            // Get the original prepared row by position
            if let Some(prep) = batch.rows.get(idx) {
                let columns: Vec<ColumnResult> = column_names
                    .iter()
                    .enumerate()
                    .map(|(col_idx, name)| {
                        let original_val = prep.column_values.get(col_idx).cloned().unwrap_or_default();
                        let ai_val = parsed_row.get_column_by_index(col_idx).unwrap_or("").to_string();
                        // Use actual grid column index from included_indices
                        let grid_col_idx = included_indices.get(col_idx).copied().unwrap_or(col_idx);
                        ColumnResult::new(grid_col_idx, name.clone(), original_val, ai_val)
                    })
                    .collect();

                let stored = StoredRowResult::new_original(
                    prep.stable_id.clone(),
                    columns,
                );
                results.push(stored);
            }
        }

        // AI-added rows - register with navigator and validate ancestry
        for parsed_row in &parse_result.ai_added_rows {
            // Register this AI-added row and get a stable ID
            let display_value = parsed_row.display_value.clone();
            
            bevy::log::info!(
                "Registering AI-added row: table='{}', display='{}', prefix={:?}, columns={:?}",
                job.table_name,
                display_value,
                parsed_row.prefix_columns,
                parsed_row.columns
            );
            
            self.navigator.register_ai_added_rows(
                &job.table_name,
                job.category.as_deref(),
                vec![display_value.clone()],
                parent_stable_index,
                parent_table_name.clone(),
            );
            
            // Get the stable ID we just registered
            if let Some(stable_idx) = self.navigator.find_by_display_value(
                &job.table_name,
                job.category.as_deref(),
                &display_value,
            ) {
                if let Some(stable_id) = self.navigator.get(&job.table_name, job.category.as_deref(), stable_idx) {
                    let columns: Vec<ColumnResult> = column_names
                        .iter()
                        .enumerate()
                        .map(|(col_idx, name)| {
                            let ai_val = parsed_row.get_column_by_index(col_idx).unwrap_or("").to_string();
                            // Use actual grid column index from included_indices
                            let grid_col_idx = included_indices.get(col_idx).copied().unwrap_or(col_idx);
                            ColumnResult::new_ai_added(grid_col_idx, name.clone(), ai_val)
                        })
                        .collect();

                    // Validate ancestry for AI-added rows
                    // The ancestry prefix values from the AI response should match expected_ancestry
                    // If they don't match, the AI hallucinated a parent that doesn't exist
                    let (parent_valid, parent_suggestions) = if expected_ancestry.is_root() {
                        // Root table - no ancestry to validate
                        (true, Vec::new())
                    } else {
                        // Child table - validate the ancestry chain
                        // The row's prefix columns should match the expected ancestry
                        let row_ancestry: Vec<String> = parsed_row.prefix_columns.clone();
                        
                        if row_ancestry != expected_ancestry.display_values() {
                            // Ancestry mismatch - AI returned a row with different ancestry
                            // This means AI added a row for a different parent than expected
                            bevy::log::warn!(
                                "AI-added row '{}' has mismatched ancestry. Expected path: {} (parent table: {}), Got values: {:?}",
                                display_value,
                                expected_ancestry.format_path(),
                                expected_ancestry.parent_table_name().unwrap_or("(root)"),
                                row_ancestry
                            );
                            
                            // Try to validate the AI's suggested ancestry
                            let valid = Genealogist::validate_ancestry_chain(
                                registry,
                                &job.category,
                                &job.table_name,
                                &row_ancestry,
                            ).is_some();
                            
                            if !valid {
                                // Invalid ancestry - get suggestions from parent table
                                bevy::log::debug!(
                                    "Row ancestry chain: {:?}, expected row_indices: {:?}",
                                    row_ancestry,
                                    expected_ancestry.row_indices()
                                );
                                let suggestions = Genealogist::get_valid_parent_values(
                                    registry,
                                    &job.category,
                                    &job.table_name,
                                );
                                (false, suggestions)
                            } else {
                                // Valid ancestry but different from expected batch
                                // This might be OK if AI is being creative within valid bounds
                                bevy::log::debug!(
                                    "AI suggested valid but unexpected ancestry for tables: {:?}",
                                    expected_ancestry.table_names()
                                );
                                (true, Vec::new())
                            }
                        } else {
                            // Ancestry matches expected
                            (true, Vec::new())
                        }
                    };

                    let stored = StoredRowResult::new_ai_added(
                        stable_id.clone(),
                        columns,
                        parent_valid,
                        parent_suggestions,
                    );
                    results.push(stored);
                }
            }
        }

        // Lost rows (mark them)
        for lost_display in &parse_result.lost_display_values {
            if let Some(stable_idx) = self.navigator.find_by_display_value(
                &job.table_name,
                job.category.as_deref(),
                lost_display,
            ) {
                if let Some(stable_id) = self.navigator.get(&job.table_name, job.category.as_deref(), stable_idx) {
                    let stored = StoredRowResult::new_lost(stable_id.clone());
                    results.push(stored);
                }
            }
        }

        results
    }

    /// Build StoredRowResult entries for orphaned rows (unmatched parent prefixes)
    /// 
    /// These rows are returned by the AI but their prefix columns don't match any 
    /// known parent in the batch. They'll be shown in AI Review for re-parenting.
    fn build_orphaned_results(
        &mut self,
        job: &PendingJob,
        column_names: &[String],
        included_indices: &[usize],
    ) -> Vec<StoredRowResult> {
        let mut results = Vec::new();

        // Take orphaned_rows from state (swap with empty to take ownership)
        let orphaned_rows = std::mem::take(&mut self.state.orphaned_rows);

        for parsed_row in orphaned_rows {
            // Register this orphaned row as AI-added (we don't know its true parent)
            let display_value = parsed_row.display_value.clone();
            let claimed_ancestry = parsed_row.prefix_columns.clone();
            
            bevy::log::warn!(
                "Registering orphaned row: table='{}', display='{}', claimed_ancestry={:?}",
                job.table_name,
                display_value,
                claimed_ancestry
            );
            
            // Register without parent info (orphan)
            self.navigator.register_ai_added_rows(
                &job.table_name,
                job.category.as_deref(),
                vec![display_value.clone()],
                None, // No parent stable index - orphan
                None, // No parent table name - orphan
            );
            
            // Get the stable ID we just registered
            if let Some(stable_idx) = self.navigator.find_by_display_value(
                &job.table_name,
                job.category.as_deref(),
                &display_value,
            ) {
                if let Some(stable_id) = self.navigator.get(&job.table_name, job.category.as_deref(), stable_idx) {
                    let columns: Vec<ColumnResult> = column_names
                        .iter()
                        .enumerate()
                        .map(|(col_idx, name)| {
                            let ai_val = parsed_row.get_column_by_index(col_idx).unwrap_or("").to_string();
                            let grid_col_idx = included_indices.get(col_idx).copied().unwrap_or(col_idx);
                            ColumnResult::new_ai_added(grid_col_idx, name.clone(), ai_val)
                        })
                        .collect();

                    let stored = StoredRowResult::new_orphaned(
                        stable_id.clone(),
                        columns,
                        claimed_ancestry,
                    );
                    results.push(stored);
                }
            }
        }

        results
    }

    // ========================================================================
    // Sync/Async Split API for Bevy Integration
    // ========================================================================
    //
    // The Director owns all orchestration logic but needs to work with Bevy's
    // async task system. These methods split the workflow:
    //
    // 1. `prepare_step()` - Sync: Prepares data, builds payload JSON
    // 2. (Integration spawns async Python call)
    // 3. `complete_step()` - Sync: Takes result, parses, stores, returns StepResult
    //
    // This keeps orchestration in Director while letting Integration handle
    // the async boundary.

    /// Prepared data for an async step
    /// 
    /// Returned by `prepare_step()`, contains everything needed for the Python call.
    pub fn prepare_step(
        &mut self,
        job: &PendingJob,
        grid: &[Vec<String>],
        row_indices: &[i64],
        registry: &SheetRegistry,
        mut request_config: RequestConfig,
    ) -> Result<PreparedStep, String> {
        self.state.status = ProcessingStatus::Preparing;
        self.state.current_table = job.table_name.clone();
        self.state.current_job = Some(job.clone());

        // Step 1: Build PreProcessConfig using included_indices from RequestConfig
        // Use is_child_table from config to correctly detect table type (not step position)
        let key_column_index = PreProcessor::get_key_column_index(request_config.is_child_table, None);
        
        // Build column names from indices (for PreProcessConfig compatibility)
        // These are just placeholders - the actual data extraction uses included_indices
        let column_names: Vec<String> = request_config.included_indices.iter()
            .map(|i| format!("col_{}", i))
            .collect();
        
        let mut batches = Vec::new();
        let mut batch_ancestries: Vec<Ancestry> = Vec::new();
        
        // Calculate ancestry depth for this table (static per table, not per row)
        let ancestry_depth = Genealogist::get_table_depth(
            registry,
            &job.category,
            &job.table_name,
        );
        
        // Gather ancestry contexts once per table (for column_contexts prefix)
        let ancestry_contexts = self.genealogist.gather_ancestry_contexts(
            registry,
            &job.category,
            &job.table_name,
        );
        
        if job.is_first_step() {
            // First step - may be root table or child table (when starting from structure navigation)
            // Check if we have root parent info from navigation context
            let pre_process_config = if let (Some(parent_table), Some(parent_idx)) = (
                &request_config.root_parent_table_name,
                request_config.root_parent_stable_index,
            ) {
                // First step is a child table - register the root parent in Navigator first
                // so that lineage building can find it when walking up the parent chain
                if !request_config.lineage_prefix_values.is_empty() {
                    // The lineage_prefix_values contain display values from root to immediate parent
                    // The last value corresponds to the immediate parent (root_parent_stable_index)
                    let parent_display = request_config.lineage_prefix_values.last()
                        .cloned()
                        .unwrap_or_default();
                    
                    // Register the root parent in Navigator as an original row
                    // This allows the Genealogist to find it when building lineage
                    self.navigator.register_original_rows(
                        parent_table,
                        job.category.as_deref(),
                        vec![(parent_idx, parent_display)],
                        None,  // Root parent has no parent
                        None,
                    );
                }
                
                // First step is a child table - use child config with parent info
                PreProcessConfig::for_child_table(
                    job.table_name.clone(),
                    job.category.clone(),
                    key_column_index,
                    request_config.included_indices.clone(),
                    column_names,
                    job.step_path.clone(),
                    parent_idx,
                    parent_table.clone(),
                )
            } else {
                // True root table - no parent info
                PreProcessConfig::for_root_table(
                    job.table_name.clone(),
                    job.category.clone(),
                    key_column_index,
                    request_config.included_indices.clone(),
                    column_names,
                )
            };

            let batch = self.pre_processor.prepare_batch(
                pre_process_config,
                grid,
                row_indices,
                &job.root_target_rows,
                &mut self.navigator,
            );
            
            if batch.is_empty() {
                return Err("No rows to process".to_string());
            }
            
            batches.push(batch);
            // Root table: ancestry comes from navigation stack (handled via lineage_prefix_values in config)
            batch_ancestries.push(Ancestry::empty());
        } else {
            // Child table - multiple batches (one per parent)
            // Each parent has its own ancestry chain
            for parent in &job.parents {
                let pre_process_config = PreProcessConfig::for_child_table(
                    job.table_name.clone(),
                    job.category.clone(),
                    key_column_index,
                    request_config.included_indices.clone(),
                    column_names.clone(),
                    job.step_path.clone(),
                    parent.parent_stable_index,
                    parent.parent_table_name.clone(),
                );

                let batch = self.pre_processor.prepare_batch(
                    pre_process_config,
                    grid,
                    row_indices,
                    &parent.target_rows,
                    &mut self.navigator,
                );
                
                // Build ancestry for this parent using Genealogist
                // Handles both AI-added and original parents
                let ancestry = self.genealogist.gather_ancestry_for_batch(
                    &job.category,
                    &job.table_name,
                    parent.parent_stable_index,
                    &parent.parent_table_name,
                    &parent.parent_display_value,
                    parent.is_ai_added,
                    &self.navigator,
                    &self.storage,
                    registry,
                );
                
                // Log ancestry chain for debugging
                if !ancestry.is_root() {
                    bevy::log::debug!(
                        "Batch ancestry for table '{}': {} (parent_key={}, is_ai_added={})",
                        job.table_name,
                        ancestry.format_path(),
                        ancestry.parent_row_index().unwrap_or(0),
                        parent.is_ai_added
                    );
                }
                
                batch_ancestries.push(ancestry);
                batches.push(batch);
            }
            
            if batches.is_empty() {
                return Err("No parents to process".to_string());
            }
        }

        // Log ancestry context info for debugging
        if !ancestry_contexts.is_root() {
            bevy::log::debug!(
                "Table '{}' has ancestry depth {} with tables: {:?}",
                job.table_name,
                ancestry_contexts.depth(),
                ancestry_contexts.table_names
            );
        }

        // Set prefix column names from ancestry for response parsing (object format)
        request_config.prefix_column_names = ancestry_contexts.table_names.clone();

        // Step 4: Build payload JSON with ancestry
        let payload_json = self.messenger.build_payload_with_ancestry(
            &request_config,
            &batches,
            &batch_ancestries,
            &ancestry_contexts.contexts,
        )?;

        // Store batch in state for complete_step
        self.state.status = ProcessingStatus::SendingRequest;

        Ok(PreparedStep {
            payload_json,
            batches,
            request_config,
            ancestry_depth,
            batch_ancestries,
        })
    }

    /// Complete a step after receiving the async result
    /// 
    /// Takes the raw MessengerResult from the Python call and:
    /// 1. Parses the response
    /// 2. Registers AI-added rows with Navigator
    /// 3. Validates ancestry for AI-added rows
    /// 4. Stores results in Storager
    /// 5. Returns StepResult for the caller
    pub fn complete_step(
        &mut self,
        job: &PendingJob,
        prepared: &PreparedStep,
        messenger_result: MessengerResult,
        registry: &SheetRegistry,
    ) -> StepResult {
        // Handle error case
        if !messenger_result.success {
            self.state.status = ProcessingStatus::Error;
            let error_msg = messenger_result.error.clone().unwrap_or_else(|| "Unknown error".to_string());
            self.state.error_message = Some(error_msg.clone());
            return StepResult::error(
                error_msg,
                messenger_result.raw_response,
            );
        }

        // Step 1: Parse response
        self.state.status = ProcessingStatus::ParsingResponse;

        // Calculate prefix count for parser
        // For first step with lineage_prefix_values: use ONLY lineage length (ancestry is already captured)
        // For child steps: use ancestry_depth (no lineage prefix in child steps)
        let prefix_count = if !prepared.request_config.lineage_prefix_values.is_empty() {
            // First step starting from child table - lineage captures the full ancestry
            prepared.request_config.lineage_prefix_values.len()
        } else {
            // Root table start or child step - use table's structural depth
            prepared.ancestry_depth
        };

        // The key column index for parsing should be the position within included columns,
        // not the raw grid index. For child tables, grid key is column 2 (after row_index, parent_key),
        // but if included_indices is [2, 3], the key is at position 0 within expected_columns.
        let grid_key_col = PreProcessor::get_key_column_index(prepared.request_config.is_child_table, None);
        let parser_key_col = prepared.request_config.included_indices
            .iter()
            .position(|&idx| idx == grid_key_col)
            .unwrap_or(0);

        let parser = ResponseParser::new(
            prepared.request_config.column_names.clone(),
            parser_key_col,
            prefix_count,
            prepared.request_config.prefix_column_names.clone(),
        );

        let raw_response = messenger_result.raw_response.clone().unwrap_or_default();
        
        let parse_results = if job.is_first_step() {
            // Single batch
            let sent_count = prepared.batches[0].rows.len();
            let result = parser.parse(&raw_response, sent_count);
            vec![result]
        } else {
            // Multiple batches - mixed response
            // Build map of Full Ancestry Path -> Sent Count
            // Key is all ancestor display values joined, to uniquely identify each parent
            let mut parent_map = HashMap::new();
            let mut parent_order = Vec::new(); // To preserve order for results
            
            for (idx, _p_ctx) in job.parents.iter().enumerate() {
                // Get full ancestry path as key (all levels joined)
                let ancestry = prepared.batch_ancestries.get(idx)
                    .cloned()
                    .unwrap_or_else(Ancestry::empty);
                
                // Join all ancestry levels to create unique key
                let ancestry_key = ancestry.levels.iter()
                    .map(|level| level.display_value.clone())
                    .collect::<Vec<_>>()
                    .join("|");
                
                let sent_count = prepared.batches[idx].rows.len();
                parent_map.insert(ancestry_key.clone(), sent_count);
                parent_order.push(ancestry_key);
            }
            
            match parser.parse_multi_parent_response(&raw_response, &parent_map) {
                Ok(multi_result) => {
                    // Destructure the multi-parent result
                    let mut results_map = multi_result.by_parent;
                    
                    // Reorder results to match job.parents order
                    let mut ordered_results = Vec::new();
                    for parent_val in parent_order {
                        if let Some(res) = results_map.remove(&parent_val) {
                            ordered_results.push(res);
                        } else {
                            // If a parent got no results, create an empty result with all rows lost
                            let sent_count = *parent_map.get(&parent_val).unwrap_or(&0);
                            let lost_display_values: Vec<String> = (0..sent_count)
                                .map(|i| format!("Row {}", i))
                                .collect();
                            ordered_results.push(ParseResult {
                                original_rows: Vec::new(),
                                ai_added_rows: Vec::new(),
                                lost_display_values,
                                error: None,
                            });
                        }
                    }
                    
                    // Store orphaned rows in state for later processing
                    // These are rows with unmatched parent prefixes that need re-parenting
                    if !multi_result.orphaned_rows.is_empty() {
                        self.state.orphaned_rows = multi_result.orphaned_rows;
                    }
                    
                    ordered_results
                },
                Err(e) => {
                     self.state.status = ProcessingStatus::Error;
                     self.state.error_message = Some(e.clone());
                     return StepResult::error(e, Some(raw_response));
                }
            }
        };

        // Check for errors in results
        for res in &parse_results {
            if !res.is_success() {
                self.state.status = ProcessingStatus::Error;
                let error_msg = res.error.clone().unwrap_or_else(|| "Parse failed".to_string());
                self.state.error_message = Some(error_msg.clone());
                return StepResult::error(
                    error_msg,
                    Some(raw_response),
                );
            }
        }

        // Step 2: Store results (AI-added rows are registered with Navigator during this step)
        self.state.status = ProcessingStatus::StoringResults;

        let mut total_processed = 0;
        let mut total_added = 0;
        let mut total_lost = 0;
        let mut all_stored_results = Vec::new();

        for (idx, parse_result) in parse_results.iter().enumerate() {
            let batch = &prepared.batches[idx];
            
            // Get expected ancestry for this batch
            let expected_ancestry = prepared.batch_ancestries.get(idx)
                .cloned()
                .unwrap_or_else(Ancestry::empty);
            
            // Need to handle parent info for AI added rows registration
            // For root, parent is None (unless we have root parent from navigation).
            // For child, parent is job.parents[idx].
            
            let (parent_stable_index, parent_table_name) = if job.is_first_step() {
                // First step - check for root parent info from navigation context
                (
                    prepared.request_config.root_parent_stable_index,
                    prepared.request_config.root_parent_table_name.clone(),
                )
            } else {
                let p = &job.parents[idx];
                (Some(p.parent_stable_index), Some(p.parent_table_name.clone()))
            };

            let stored_results = self.build_stored_results(
                job,
                batch,
                parse_result,
                &prepared.request_config.column_names,
                &prepared.request_config.included_indices,
                parent_stable_index,
                parent_table_name,
                registry,
                &expected_ancestry,
            );
            
            all_stored_results.extend(stored_results);
            
            total_processed += parse_result.original_rows.len() + parse_result.ai_added_rows.len();
            total_added += parse_result.ai_added_rows.len();
            total_lost += parse_result.lost_display_values.len();
        }

        // Build StoredRowResults for orphaned rows (unmatched parent prefixes)
        // These are rows the AI returned that don't match any parent in the batch
        if !self.state.orphaned_rows.is_empty() {
            let orphaned_results = self.build_orphaned_results(
                job,
                &prepared.request_config.column_names,
                &prepared.request_config.included_indices,
            );
            bevy::log::info!(
                "Built {} orphaned row results for table '{}'",
                orphaned_results.len(),
                job.table_name
            );
            total_added += orphaned_results.len();
            all_stored_results.extend(orphaned_results);
        }

        self.storage.store_results(
            &job.table_name,
            job.category.as_deref(),
            job.step_path.clone(),
            all_stored_results,
        );

        // For child steps, also store the list of processed parents
        // This enables creating empty StructureReviewEntry items when AI returns 0 results
        if !job.is_first_step() && !job.parents.is_empty() {
            use super::storager::ProcessedParent;
            let processed_parents: Vec<ProcessedParent> = job.parents.iter().map(|p| {
                ProcessedParent {
                    parent_table: p.parent_table_name.clone(),
                    parent_stable_index: p.parent_stable_index,
                    is_ai_added: p.is_ai_added,
                }
            }).collect();
            self.storage.store_processed_parents(
                &job.table_name,
                job.category.as_deref(),
                job.step_path.clone(),
                processed_parents,
            );
        }

        // Step 4: Update step counter
        self.state.current_step += 1;
        self.storage.set_current_step(self.state.current_step);

        // Step 5: Update status based on remaining work
        if self.job_queue.is_empty() {
            self.state.status = ProcessingStatus::Complete;
        } else {
            self.state.status = ProcessingStatus::QueueingChildren;
        }

        // Build result
        StepResult::success(
            total_processed,
            total_added,
            total_lost,
            messenger_result.raw_response,
        )
    }
}

/// Prepared step data for async execution
/// 
/// Returned by `prepare_step()`, contains the payload and batch data
/// needed by `complete_step()`.
#[derive(Debug, Clone)]
pub struct PreparedStep {
    /// JSON payload to send to Python
    pub payload_json: String,
    /// Prepared batches (stored for complete_step)
    /// For root table: contains 1 batch
    /// For child table: contains N batches (one per parent)
    pub batches: Vec<PreparedBatch>,
    /// Request config (stored for complete_step)
    pub request_config: RequestConfig,
    /// Ancestry depth (number of ancestor columns prepended to each row)
    /// Used by parser to correctly strip prefix columns from AI response
    pub ancestry_depth: usize,
    /// Per-batch ancestry (for multi-parent child tables)
    /// Each entry corresponds to a batch in `batches`
    /// For root tables, this is empty
    pub batch_ancestries: Vec<Ancestry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processing_state() {
        let state = ProcessingState::new(12345);
        assert_eq!(state.generation_id, 12345);
        assert_eq!(state.status, ProcessingStatus::Idle);
        assert_eq!(state.progress(), 0.0);
    }

    #[test]
    fn test_pending_job() {
        let root_job = PendingJob::root("Aircraft".to_string(), None, vec![0, 1, 2]);
        assert!(root_job.is_first_step());

        let parents = vec![ParentContext {
            parent_stable_index: 5,
            parent_table_name: "Aircraft".to_string(),
            target_rows: vec![0, 1],
            parent_display_value: "F-16C".to_string(),
            is_ai_added: false,
            ai_column_values: std::collections::HashMap::new(),
        }];

        let child_job = PendingJob::child_batch(
            "Engines".to_string(),
            None,
            vec![0],
            parents,
        );
        assert!(!child_job.is_first_step());
        assert_eq!(child_job.parents[0].parent_stable_index, 5);
    }

    #[test]
    fn test_director_session() {
        let mut director = Director::new();

        let job = PendingJob::root("Aircraft".to_string(), None, vec![0, 1]);
        director.start_session(1, job);

        assert!(director.has_pending_jobs());
        assert_eq!(director.state().total_steps, 1);
        assert_eq!(director.state().generation_id, 1);

        let next = director.take_next_job();
        assert!(next.is_some());
        assert!(!director.has_pending_jobs());
    }

    #[test]
    fn test_queue_child_jobs() {
        let mut director = Director::new();

        let root_job = PendingJob::root("Aircraft".to_string(), None, vec![0]);
        director.start_session(1, root_job);
        assert_eq!(director.state().total_steps, 1);

        // Simulate processing root and queuing children
        let _ = director.take_next_job();

        let parents_0 = vec![ParentContext {
            parent_stable_index: 0,
            parent_table_name: "Aircraft".to_string(),
            target_rows: vec![0, 1],
            parent_display_value: "MiG-25PD".to_string(),
            is_ai_added: false,
            ai_column_values: std::collections::HashMap::new(),
        }];
        
        let parents_1 = vec![ParentContext {
            parent_stable_index: 0,
            parent_table_name: "Aircraft".to_string(),
            target_rows: vec![0],
            parent_display_value: "MiG-25PD".to_string(),
            is_ai_added: false,
            ai_column_values: std::collections::HashMap::new(),
        }];

        let child_jobs = vec![
            PendingJob::child_batch("Engines".to_string(), None, vec![0], parents_0),
            PendingJob::child_batch("Weapons".to_string(), None, vec![1], parents_1),
        ];
        director.queue_jobs(child_jobs);

        assert_eq!(director.state().total_steps, 3); // 1 + 2 children
        assert!(director.has_pending_jobs());
    }

    #[test]
    fn test_step_result() {
        let success = StepResult::success(10, 2, 1, Some("raw".to_string()));
        assert!(success.success);
        assert_eq!(success.rows_processed, 10);
        assert_eq!(success.ai_added_count, 2);
        assert_eq!(success.lost_count, 1);
        assert_eq!(success.raw_response, Some("raw".to_string()));

        let error = StepResult::error("Test error".to_string(), None);
        assert!(!error.success);
        assert_eq!(error.error, Some("Test error".to_string()));
        assert!(error.raw_response.is_none());
    }

    #[test]
    fn test_child_job_builder() {
        use std::collections::HashMap;

        let mut builder = ChildJobBuilder::new("Aircraft".to_string(), None);

        // Add structure columns
        builder.add_structure_column(2, "Engines".to_string(), true);
        builder.add_structure_column(3, "Weapons".to_string(), true);
        builder.add_structure_column(4, "Avionics".to_string(), false); // Not included

        assert_eq!(builder.included_columns().len(), 2);

        // Create child row ranges
        let mut ranges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        // Parent 0 has children in both Engines and Weapons
        ranges.insert((0, 2), vec![0, 1, 2]); // 3 engine rows for parent 0
        ranges.insert((0, 3), vec![0, 1]); // 2 weapon rows for parent 0
        // Parent 1 has only engine children
        ranges.insert((1, 2), vec![3, 4]); // 2 engine rows for parent 1

        let parents_info = vec![
            ProcessedParentInfo { stable_index: 0, display_value: "F-16C".to_string(), is_ai_added: false, ai_column_values: std::collections::HashMap::new() },
            ProcessedParentInfo { stable_index: 1, display_value: "MiG-25".to_string(), is_ai_added: false, ai_column_values: std::collections::HashMap::new() },
        ];
        let jobs = builder.build_child_jobs(&parents_info, &ranges);

        // Should have 2 jobs (one per child table, batched):
        // - Aircraft_Engines (Parents 0 and 1)
        // - Aircraft_Weapons (Parents 0 and 1) - both parents get weapon jobs, even if 1 has no children yet
        assert_eq!(jobs.len(), 2);

        // Find engines job
        let engines_job = jobs.iter().find(|j| j.table_name == "Aircraft_Engines").unwrap();
        assert_eq!(engines_job.parents.len(), 2);
        
        // Check parent 0 in engines job
        let p0 = engines_job.parents.iter().find(|p| p.parent_stable_index == 0).unwrap();
        assert_eq!(p0.target_rows, vec![0, 1, 2]);
        
        // Check parent 1 in engines job
        let p1 = engines_job.parents.iter().find(|p| p.parent_stable_index == 1).unwrap();
        assert_eq!(p1.target_rows, vec![3, 4]);

        // Find weapons job - now includes all parents (AI can generate children for those without)
        let weapons_job = jobs.iter().find(|j| j.table_name == "Aircraft_Weapons").unwrap();
        assert_eq!(weapons_job.parents.len(), 2); // Both parents now included
        
        // Parent 0 has existing weapon children
        let p0_weapons = weapons_job.parents.iter().find(|p| p.parent_stable_index == 0).unwrap();
        assert_eq!(p0_weapons.target_rows, vec![0, 1]);
        
        // Parent 1 has no existing weapon children (AI will generate them)
        let p1_weapons = weapons_job.parents.iter().find(|p| p.parent_stable_index == 1).unwrap();
        assert!(p1_weapons.target_rows.is_empty());
    }

    #[test]
    fn test_child_job_builder_no_children() {
        use std::collections::HashMap;

        let mut builder = ChildJobBuilder::new("Aircraft".to_string(), None);
        builder.add_structure_column(2, "Engines".to_string(), true);

        // No children defined
        let ranges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();

        let parents_info = vec![
            ProcessedParentInfo { stable_index: 0, display_value: "F-16C".to_string(), is_ai_added: false, ai_column_values: HashMap::new() },
            ProcessedParentInfo { stable_index: 1, display_value: "MiG-25".to_string(), is_ai_added: false, ai_column_values: HashMap::new() },
        ];
        let jobs = builder.build_child_jobs(&parents_info, &ranges);
        // Jobs are created for all parents, even if they have no children
        // (AI will be asked to generate children for them)
        assert_eq!(jobs.len(), 1); // 1 job for Aircraft_Engines with 2 parents
    }

    #[test]
    fn test_processing_status_cancelled() {
        let mut director = Director::new();
        let job = PendingJob::root("Aircraft".to_string(), None, vec![0, 1, 2]);
        director.start_session(1, job);
        
        assert!(director.has_pending_jobs());
        
        // Cancel the session
        director.cancel();
        
        assert_eq!(director.state().status, ProcessingStatus::Cancelled);
        assert!(!director.has_pending_jobs()); // Queue should be cleared
    }

    #[test]
    fn test_queue_jobs_sets_queuing_children_status() {
        let mut director = Director::new();
        let job = PendingJob::root("Aircraft".to_string(), None, vec![0]);
        director.start_session(1, job);
        
        // Take the root job
        let _ = director.take_next_job();
        
        let parents = vec![ParentContext {
            parent_stable_index: 0,
            parent_table_name: "Aircraft".to_string(),
            target_rows: vec![0, 1],
            parent_display_value: "F-16C".to_string(),
            is_ai_added: false,
            ai_column_values: std::collections::HashMap::new(),
        }];

        // Queue child jobs
        let child_jobs = vec![
            PendingJob::child_batch("Engines".to_string(), None, vec![0], parents),
        ];
        director.queue_jobs(child_jobs);
        
        // Status should be QueueingChildren
        assert_eq!(director.state().status, ProcessingStatus::QueueingChildren);
        assert_eq!(director.state().total_steps, 2);
    }
}
