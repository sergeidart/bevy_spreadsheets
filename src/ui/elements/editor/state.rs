// src/ui/elements/editor/state.rs
use crate::sheets::definitions::ColumnDataType;
use crate::sheets::SheetRegistry;
use bevy::log::debug;
use bevy::prelude::Resource;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewChoice {
    Original,
    AI,
}

fn calculate_filters_hash(filters: &Vec<Option<String>>) -> u64 {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    filters.hash(&mut s);
    s.finish()
}

#[derive(Debug, Clone, Default)]
pub struct ColumnDragState {
    pub source_index: Option<usize>, 
}


#[derive(Default, Resource)]
pub struct EditorWindowState {
    pub selected_category: Option<String>,
    pub selected_sheet_name: Option<String>,
    
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
    pub options_column_ai_context_input: String,
    pub options_validator_type: Option<ValidatorTypeChoice>,
    pub options_basic_type_select: ColumnDataType,
    pub options_link_target_sheet: Option<String>,
    pub options_link_target_column_index: Option<usize>,
    pub linked_column_cache: HashMap<(String, usize), HashSet<String>>,

    // NEW: State for New Sheet Popup
    pub show_new_sheet_popup: bool,
    pub new_sheet_name_input: String, // Re-using new_name_input is an option, but separate is cleaner
    pub new_sheet_target_category: Option<String>,


    // AI Mode specific state
    pub ai_mode: AiModeState,
    pub ai_selected_rows: HashSet<usize>, 
    pub ai_suggestions: HashMap<usize, Vec<String>>,
    pub ai_review_queue: Vec<usize>,
    pub ai_current_review_index: Option<usize>,
    pub current_ai_suggestion_edit_buffer: Option<(usize, Vec<String>)>,
    pub ai_review_column_choices: Vec<ReviewChoice>,

    pub ai_model_id_input: String,
    pub ai_general_rule_input: String,
    pub ai_temperature_input: f32,
    pub ai_top_k_input: i32,
    pub ai_top_p_input: f32,
    pub show_ai_rule_popup: bool,
    pub ai_rule_popup_needs_init: bool,
    pub ai_prompt_display: String,
    pub ai_raw_output_display: String,

    // General Settings Popup
    pub show_settings_popup: bool, 
    pub settings_new_api_key_input: String,

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
}

impl EditorWindowState {
    pub fn invalidate_current_sheet_filter_cache(&mut self, registry: &SheetRegistry) {
        if let Some(sheet_name) = &self.selected_sheet_name {
            let cat = self.selected_category.clone();
            let name = sheet_name.clone();
            if let Some(sheet_data) = registry.get_sheet(&cat, &name) {
                if let Some(metadata) = &sheet_data.metadata {
                    let filters_hash = calculate_filters_hash(&metadata.get_filters());
                    let cache_key = (cat.clone(), name.clone(), filters_hash);
                    self.filtered_row_indices_cache.remove(&cache_key);
                }
            }
            self.force_filter_recalculation = true;
            debug!("Invalidated filter cache for sheet: '{:?}/{}'", cat, name);
        }
    }

    pub fn reset_interaction_modes_and_selections(&mut self) {
        self.current_interaction_mode = SheetInteractionState::Idle;
        self.ai_mode = AiModeState::Idle; 
        self.ai_selected_rows.clear();
        self.selected_columns_for_deletion.clear();
        
        self.ai_suggestions.clear();
        self.ai_review_queue.clear();
        self.current_ai_suggestion_edit_buffer = None;
        self.ai_review_column_choices.clear();
        self.ai_current_review_index = None;

        self.column_drag_state = ColumnDragState::default();
    }
}