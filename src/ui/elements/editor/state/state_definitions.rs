// src/ui/elements/editor/state_definitions.rs
// Type definitions and enums for editor state

// Re-export types from sibling modules
pub use super::ai_types::*;
pub use super::navigation_types::*;
pub use super::review_types::*;
pub use super::ui_types::*;

use crate::sheets::definitions::ColumnDataType;
use bevy::prelude::Resource;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Resource)]
pub struct EditorWindowState {
    pub selected_category: Option<String>,
    pub selected_sheet_name: Option<String>,

    // Stack for real structure sheet navigation with hidden filters
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
    /// Ephemeral hidden checkbox state for Column Options popup
    pub options_column_hidden_input: bool,
    pub options_validator_type: Option<ValidatorTypeChoice>,
    pub options_basic_type_select: ColumnDataType,
    pub options_link_target_sheet: Option<String>,
    pub options_link_target_column_index: Option<usize>,
    // NEW: Structure selection chain (always at least length 1 with possibly None meaning no selection yet)
    pub options_structure_source_columns: Vec<Option<usize>>,
    pub linked_column_cache: LinkedColumnCache,
    // Normalized (lowercased, CR/LF removed) mirror of linked_column_cache for O(1) membership
    pub linked_column_cache_normalized: LinkedColumnCacheNormalized,

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
    pub ai_original_row_snapshot_cache: HashMap<(Option<usize>, Option<usize>), Vec<String>>,
    // New unified review model
    // (legacy single-row review fields removed)
    pub ai_model_id_input: String,
    pub ai_general_rule_input: String,

    // Structure detail navigation context (when user dives into a structure review)
    pub ai_structure_detail_context: Option<StructureDetailContext>,

    // NEW: Navigation-based AI review (replaces structure detail branching)
    /// Navigation breadcrumb stack - each level represents a sheet we've drilled into
    pub ai_navigation_stack: Vec<NavigationContext>,
    /// Current sheet being reviewed (can be parent or child table)
    pub ai_current_sheet: String,
    /// Current category being reviewed
    pub ai_current_category: Option<String>,
    /// Optional filter for child table reviews (filters by parent_key)
    pub ai_parent_filter: Option<ParentFilter>,
    /// Map of (row_idx, col_idx) -> status for structure column drill-down indicators
    #[allow(dead_code)]
    pub ai_pending_structure_drilldowns: HashMap<(usize, usize), StructureStatus>,

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
    /// Batch processing context - stored when processing AI results for structure job enqueueing
    pub ai_batch_context: Option<BatchProcessingContext>,
    pub ai_structure_results_expected: usize,
    pub ai_structure_results_received: usize,
    pub ai_waiting_for_structure_results: bool,
    pub ai_structure_generation_counter: u64,
    pub ai_structure_active_generation: u64,
    /// Flag to trigger loading of structure child tables when AI Review starts
    pub ai_needs_structure_child_tables_loaded: bool,
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
    /// Flag to trigger cache reload from DB when switching sheets
    pub force_cache_reload: bool,
    pub scroll_to_row_index: Option<usize>,
    
    /// Parent lineage cache: (category, sheet_name, row_index) -> Vec<(table_name, display_value, row_index)>
    /// Cleared when switching databases or categories to ensure fresh data
    pub parent_lineage_cache: HashMap<(Option<String>, String, usize), Vec<(String, String, usize)>>,

    /// Flag to indicate that the selected category needs its table list loaded (lazy loading)
    pub category_needs_table_list_load: bool,
    
    /// Flag to indicate that a sheet is currently loading (prevents rendering empty state)
    pub sheet_is_loading: bool,

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
    pub pending_validator_change_requires_confirmation: bool,
    pub pending_validator_new_validator_summary: Option<String>,
    pub pending_validator_target_is_structure: bool,
    // Key Column (context-only) selection ephemeral states
    pub options_structure_key_parent_column_temp: Option<usize>, // during initial creation
    pub options_existing_structure_key_parent_column: Option<usize>, // editing existing structure
    pub ai_context_only_prefix_count: usize,
    pub pending_structure_key_apply: Option<(Option<String>, String, usize, Option<usize>)>,
    // Stored context-only prefix values per row (for review UI display): Vec of (header, value)
    pub ai_context_prefix_by_row: HashMap<usize, Vec<(String, String)>>,
    pub category_picker_expanded: bool,
    pub sheet_picker_expanded: bool,
    pub ai_groups_expanded: bool,
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
    pub show_hidden_sheets: bool,
    /// AI depth limit: how many levels of structure tables to process (default: 2)
    pub ai_depth_limit: usize,
    /// AI width limit: how many rows to send in one batch (default: 32)
    pub ai_width_limit: usize,
    pub ai_throttled_apply_queue: VecDeque<ThrottledAiAction>,
    pub ai_throttled_batch_add_queue: VecDeque<(Option<String>, String, Vec<Vec<(usize, String)>>)>,
    pub ai_batch_has_undecided_merge: bool,
    pub show_ai_prompt_popup: bool,
    pub ai_prompt_input: String,
    pub last_ai_prompt_only: bool,
    pub ai_cached_included_columns: Vec<bool>,
    pub ai_cached_included_structure_columns: Vec<bool>,
    pub ai_cached_included_columns_category: Option<String>,
    pub ai_cached_included_columns_sheet: Option<String>,
    pub ai_cached_included_columns_path: Vec<usize>,
    pub ai_cached_included_columns_dirty: bool,
    pub ai_cached_included_columns_valid: bool,
    pub ui_structure_row_count_cache:
        std::collections::HashMap<(Option<String>, String, usize, usize, usize), usize>,
    // Tracks the right edge of the last rendered header in content coordinates for Add Column placement
    pub last_header_right_edge_x: f32,
    
    // Flag to trigger revalidation when a sheet is opened/re-opened
    pub pending_sheet_revalidation: bool,
    
    // Structure table recreation popup state
    pub show_structure_recreation_popup: bool,
    pub structure_recreation_category: Option<String>,
    pub structure_recreation_sheet_name: String,
    pub structure_recreation_parent_sheet_name: String,
    pub structure_recreation_parent_col_def: Option<crate::sheets::definitions::ColumnDefinition>,
    pub structure_recreation_struct_columns: Vec<crate::sheets::definitions::ColumnDefinition>,
}
