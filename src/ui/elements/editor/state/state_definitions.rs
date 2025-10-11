// src/ui/elements/editor/state_definitions.rs
// Type definitions and enums for editor state

use crate::sheets::definitions::{ColumnDataType, ColumnValidator};
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiModeState {
    #[default]
    Idle,
    Preparing,
    Submitting,
    ResultsReady,
    Reviewing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SheetInteractionState {
    #[default]
    Idle,
    AiModeActive,
    DeleteModeActive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidatorTypeChoice {
    #[default]
    Basic,
    Linked,
    Structure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewChoice {
    Original,
    AI,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToyboxMode {
    #[default]
    Randomizer,
    Summarizer,
}

#[derive(Clone, Debug)]
pub struct FilteredRowsCacheEntry {
    pub rows: Arc<Vec<usize>>,
    pub filters_hash: u64,
    pub total_rows: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FpsSetting {
    Thirty,
    Sixty,
    ScreenHz, // Auto
}

impl Default for FpsSetting {
    fn default() -> Self {
        FpsSetting::Sixty
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ThrottledAiAction {
    UpdateCell {
        row_index: usize,
        col_index: usize,
        value: String,
    },
    AddRow {
        initial_values: Vec<(usize, String)>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ColumnDragState {
    pub source_index: Option<usize>,
}

/// Log entry for a single AI call (newest entries are added to the front)
#[derive(Debug, Clone)]
pub struct AiCallLogEntry {
    /// Human-readable status (e.g., "2/4 received", "Completed", "Error")
    pub status: String,
    /// The full response JSON (if available)
    pub response: Option<String>,
    /// The full request payload that was sent
    pub request: Option<String>,
    /// Timestamp or sequence number for ordering
    pub timestamp: std::time::SystemTime,
    /// Whether this is an error entry
    pub is_error: bool,
}

/// Phase 1 intermediate data - stored after initial discovery call, before Phase 2 deep review
#[derive(Debug, Clone)]
pub struct Phase1IntermediateData {
    /// All rows from Phase 1 AI response
    pub all_ai_rows: Vec<Vec<String>>,
    /// Indices of rows that are duplicates (in all_ai_rows, after originals)
    pub duplicate_indices: Vec<usize>,
    /// Number of original rows sent
    pub original_count: usize,
    /// Included column indices
    pub included_columns: Vec<usize>,
    /// Context for sending Phase 2
    pub category: Option<String>,
    pub sheet_name: String,
    pub key_prefix_count: usize,
    /// Original row indices from Phase 1 (for structure processing)
    pub original_row_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct StructureParentContext {
    pub parent_category: Option<String>,
    pub parent_sheet: String,
    pub parent_row: usize,
    pub parent_col: usize,
}

/// Navigation context for real structure sheets (not virtual)
/// When navigating into a structure column, we open the real structure sheet
/// with a hidden filter that shows only rows related to the parent row
#[derive(Debug, Clone)]
pub struct StructureNavigationContext {
    /// The real structure sheet name (e.g., "TacticalFrontlines_Items")
    pub structure_sheet_name: String,
    /// Parent sheet information
    pub parent_category: Option<String>,
    pub parent_sheet_name: String,
    /// Parent row's primary key value (for filtering)
    pub parent_row_key: String,
    /// Column name in parent that was clicked
    pub parent_column_name: String,
}

#[derive(Debug, Clone)]
pub struct RowReview {
    pub row_index: usize,
    pub original: Vec<String>,
    pub ai: Vec<String>,
    pub choices: Vec<ReviewChoice>, // length == editable (non-structure) columns shown order
    pub non_structure_columns: Vec<usize>, // mapping for editable subset
}

#[derive(Debug, Clone)]
pub struct NewRowReview {
    pub ai: Vec<String>,
    pub non_structure_columns: Vec<usize>,
    // If Some(row_index) then this new row's first column matches an existing row's first column
    // allowing user to choose between merging into that row or adding a separate row.
    pub duplicate_match_row: Option<usize>,
    // Per-column review choices (only meaningful when merging). Mirrors RowReview. Length == non_structure_columns.len()
    pub choices: Option<Vec<ReviewChoice>>,
    // Whether the user currently has the Merge option selected (pre-decision). Defaults true if duplicate_match_row present.
    pub merge_selected: bool,
    // Whether the user clicked Decide (once decided Accept/Cancel replace Decide and toggle is hidden)
    pub merge_decided: bool,
    // Original row data for the matched duplicate row (used for merge comparison)
    pub original_for_merge: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct StructureSendJob {
    pub root_category: Option<String>,
    pub root_sheet: String,
    /// Identifies the nested structure (first element is root column index)
    pub structure_path: Vec<usize>,
    /// Optional friendly label resolved from metadata for status logs
    pub label: Option<String>,
    /// Snapshot of row indices (within the root sheet) that triggered this job
    pub target_rows: Vec<usize>,
    pub generation_id: u64,
}

#[derive(Debug, Clone)]
pub struct StructureNewRowContext {
    pub new_row_index: usize,
    pub non_structure_values: Vec<(usize, String)>,
    /// Full AI row from Phase 1, including structure columns (stored as JSON strings)
    pub full_ai_row: Option<Vec<String>>,
}

/// Context for navigating into structure detail view during AI review
#[derive(Debug, Clone)]
pub struct StructureDetailContext {
    /// Root category of the sheet containing the structure
    pub root_category: Option<String>,
    /// Root sheet name containing the structure
    pub root_sheet: String,
    /// Index of parent row in ai_row_reviews (for existing rows)
    pub parent_row_index: Option<usize>,
    /// Index of parent row in ai_new_row_reviews (for new rows)
    pub parent_new_row_index: Option<usize>,
    /// Structure path to the structure being viewed
    pub structure_path: Vec<usize>,
    /// One-time hydration flag so we don't rebuild working RowReview vectors every frame
    pub hydrated: bool,
    /// Saved top-level reviews from before entering structure mode (restored on exit)
    pub saved_row_reviews: Vec<RowReview>,
    pub saved_new_row_reviews: Vec<NewRowReview>,
}

#[derive(Debug, Clone)]
pub struct StructureReviewEntry {
    pub root_category: Option<String>,
    pub root_sheet: String,
    pub parent_row_index: usize,
    pub parent_new_row_index: Option<usize>,
    /// Path from root sheet to this structure (first element is column index in root, subsequent are structure col indices)
    pub structure_path: Vec<usize>,
    pub has_changes: bool,
    pub accepted: bool,
    pub rejected: bool,
    /// Whether all rows inside the structure have been decided (accepted or declined).
    /// This is true when the structure review is complete, regardless of accept/reject.
    pub decided: bool,
    /// The original structure rows parsed from the cell
    pub original_rows: Vec<Vec<String>>,
    /// The AI-suggested structure rows
    pub ai_rows: Vec<Vec<String>>,
    /// The merged rows (combines accepted changes)
    pub merged_rows: Vec<Vec<String>>,
    /// Per-row, per-column difference flags
    pub differences: Vec<Vec<bool>>,
    /// Per-row operation flags: None = no change, Some(RowOperation) = tracked action
    pub row_operations: Vec<RowOperation>,
    /// Schema field headers for proper JSON serialization
    pub schema_headers: Vec<String>,
    /// Number of original rows (before AI additions) - used to identify AI-added rows
    pub original_rows_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowOperation {
    /// Row was merged with AI changes
    Merged,
    /// Row was deleted
    Deleted,
    /// Row was added (new AI row)
    Added,
}

impl StructureReviewEntry {
    pub fn is_pending(&self) -> bool {
        self.has_changes && !self.accepted && !self.rejected
    }

    pub fn is_undecided(&self) -> bool {
        self.has_changes && !self.decided
    }
}

#[derive(Debug, Clone)]
pub struct VirtualStructureContext {
    pub virtual_sheet_name: String,
    pub parent: StructureParentContext,
}

#[derive(Resource)]
pub struct EditorWindowState {
    pub selected_category: Option<String>,
    pub selected_sheet_name: Option<String>,
    // Stack of active virtual structure sheets (each represents a nested structure view)
    // Top of stack is current virtual sheet. Empty means not in structure view.
    pub virtual_structure_stack: Vec<VirtualStructureContext>,

    // NEW: Stack for real structure sheet navigation with hidden filters
    // When user clicks a structure column cell, we push a context and navigate to the real structure sheet
    // The filter is hidden and temporal - only active during this navigation path
    pub structure_navigation_stack: Vec<StructureNavigationContext>,

    // Popups related to selected sheet
    pub show_rename_popup: bool,
    pub rename_target_category: Option<String>,
    pub rename_target_sheet: String,
    pub new_name_input: String, // Used by rename sheet and new sheet

    pub show_delete_confirm_popup: bool,
    pub delete_target_category: Option<String>,
    pub delete_target_sheet: String,

    pub show_column_options_popup: bool,
    pub options_column_target_category: Option<String>,
    pub options_column_target_sheet: String,
    pub options_column_target_index: usize,
    pub column_options_popup_needs_init: bool,
    pub options_column_rename_input: String,
    pub options_column_filter_input: String,
    // Multi-term OR filter terms (each term 'contains' OR). Joined when stored.
    pub options_column_filter_terms: Vec<String>,
    pub options_column_ai_context_input: String,
    pub options_validator_type: Option<ValidatorTypeChoice>,
    pub options_basic_type_select: ColumnDataType,
    pub options_link_target_sheet: Option<String>,
    pub options_link_target_column_index: Option<usize>,
    // NEW: Structure selection chain (always at least length 1 with possibly None meaning no selection yet)
    pub options_structure_source_columns: Vec<Option<usize>>,
    pub linked_column_cache: HashMap<(String, usize), Arc<HashSet<String>>>,
    // Normalized (lowercased, CR/LF removed) mirror of linked_column_cache for O(1) membership
    pub linked_column_cache_normalized: HashMap<(String, usize), Arc<HashSet<String>>>,

    // NEW: State for New Sheet Popup
    pub show_new_sheet_popup: bool,
    pub new_sheet_name_input: String, // Re-using new_name_input is an option, but separate is cleaner
    pub new_sheet_target_category: Option<String>,
    pub new_sheet_show_validation_hint: bool,

    // NEW: State for Add Table Popup (database mode)
    pub show_add_table_popup: bool,

    // Category management UI state
    pub show_new_category_popup: bool,
    pub new_category_name_input: String,
    pub show_delete_category_confirm_popup: bool,
    pub delete_category_name: Option<String>,
    pub show_delete_category_double_confirm_popup: bool,

    // AI Mode specific state
    pub ai_mode: AiModeState,
    pub ai_selected_rows: HashSet<usize>,
    pub ai_batch_review_active: bool, // unified batch review flag
    // Unified snapshot model
    pub ai_row_reviews: Vec<RowReview>,
    pub ai_new_row_reviews: Vec<NewRowReview>,
    pub ai_structure_reviews: Vec<StructureReviewEntry>,
    pub ai_structure_new_row_contexts: HashMap<usize, StructureNewRowContext>,
    pub ai_structure_new_row_token_counter: usize,
    /// Cache of original row content snapshots for rendering original previews.
    ///
    /// **Design**: Single source of truth for all original row content in AI review.
    /// - Key: `(parent_row_index, parent_new_row_index)` where exactly one is `Some`.
    ///   - `(Some(row_idx), None)`: Existing row at `row_idx`.
    ///   - `(None, Some(new_idx))`: New/duplicate row at `new_idx` in `ai_new_row_reviews`.
    /// - Value: Complete grid row including raw structure cell JSON strings.
    ///
    /// **Population**:
    /// - For existing rows: Cloned from sheet grid when AI results processed.
    /// - For duplicate rows: Cloned from matched existing row in grid.
    /// - For truly new rows: Empty row with correct column count.
    ///
    /// **Usage**:
    /// - All original preview rendering uses this cache (no per-frame parsing).
    /// - Structure columns: Extract JSON and parse once in helper, or use pre-parsed
    ///   `StructureReviewEntry.original_rows` for nested structures.
    /// - Regular columns: Already extracted into `RowReview.original` / `NewRowReview.original_for_merge`.
    ///
    /// **Performance**: Eliminates repeated JSON parsing on every frame render.
    pub ai_original_row_snapshot_cache: HashMap<(Option<usize>, Option<usize>), Vec<String>>,
    // New unified review model
    // (legacy single-row review fields removed)
    pub ai_model_id_input: String,
    pub ai_general_rule_input: String,

    // Structure detail navigation context (when user dives into a structure review)
    pub ai_structure_detail_context: Option<StructureDetailContext>,

    // Combined AI call log (chat-like format, newest first)
    pub ai_call_log: Vec<AiCallLogEntry>,
    // Removed dedicated structure detail view; field deleted.
    pub ai_raw_output_display: String,
    // Bottom AI output panel visibility & context tracking
    pub ai_output_panel_visible: bool,
    pub ai_group_add_popup_open: bool,
    pub ai_group_add_name_input: String,
    pub ai_group_rename_popup_open: bool,
    pub ai_group_rename_target: Option<String>,
    pub ai_group_rename_input: String,
    pub ai_group_delete_popup_open: bool,
    pub ai_group_delete_target: Option<String>,
    pub ai_group_delete_target_category: Option<String>,
    pub ai_group_delete_target_sheet: Option<String>,
    pub ai_pending_structure_jobs: VecDeque<StructureSendJob>,
    pub ai_active_structure_job: Option<StructureSendJob>,
    pub ai_last_send_root_rows: Vec<usize>,
    pub ai_last_send_root_category: Option<String>,
    pub ai_last_send_root_sheet: Option<String>,
    pub ai_planned_structure_paths: Vec<Vec<usize>>,
    /// Phase 1 intermediate data - stored after initial AI call, before Phase 2 deep review
    pub ai_phase1_intermediate: Option<Phase1IntermediateData>,
    /// Flag indicating that the next AI result should be processed as Phase 2 deep review
    pub ai_expecting_phase2_result: bool,
    pub ai_structure_results_expected: usize,
    pub ai_structure_results_received: usize,
    pub ai_waiting_for_structure_results: bool,
    pub ai_structure_generation_counter: u64,
    pub ai_structure_active_generation: u64,
    /// Total count of AI tasks for progress tracking (Phase 1 + Phase 2 + structures)
    pub ai_total_tasks: usize,
    /// Completed AI tasks for progress tracking
    pub ai_completed_tasks: usize,

    // General Settings Popup
    pub show_settings_popup: bool,
    pub settings_new_api_key_input: String,
    pub was_settings_popup_open: bool, // Tracks previous state of settings popup

    // AI Rule (per-sheet AI Context) Popup state
    pub show_ai_rule_popup: bool,
    pub ai_rule_popup_needs_init: bool,
    pub ai_rule_popup_last_category: Option<Option<String>>, // tracks last category when popup opened
    pub ai_rule_popup_last_sheet: Option<String>,
    // Grounding toggle value while the AI Context popup is open
    pub ai_rule_popup_grounding: Option<bool>,

    // Table rendering helpers
    pub filtered_row_indices_cache: HashMap<(Option<String>, String), FilteredRowsCacheEntry>,
    pub force_filter_recalculation: bool,
    pub request_scroll_to_new_row: bool,
    pub scroll_to_row_index: Option<usize>,

    // Core Interaction Mode
    pub current_interaction_mode: SheetInteractionState,
    pub selected_columns_for_deletion: HashSet<usize>,
    pub column_drag_state: ColumnDragState,
    // Drag-and-drop of sheets between categories
    pub dragged_sheet: Option<(Option<String>, String)>,

    // NEW: Random Picker UI state (per-session)
    pub show_random_picker_panel: bool,
    pub random_picker_mode_is_complex: bool,
    pub random_simple_result_col: usize,
    pub random_complex_result_col: usize,
    pub random_complex_weight_col: Option<usize>,
    pub random_complex_second_weight_col: Option<usize>,
    // Dynamic list of optional weight columns for the Random Picker UI (auto-expand/contract)
    pub random_picker_weight_columns: Vec<Option<usize>>,
    // Parallel vector storing per-weight-column exponents. Length matches the number of Some(..) entries in random_picker_weight_columns
    pub random_picker_weight_exponents: Vec<f64>,
    // Parallel vector storing per-weight-column multipliers (applied before exponentiation)
    pub random_picker_weight_multipliers: Vec<f64>,
    pub random_picker_last_value: String,
    // Transient copy status shown after user clicks to copy the value
    pub random_picker_copy_status: String,
    // Ensure RP UI initializes once per selection (also on app startup)
    pub random_picker_needs_init: bool,

    // NEW: Summarizer UI state (per-session, not persisted yet)
    pub summarizer_selected_col: usize,
    pub summarizer_last_result: String, // Prefixed with Sum:/Count:
    // Transient copy status for summarizer result
    pub summarizer_copy_status: String,
    // Multiple selected columns for Summarizer when edited in the shared popup
    pub summarizer_selected_columns: Vec<Option<usize>>,

    // Confirmation dialogs
    pub pending_validator_change_requires_confirmation: bool,
    pub pending_validator_new_validator_summary: Option<String>,
    // NEW: store the validator choice awaiting confirmation (serialized summary & type flag)
    pub pending_validator_target_is_structure: bool,
    // Key Column (context-only) selection ephemeral states
    pub options_structure_key_parent_column_temp: Option<usize>, // during initial creation
    pub options_existing_structure_key_parent_column: Option<usize>, // editing existing structure
    // Number of context-only key columns prepended to last AI send
    pub ai_context_only_prefix_count: usize,
    // Pending apply structure key selection (category, sheet, structure_col_index, new key parent col)
    pub pending_structure_key_apply: Option<(Option<String>, String, usize, Option<usize>)>,
    // Stored context-only prefix values per row (for review UI display): Vec of (header, value)
    pub ai_context_prefix_by_row: HashMap<usize, Vec<(String, String)>>,

    // UI layout prefs (persisted): expand/collapse of pickers
    pub category_picker_expanded: bool,
    pub sheet_picker_expanded: bool,
    pub ai_groups_expanded: bool,

    // Edit Mode expanded row visibility (toolbar-expander)
    pub show_edit_mode_panel: bool,

    // UI alignment helpers (not persisted): store x positions where toggles were placed
    pub last_ai_button_min_x: f32,
    pub last_edit_mode_button_min_x: f32,
    pub last_toybox_button_min_x: f32,

    // Toybox (container for Random Picker + Summarizer)
    pub show_toybox_menu: bool,
    pub toybox_mode: ToyboxMode,
    // App-wide FPS setting controlled from Settings popup
    pub fps_setting: FpsSetting,
    // UI preference: show hidden sheets (persisted via AppSettings)
    pub show_hidden_sheets: bool,

    // Throttled apply queue for Accept All (row_index, col_index, new_value)
    pub ai_throttled_apply_queue: VecDeque<ThrottledAiAction>,
    // Cached flag: true if there are duplicate new rows needing a Merge/Separate decision
    pub ai_batch_has_undecided_merge: bool,
    // NEW: Prompt popup for zero-row AI request
    pub show_ai_prompt_popup: bool,
    pub ai_prompt_input: String,
    // Marker that last AI batch send was prompt-only (no original rows) so we treat incoming rows specially
    pub last_ai_prompt_only: bool,
    pub ai_cached_included_columns: Vec<bool>,
    pub ai_cached_included_structure_columns: Vec<bool>,
    pub ai_cached_included_columns_category: Option<String>,
    pub ai_cached_included_columns_sheet: Option<String>,
    pub ai_cached_included_columns_path: Vec<usize>,
    pub ai_cached_included_columns_dirty: bool,
    pub ai_cached_included_columns_valid: bool,
    /// Cache for structure row counts in hover previews to avoid per-frame recomputation
    /// Key: (category, structure_sheet_name, parent_row_index_in_root, structure_col_index, structure_path_len)
    pub ui_structure_row_count_cache:
        std::collections::HashMap<(Option<String>, String, usize, usize, usize), usize>,
    // Tracks the right edge of the last rendered header in content coordinates for Add Column placement
    pub last_header_right_edge_x: f32,
}

impl Default for EditorWindowState {
    fn default() -> Self {
        Self {
            selected_category: None,
            selected_sheet_name: None,
            virtual_structure_stack: Vec::new(),
            structure_navigation_stack: Vec::new(),
            show_rename_popup: false,
            rename_target_category: None,
            rename_target_sheet: String::new(),
            new_name_input: String::new(),
            show_delete_confirm_popup: false,
            delete_target_category: None,
            delete_target_sheet: String::new(),
            show_column_options_popup: false,
            options_column_target_category: None,
            options_column_target_sheet: String::new(),
            options_column_target_index: 0,
            column_options_popup_needs_init: false,
            options_column_rename_input: String::new(),
            options_column_filter_input: String::new(),
            options_column_filter_terms: vec![String::new()],
            options_column_ai_context_input: String::new(),
            options_validator_type: None,
            options_basic_type_select: ColumnDataType::String,
            options_link_target_sheet: None,
            options_link_target_column_index: None,
            options_structure_source_columns: vec![None],
            linked_column_cache: HashMap::new(),
            linked_column_cache_normalized: HashMap::new(),
            show_new_sheet_popup: false,
            new_sheet_name_input: String::new(),
            new_sheet_target_category: None,
            new_sheet_show_validation_hint: false,
            show_add_table_popup: false,
            show_new_category_popup: false,
            new_category_name_input: String::new(),
            show_delete_category_confirm_popup: false,
            delete_category_name: None,
            show_delete_category_double_confirm_popup: false,
            ai_mode: AiModeState::Idle,
            ai_selected_rows: HashSet::new(),
            ai_batch_review_active: false,
            ai_row_reviews: Vec::new(),
            ai_new_row_reviews: Vec::new(),
            ai_structure_reviews: Vec::new(),
            ai_structure_new_row_contexts: HashMap::new(),
            ai_structure_new_row_token_counter: 0,
            ai_original_row_snapshot_cache: HashMap::new(),
            ai_model_id_input: String::new(),
            ai_general_rule_input: String::new(),
            ai_structure_detail_context: None,

            ai_call_log: Vec::new(),
            ai_raw_output_display: String::new(),
            ai_output_panel_visible: false,
            ai_group_add_popup_open: false,
            ai_group_add_name_input: String::new(),
            ai_group_rename_popup_open: false,
            ai_group_rename_target: None,
            ai_group_rename_input: String::new(),
            ai_group_delete_popup_open: false,
            ai_group_delete_target: None,
            ai_group_delete_target_category: None,
            ai_group_delete_target_sheet: None,
            ai_pending_structure_jobs: VecDeque::new(),
            ai_active_structure_job: None,
            ai_last_send_root_rows: Vec::new(),
            ai_last_send_root_category: None,
            ai_last_send_root_sheet: None,
            ai_planned_structure_paths: Vec::new(),
            ai_phase1_intermediate: None,
            ai_expecting_phase2_result: false,
            ai_structure_results_expected: 0,
            ai_structure_results_received: 0,
            ai_waiting_for_structure_results: false,
            ai_structure_generation_counter: 0,
            ai_structure_active_generation: 0,
            ai_total_tasks: 0,
            ai_completed_tasks: 0,
            show_settings_popup: false,
            settings_new_api_key_input: String::new(),
            was_settings_popup_open: false,
            show_ai_rule_popup: false,
            ai_rule_popup_needs_init: false,
            ai_rule_popup_last_category: None,
            ai_rule_popup_last_sheet: None,
            ai_rule_popup_grounding: None,
            filtered_row_indices_cache: HashMap::new(),
            force_filter_recalculation: false,
            request_scroll_to_new_row: false,
            scroll_to_row_index: None,
            current_interaction_mode: SheetInteractionState::Idle,
            selected_columns_for_deletion: HashSet::new(),
            column_drag_state: ColumnDragState::default(),
            dragged_sheet: None,

            show_random_picker_panel: false,
            random_picker_mode_is_complex: false,
            random_simple_result_col: 0,
            random_complex_result_col: 0,
            random_complex_weight_col: None,
            random_complex_second_weight_col: None,
            random_picker_weight_columns: vec![None],
            random_picker_weight_exponents: vec![1.0],
            random_picker_weight_multipliers: vec![1.0],
            random_picker_last_value: String::new(),
            random_picker_copy_status: String::new(),
            random_picker_needs_init: true,
            summarizer_selected_col: 0,
            summarizer_last_result: String::new(),
            summarizer_copy_status: String::new(),
            summarizer_selected_columns: vec![None],
            pending_validator_change_requires_confirmation: false,
            pending_validator_new_validator_summary: None,
            pending_validator_target_is_structure: false,
            options_structure_key_parent_column_temp: None,
            options_existing_structure_key_parent_column: None,
            ai_context_only_prefix_count: 0,
            pending_structure_key_apply: None,
            ai_context_prefix_by_row: HashMap::new(),
            category_picker_expanded: true,
            sheet_picker_expanded: true,
            ai_groups_expanded: true,
            show_edit_mode_panel: false,
            last_ai_button_min_x: 0.0,
            last_edit_mode_button_min_x: 0.0,
            last_toybox_button_min_x: 0.0,
            show_toybox_menu: false,
            toybox_mode: ToyboxMode::Randomizer,
            fps_setting: FpsSetting::default(),
            show_hidden_sheets: false,
            ai_throttled_apply_queue: VecDeque::new(),
            ai_batch_has_undecided_merge: false,
            show_ai_prompt_popup: false,
            ai_prompt_input: String::new(),
            last_ai_prompt_only: false,
            ai_cached_included_columns: Vec::new(),
            ai_cached_included_structure_columns: Vec::new(),
            ai_cached_included_columns_category: None,
            ai_cached_included_columns_sheet: None,
            ai_cached_included_columns_path: Vec::new(),
            ai_cached_included_columns_dirty: true,
            ai_cached_included_columns_valid: false,
            ui_structure_row_count_cache: std::collections::HashMap::new(),
            last_header_right_edge_x: 0.0,
        }
    }
}
