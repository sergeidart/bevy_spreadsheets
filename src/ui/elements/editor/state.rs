// src/ui/elements/editor/state.rs
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

#[derive(Resource)]
#[allow(dead_code)]
pub struct EditorWindowState {
    pub selected_category: Option<String>,
    pub selected_sheet_name: Option<String>,
    // Stack for nested structure navigation (root at index 0)
    pub sheet_nav_stack: Vec<(Option<String>, String)>,
    // Stack of active virtual structure sheets (each represents a nested structure view)
    // Top of stack is current virtual sheet. Empty means not in structure view.
    pub virtual_structure_stack: Vec<VirtualStructureContext>,

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
    // New unified review model
    // (legacy single-row review fields removed)
    pub ai_model_id_input: String,
    pub ai_general_rule_input: String,

    pub ai_raw_output_display: String,
    // Bottom AI output panel visibility & context tracking
    pub ai_output_panel_visible: bool,
    pub ai_output_panel_last_context: Option<(Option<String>, String, bool)>, // (category, sheet, in_structure)
    pub ai_group_add_popup_open: bool,
    pub ai_group_add_name_input: String,
    pub ai_group_rename_popup_open: bool,
    pub ai_group_rename_target: Option<String>,
    pub ai_group_rename_input: String,

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

    // UI Toggles
    pub show_quick_copy_bar: bool,

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
    pub show_summarizer_panel: bool,
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
    pub ai_cached_included_columns_category: Option<String>,
    pub ai_cached_included_columns_sheet: Option<String>,
    pub ai_cached_included_columns_path: Vec<usize>,
    pub ai_cached_included_columns_dirty: bool,
    pub ai_cached_included_columns_valid: bool,
}

