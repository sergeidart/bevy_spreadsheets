// src/ui/elements/editor/state.rs
// NO CHANGES needed for the validation refactor, file remains as is.
use crate::sheets::definitions::ColumnDataType;
use crate::sheets::SheetRegistry; // Keep for cache invalidation helper maybe?
use bevy::log::debug;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiModeState {
    #[default] Idle, Preparing, Submitting, ResultsReady, Reviewing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidatorTypeChoice {
    #[default] Basic, Linked,
}

fn calculate_filters_hash(filters: &Vec<Option<String>>) -> u64 {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    filters.hash(&mut s);
    s.finish()
}

// Local state for the editor window (doesn't need Serialize/Deserialize)
#[derive(Default)]
pub struct EditorWindowState {
    // General State
    pub selected_category: Option<String>,
    pub selected_sheet_name: Option<String>,

    // Rename Popup State
    pub show_rename_popup: bool,
    pub rename_target_category: Option<String>,
    pub rename_target_sheet: String,
    pub new_name_input: String,

    // Delete Popup State
    pub show_delete_confirm_popup: bool,
    pub delete_target_category: Option<String>,
    pub delete_target_sheet: String,

    // Column Options Popup State
    pub show_column_options_popup: bool,
    pub options_column_target_category: Option<String>,
    pub options_column_target_sheet: String,
    pub options_column_target_index: usize,
    pub column_options_popup_needs_init: bool,
    pub options_column_rename_input: String,
    pub options_column_filter_input: String,
    pub options_column_ai_context_input: String, // NEW: AI Context Input
    pub options_validator_type: Option<ValidatorTypeChoice>,
    pub options_basic_type_select: ColumnDataType,
    pub options_link_target_sheet: Option<String>,
    pub options_link_target_column_index: Option<usize>,

    // Cache for linked column dropdown options
    pub linked_column_cache: HashMap<(String, usize), HashSet<String>>,

    // AI Interaction State
    pub ai_mode: AiModeState,
    pub ai_selected_rows: HashSet<usize>,
    pub ai_suggestions: HashMap<usize, Vec<String>>,
    pub ai_review_queue: Vec<usize>,
    pub ai_current_review_index: Option<usize>,
    // AI Rule State
    pub ai_general_rule_input: String,
    pub show_ai_rule_popup: bool,

    // Settings State
    pub show_settings_popup: bool,
    pub settings_new_api_key_input: String,
    pub settings_api_key_status: String,

    // Filter Cache
    // Key: (Category, SheetName, FiltersHash) -> Value: Vec of original row indices
    pub filtered_row_indices_cache: HashMap<(Option<String>, String, u64), Vec<usize>>,
    // Flag to force recalculation when data changes or filters change significantly
    pub force_filter_recalculation: bool,
}

impl EditorWindowState {
    // Helper method to invalidate the filter cache for the currently selected sheet
    // Note: This only affects the *row filtering*, not the cell validation state.
    pub fn invalidate_current_sheet_filter_cache(&mut self, registry: &SheetRegistry) {
        if let Some(sheet_name) = &self.selected_sheet_name {
            let cat = self.selected_category.clone();
            let name = sheet_name.clone();
            // Remove specific hash first if metadata available
            if let Some(sheet_data) = registry.get_sheet(&cat, &name) {
                 if let Some(metadata) = &sheet_data.metadata {
                    let filters_hash = calculate_filters_hash(&metadata.get_filters());
                    let cache_key = (cat.clone(), name.clone(), filters_hash);
                    self.filtered_row_indices_cache.remove(&cache_key);
                }
            }
            // Also perform broad invalidation just in case hash wasn't right or metadata unavailable
            self.filtered_row_indices_cache.retain(|(s_cat, s_name, _), _| {
                !(*s_cat == cat && *s_name == name)
            });
            self.force_filter_recalculation = true;
            debug!("Invalidated filter cache for sheet: '{:?}/{}'", cat, name);
        }
    }
}