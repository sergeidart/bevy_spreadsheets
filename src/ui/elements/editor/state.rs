// src/ui/elements/editor/state.rs
use crate::sheets::definitions::ColumnDataType;
use bevy::prelude::Resource;
use std::collections::{HashMap, HashSet};

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
    ColumnModeActive,
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

#[derive(Debug, Clone, Default)]
pub struct ColumnDragState {
    pub source_index: Option<usize>, 
}


#[derive(Resource)]
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
    pub linked_column_cache: HashMap<(String, usize), HashSet<String>>,

    // NEW: State for New Sheet Popup
    pub show_new_sheet_popup: bool,
    pub new_sheet_name_input: String, // Re-using new_name_input is an option, but separate is cleaner
    pub new_sheet_target_category: Option<String>,


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
    pub ai_temperature_input: f32,
    pub ai_top_k_input: i32,
    pub ai_top_p_input: f32,
    pub show_ai_rule_popup: bool,
    pub ai_rule_popup_needs_init: bool,
    pub ai_raw_output_display: String,
    // Bottom AI output panel visibility & context tracking
    pub ai_output_panel_visible: bool,
    pub ai_output_panel_last_context: Option<(Option<String>, String, bool)>, // (category, sheet, in_structure)

    // General Settings Popup
    pub show_settings_popup: bool, 
    pub settings_new_api_key_input: String,
    pub was_settings_popup_open: bool, // Tracks previous state of settings popup

    // Table rendering helpers
    pub filtered_row_indices_cache: HashMap<(Option<String>, String, u64), Vec<usize>>,
    pub force_filter_recalculation: bool,
    pub request_scroll_to_new_row: bool,
    pub scroll_to_row_index: Option<usize>,
    
    // UI Toggles
    pub show_quick_copy_bar: bool,

    // Core Interaction Mode
    pub current_interaction_mode: SheetInteractionState,
    pub selected_columns_for_deletion: HashSet<usize>,
    pub column_drag_state: ColumnDragState,

    // NEW: Fields for AI Rule Popup context
    pub ai_rule_popup_last_category: Option<String>,
    pub ai_rule_popup_last_sheet: Option<String>,

    // NEW: Random Picker UI state (per-session)
    pub show_random_picker_panel: bool,
    pub random_picker_mode_is_complex: bool,
    pub random_simple_result_col: usize,
    pub random_complex_result_col: usize,
    pub random_complex_weight_col: Option<usize>,
    pub random_complex_second_weight_col: Option<usize>,
    pub random_picker_last_value: String,
    // Ensure RP UI initializes once per selection (also on app startup)
    pub random_picker_needs_init: bool,

    // NEW: Summarizer UI state (per-session, not persisted yet)
    pub show_summarizer_panel: bool,
    pub summarizer_selected_col: usize,
    pub summarizer_last_result: String, // Prefixed with Sum:/Count:

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
        // Removed per-sheet override cache: flag now read directly from persisted metadata.
        pub effective_ai_can_add_rows: Option<bool>,
    // Optimistic pending toggle for AI row generation (root sheet) to avoid UI flicker while event processes
    pub pending_ai_row_generation_toggle: Option<(Option<String>, String, bool)>, // (root_category, root_sheet, desired_flag)
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
            show_new_sheet_popup: false,
            new_sheet_name_input: String::new(),
            new_sheet_target_category: None,
            ai_mode: AiModeState::Idle,
            ai_selected_rows: HashSet::new(),
            ai_batch_review_active: false,
            ai_row_reviews: Vec::new(),
            ai_new_row_reviews: Vec::new(),
            ai_model_id_input: String::new(),
            ai_general_rule_input: String::new(),
            ai_temperature_input: 0.7,
            ai_top_k_input: 40,
            ai_top_p_input: 0.9,
            show_ai_rule_popup: false,
            ai_rule_popup_needs_init: false,
            ai_raw_output_display: String::new(),
            ai_output_panel_visible: false,
            ai_output_panel_last_context: None,
            show_settings_popup: false,
            settings_new_api_key_input: String::new(),
            was_settings_popup_open: false,
            filtered_row_indices_cache: HashMap::new(),
            force_filter_recalculation: false,
            request_scroll_to_new_row: false,
            scroll_to_row_index: None,
            show_quick_copy_bar: false,
            current_interaction_mode: SheetInteractionState::Idle,
            selected_columns_for_deletion: HashSet::new(),
            column_drag_state: ColumnDragState::default(),
            ai_rule_popup_last_category: None,
            ai_rule_popup_last_sheet: None,
            show_random_picker_panel: false,
            random_picker_mode_is_complex: false,
            random_simple_result_col: 0,
            random_complex_result_col: 0,
            random_complex_weight_col: None,
            random_complex_second_weight_col: None,
            random_picker_last_value: String::new(),
            random_picker_needs_init: true,
            show_summarizer_panel: false,
            summarizer_selected_col: 0,
            summarizer_last_result: String::new(),
            pending_validator_change_requires_confirmation: false,
            pending_validator_new_validator_summary: None,
            pending_validator_target_is_structure: false,
            options_structure_key_parent_column_temp: None,
            options_existing_structure_key_parent_column: None,
            ai_context_only_prefix_count: 0,
            pending_structure_key_apply: None,
            ai_context_prefix_by_row: HashMap::new(),
            effective_ai_can_add_rows: None,
            pending_ai_row_generation_toggle: None,
        }
    }
}