impl Default for EditorWindowState {
    fn default() -> Self {
        Self {
            selected_category: None,
            selected_sheet_name: None,
            sheet_nav_stack: Vec::new(),
            virtual_structure_stack: Vec::new(),
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
            ai_model_id_input: String::new(),
            ai_general_rule_input: String::new(),

            ai_raw_output_display: String::new(),
            ai_output_panel_visible: false,
            ai_output_panel_last_context: None,
            ai_group_add_popup_open: false,
            ai_group_add_name_input: String::new(),
            ai_group_rename_popup_open: false,
            ai_group_rename_target: None,
            ai_group_rename_input: String::new(),
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
            show_quick_copy_bar: true,
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
            show_summarizer_panel: false,
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
            show_edit_mode_panel: false,
            last_ai_button_min_x: 0.0,
            last_edit_mode_button_min_x: 0.0,
            last_toybox_button_min_x: 0.0,
            show_toybox_menu: false,
            toybox_mode: ToyboxMode::Randomizer,
            fps_setting: FpsSetting::default(),
            ai_throttled_apply_queue: VecDeque::new(),
            ai_batch_has_undecided_merge: false,
            show_ai_prompt_popup: false,
            ai_prompt_input: String::new(),
            last_ai_prompt_only: false,
            ai_cached_included_columns: Vec::new(),
            ai_cached_included_columns_category: None,
            ai_cached_included_columns_sheet: None,
            ai_cached_included_columns_path: Vec::new(),
            ai_cached_included_columns_dirty: true,
            ai_cached_included_columns_valid: false,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StructureParentContext {
    pub parent_category: Option<String>,
    pub parent_sheet: String,
    pub parent_row: usize,
    pub parent_col: usize,
    pub parent_column_header: String,
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
#[allow(dead_code)]
pub struct NewRowReview {
    pub ai: Vec<String>,
    pub non_structure_columns: Vec<usize>,
    pub accept: bool,
    // If Some(row_index) then this new row's first column matches an existing row's first column
    // allowing user to choose between merging into that row or adding a separate row.
    pub duplicate_match_row: Option<usize>,
    // Snapshot of the matched original row's values for the same non_structure_columns (only if duplicate_match_row is Some)
    pub original_for_merge: Option<Vec<String>>,
    // Per-column review choices (only meaningful when merging). Mirrors RowReview. Length == non_structure_columns.len()
    pub choices: Option<Vec<ReviewChoice>>,
    // Whether the user currently has the Merge option selected (pre-decision). Defaults true if duplicate_match_row present.
    pub merge_selected: bool,
    // Whether the user clicked Decide (once decided Accept/Cancel replace Decide and toggle is hidden)
    pub merge_decided: bool,
}

#[derive(Debug, Clone)]
pub struct VirtualStructureContext {
    pub virtual_sheet_name: String,
    pub parent: StructureParentContext,
}

#[allow(dead_code)]
impl EditorWindowState {
    // Returns the currently active sheet context considering virtual structure navigation.
    // If inside a virtual structure view, returns that virtual sheet name and its parent category.
    // Otherwise returns the user's selected (category, sheet) pair.
    pub fn current_sheet_context(&self) -> (Option<String>, Option<String>) {
        if let Some(vctx) = self.virtual_structure_stack.last() {
            return (
                vctx.parent.parent_category.clone(),
                Some(vctx.virtual_sheet_name.clone()),
            );
        }
        (
            self.selected_category.clone(),
            self.selected_sheet_name.clone(),
        )
    }

    pub fn reset_interaction_modes_and_selections(&mut self) {
        self.current_interaction_mode = SheetInteractionState::Idle;
        self.ai_mode = AiModeState::Idle;
        self.ai_selected_rows.clear();
        self.selected_columns_for_deletion.clear();
        // Legacy single-row / multi-map AI review fields removed.

        self.column_drag_state = ColumnDragState::default();

        // Ensure structure source columns chain has at least one entry
        if self.options_structure_source_columns.is_empty() {
            self.options_structure_source_columns.push(None);
        }

        self.pending_validator_change_requires_confirmation = false;
        self.pending_validator_new_validator_summary = None;
        self.pending_validator_target_is_structure = false;

        // NOTE: virtual structure stack intentionally preserved so user can back out after mode changes
        // Keep random picker visible state as-is across mode changes.

        self.ai_group_add_popup_open = false;
        self.ai_group_add_name_input.clear();
        self.ai_group_rename_popup_open = false;
        self.ai_group_rename_target = None;
        self.ai_group_rename_input.clear();

        self.mark_ai_included_columns_dirty();
    }

    pub fn mark_ai_included_columns_dirty(&mut self) {
        self.ai_cached_included_columns_dirty = true;
        self.ai_cached_included_columns_valid = false;
    }

    pub fn ensure_ai_included_columns_cache(
        &mut self,
        registry: &crate::sheets::resources::SheetRegistry,
        category: &Option<String>,
        sheet_name: &str,
    ) {
        let path_matches = self.ai_cached_included_columns_path.len()
            == self.virtual_structure_stack.len()
            && self
                .virtual_structure_stack
                .iter()
                .zip(self.ai_cached_included_columns_path.iter())
                .all(|(ctx, cached)| ctx.parent.parent_col == *cached);

        let needs_rebuild = self.ai_cached_included_columns_dirty
            || self.ai_cached_included_columns_category.as_ref() != category.as_ref()
            || self.ai_cached_included_columns_sheet.as_deref() != Some(sheet_name)
            || !path_matches
            || !self.ai_cached_included_columns_valid;

        if !needs_rebuild {
            self.ai_cached_included_columns_valid = true;
            return;
        }

        self.ai_cached_included_columns_category = category.clone();
        self.ai_cached_included_columns_sheet = Some(sheet_name.to_string());
        self.ai_cached_included_columns_path.clear();
        self.ai_cached_included_columns_path.extend(
            self.virtual_structure_stack
                .iter()
                .map(|ctx| ctx.parent.parent_col),
        );
        self.ai_cached_included_columns_dirty = false;
        self.ai_cached_included_columns_valid = false;

        if let Some(meta) = registry
            .get_sheet(category, sheet_name)
            .and_then(|s| s.metadata.as_ref())
        {
            self.ai_cached_included_columns.clear();
            self.ai_cached_included_columns
                .resize(meta.columns.len(), false);
            for (idx, column) in meta.columns.iter().enumerate() {
                if matches!(column.validator, Some(ColumnValidator::Structure)) {
                    continue;
                }
                if !matches!(column.ai_include_in_send, Some(false)) {
                    if let Some(flag) = self.ai_cached_included_columns.get_mut(idx) {
                        *flag = true;
                    }
                }
            }
            self.ai_cached_included_columns_valid = true;
        } else {
            self.ai_cached_included_columns.clear();
        }
    }

    /// Resolve ultimate root sheet (category, sheet) for current view (following structure parents).
    pub fn resolve_root_sheet(
        &self,
        registry: &crate::sheets::resources::SheetRegistry,
    ) -> (Option<String>, Option<String>) {
        if let Some(vctx) = self.virtual_structure_stack.last() {
            let mut current_category = self.selected_category.clone();
            let mut current_sheet = vctx.virtual_sheet_name.clone();
            let mut safety = 0;
            while safety < 32 {
                safety += 1;
                if let Some(meta) = registry
                    .get_sheet(&current_category, &current_sheet)
                    .and_then(|s| s.metadata.as_ref())
                {
                    if let Some(parent) = &meta.structure_parent {
                        current_category = parent.parent_category.clone();
                        current_sheet = parent.parent_sheet.clone();
                        continue;
                    }
                }
                break;
            }
            return (current_category, Some(current_sheet));
        }
        (
            self.selected_category.clone(),
            self.selected_sheet_name.clone(),
        )
    }

    /// Compute effective AI row-generation permission (root sheet meta + override). Returns None if no meta.
    pub fn effective_ai_add_rows(
        &self,
        registry: &crate::sheets::resources::SheetRegistry,
    ) -> Option<bool> {
        let (cat, sheet_opt) = self.resolve_root_sheet(registry);
        let sheet = sheet_opt?;
        registry
            .get_sheet(&cat, &sheet)
            .and_then(|s| s.metadata.as_ref())
            .map(|m| m.ai_enable_row_generation)
    }
}

// Removed StructureViewData (overlay approach) in favor of virtual sheets
