// src/ui/elements/editor/state.rs
use bevy::prelude::Resource; // Resource isn't actually used here, Local is used in system
use std::collections::{HashMap, HashSet}; // Add HashSet import
// Corrected import path
use crate::sheets::definitions::{ColumnDataType, ColumnValidator};

// Define the local state for the editor window
// Using Local<T> in the system. It doesn't need Serialize/Deserialize.
#[derive(Default)] // No serde derive needed for Local state
pub struct EditorWindowState {
    // General State
    pub selected_sheet_name: Option<String>,

    // Rename Popup State
    pub show_rename_popup: bool,
    pub rename_target: String,
    pub new_name_input: String,

    // Delete Popup State
    pub show_delete_confirm_popup: bool,
    pub delete_target: String,

    // Column Options Popup State
    pub show_column_options_popup: bool,
    pub options_column_target_sheet: String,
    pub options_column_target_index: usize,
    pub column_options_popup_needs_init: bool, // Flag to init fields on open

    // Fields for Column Options Popup UI selections
    pub options_column_rename_input: String,
    pub options_column_filter_input: String,
    // -- Validator selection state --
    pub options_validator_type: Option<ValidatorTypeChoice>, // Radio button choice
    pub options_basic_type_select: ColumnDataType, // Selected basic type
    pub options_link_target_sheet: Option<String>, // Selected target sheet name for linking
    pub options_link_target_column_index: Option<usize>, // Selected target column index

    // --- Cache for linked column dropdown options ---
    // Key: (Target Sheet Name, Target Column Index)
    // Value: HashSet of unique non-empty string values from that target column for fast lookups
    // Displayed suggestions will be generated and sorted from this set.
    // #[serde(skip)] // REMOVED - This struct isn't serialized
    pub linked_column_cache: HashMap<(String, usize), HashSet<String>>, // Use HashSet for faster lookups

}

// Enum for Validator Choice Radio Buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidatorTypeChoice {
    #[default]
    Basic,
    Linked,
}