#[derive(Debug, Clone)]
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
pub struct NewRowReview {
    pub ai: Vec<String>,
    pub non_structure_columns: Vec<usize>,
    pub accept: bool,
}

#[derive(Debug, Clone)]
pub struct VirtualStructureContext {
    pub virtual_sheet_name: String,
    pub parent: StructureParentContext,
}

impl EditorWindowState {
    // Returns the currently active sheet context considering virtual structure navigation.
    // If inside a virtual structure view, returns that virtual sheet name and its parent category.
    // Otherwise returns the user's selected (category, sheet) pair.
    pub fn current_sheet_context(&self) -> (Option<String>, Option<String>) {
        if let Some(vctx) = self.virtual_structure_stack.last() {
            return (vctx.parent.parent_category.clone(), Some(vctx.virtual_sheet_name.clone()));
        }
        (self.selected_category.clone(), self.selected_sheet_name.clone())
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
    }

    /// Resolve ultimate root sheet (category, sheet) for current view (following structure parents).
    pub fn resolve_root_sheet(&self, registry: &crate::sheets::resources::SheetRegistry) -> (Option<String>, Option<String>) {
        if let Some(vctx) = self.virtual_structure_stack.last() {
            let mut current_category = self.selected_category.clone();
            let mut current_sheet = vctx.virtual_sheet_name.clone();
            let mut safety = 0;
            while safety < 32 {
                safety += 1;
                if let Some(meta) = registry.get_sheet(&current_category, &current_sheet).and_then(|s| s.metadata.as_ref()) {
                    if let Some(parent) = &meta.structure_parent { current_category = parent.parent_category.clone(); current_sheet = parent.parent_sheet.clone(); continue; }
                }
                break;
            }
            return (current_category, Some(current_sheet));
        }
        (self.selected_category.clone(), self.selected_sheet_name.clone())
    }

    /// Compute effective AI row-generation permission (root sheet meta + override). Returns None if no meta.
    pub fn effective_ai_add_rows(&self, registry: &crate::sheets::resources::SheetRegistry) -> Option<bool> {
        let (cat, sheet_opt) = self.resolve_root_sheet(registry);
        let sheet = sheet_opt?;
        let meta_opt = registry.get_sheet(&cat, &sheet).and_then(|s| s.metadata.as_ref());
    registry.get_sheet(&cat, &sheet).and_then(|s| s.metadata.as_ref()).map(|m| m.ai_enable_row_generation)
    }
}

// Removed StructureViewData (overlay approach) in favor of virtual sheets