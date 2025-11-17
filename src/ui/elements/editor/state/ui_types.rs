// UI-related type definitions for editor state

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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

#[derive(Debug, Clone, Default)]
pub struct ColumnDragState {
    pub source_index: Option<usize>,
}

/// Linked column cache: (sheet_name, column_index) -> set of valid values
pub type LinkedColumnCache = HashMap<(String, usize), Arc<HashSet<String>>>;

/// Normalized linked column cache: (sheet_name, column_index) -> set of normalized valid values
pub type LinkedColumnCacheNormalized = HashMap<(String, usize), Arc<HashSet<String>>>;
