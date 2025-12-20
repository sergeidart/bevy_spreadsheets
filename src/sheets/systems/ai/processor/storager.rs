// src/sheets/systems/ai/processor/storager.rs
//! Result Storage System
//!
//! This module persists AI results with proper index mapping.
//! It accumulates results across multi-step processing and
//! provides the data needed for the AI Review panel.
//!
//! ## Responsibilities
//!
//! - Store parsed AI responses linked to StableRowIds
//! - Support multi-step result accumulation
//! - Track row categories (Original, AiAdded, Lost)
//! - Provide data for AI Review display
//! - Clear on cancel/complete

use std::collections::HashMap;
use super::navigator::StableRowId;

/// Category of a row result - determines how it should be handled
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowCategory {
    /// Row was in original data and found in AI response - update existing
    Original,
    /// Row was created by AI (not in original data) - insert new
    AiAdded,
    /// Row was sent but not returned by AI - no change
    Lost,
    /// Row was returned by AI but its parent prefix doesn't match any known parent
    /// These rows are displayed in the AI Review for re-parenting via drag-drop/dropdown
    Orphaned,
}

/// Result for a single column in a row
#[derive(Debug, Clone)]
pub struct ColumnResult {
    /// Column index in the table
    pub column_index: usize,
    /// Column name/header
    pub column_name: String,
    /// Original value (if applicable, empty for AI-added rows)
    pub original_value: String,
    /// AI-suggested value
    pub ai_value: String,
}

impl ColumnResult {
    /// Create a new column result
    pub fn new(
        column_index: usize,
        column_name: String,
        original_value: String,
        ai_value: String,
    ) -> Self {
        Self {
            column_index,
            column_name,
            original_value,
            ai_value,
        }
    }

    /// Create for an AI-added row (no original value)
    pub fn new_ai_added(column_index: usize, column_name: String, ai_value: String) -> Self {
        Self {
            column_index,
            column_name,
            original_value: String::new(),
            ai_value,
        }
    }
}

/// Stored result for a single row
#[derive(Debug, Clone)]
pub struct StoredRowResult {
    /// Stable identifier for this row
    pub stable_id: StableRowId,
    /// Results for each column (column_index -> result)
    pub columns: HashMap<usize, ColumnResult>,
    /// Category of this row
    pub category: RowCategory,
    /// Parent validation status (for AI-added rows)
    pub parent_valid: bool,
    /// If parent is invalid, suggested valid parent display values
    pub parent_suggestions: Vec<String>,
    /// For Orphaned rows: the ancestry prefix values the AI claimed
    /// (parent table values in order). Used to show what the AI tried to claim.
    pub claimed_ancestry: Vec<String>,
}

impl StoredRowResult {
    /// Create a new stored result for an original row
    pub fn new_original(
        stable_id: StableRowId,
        columns: Vec<ColumnResult>,
    ) -> Self {
        let columns_map = columns
            .into_iter()
            .map(|c| (c.column_index, c))
            .collect();

        Self {
            stable_id,
            columns: columns_map,
            category: RowCategory::Original,
            parent_valid: true, // Original rows have valid parents by definition
            parent_suggestions: Vec::new(),
            claimed_ancestry: Vec::new(),
        }
    }

    /// Create a new stored result for an AI-added row
    pub fn new_ai_added(
        stable_id: StableRowId,
        columns: Vec<ColumnResult>,
        parent_valid: bool,
        parent_suggestions: Vec<String>,
    ) -> Self {
        let columns_map = columns
            .into_iter()
            .map(|c| (c.column_index, c))
            .collect();

        Self {
            stable_id,
            columns: columns_map,
            category: RowCategory::AiAdded,
            parent_valid,
            parent_suggestions,
            claimed_ancestry: Vec::new(),
        }
    }

    /// Create a marker for a lost row (sent but not returned)
    pub fn new_lost(stable_id: StableRowId) -> Self {
        Self {
            stable_id,
            columns: HashMap::new(),
            category: RowCategory::Lost,
            parent_valid: true,
            parent_suggestions: Vec::new(),
            claimed_ancestry: Vec::new(),
        }
    }

    /// Create a new stored result for an orphaned row (unmatched parent prefix)
    pub fn new_orphaned(
        stable_id: StableRowId,
        columns: Vec<ColumnResult>,
        claimed_ancestry: Vec<String>,
    ) -> Self {
        let columns_map = columns
            .into_iter()
            .map(|c| (c.column_index, c))
            .collect();

        Self {
            stable_id,
            columns: columns_map,
            category: RowCategory::Orphaned,
            parent_valid: false, // Orphaned rows by definition have invalid parents
            parent_suggestions: Vec::new(),
            claimed_ancestry,
        }
    }

    /// Get stable ID reference
    pub fn stable_id(&self) -> &StableRowId {
        &self.stable_id
    }

    /// Get row category
    pub fn category(&self) -> RowCategory {
        self.category
    }

    /// Get columns as a vector sorted by index
    pub fn columns(&self) -> Vec<&ColumnResult> {
        let mut cols: Vec<&ColumnResult> = self.columns.values().collect();
        cols.sort_by_key(|c| c.column_index);
        cols
    }
}

/// Key for table lookup in storage
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableKey {
    pub table_name: String,
    pub category: Option<String>,
    pub step_path: Vec<usize>,
}

impl TableKey {
    pub fn new(table_name: String, category: Option<String>, step_path: Vec<usize>) -> Self {
        Self {
            table_name,
            category,
            step_path,
        }
    }
}

/// Info about a parent that was processed in a child step
#[derive(Debug, Clone)]
pub struct ProcessedParent {
    /// Parent table name
    pub parent_table: String,
    /// Parent stable index
    pub parent_stable_index: usize,
    /// Whether this parent is AI-added
    pub is_ai_added: bool,
}

/// Result Storage - accumulates AI results across processing steps
#[derive(Debug, Clone, Default)]
pub struct ResultStorage {
    /// Stored results organized by table
    /// Key: (table_name, category, structure_path)
    results: HashMap<TableKey, Vec<StoredRowResult>>,
    /// Processed parents for each child step
    /// Key: TableKey (child table), Value: list of parents that were queried
    processed_parents: HashMap<TableKey, Vec<ProcessedParent>>,
}

impl ResultStorage {
    /// Create a new result storage
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a new processing session
    pub fn start_session(&mut self, _generation_id: u64) {
        self.clear();
    }

    /// Set the current processing step (no-op, kept for API compatibility)
    pub fn set_current_step(&mut self, _step: usize) {
        // No-op - step tracking removed
    }

    /// Store results for a table
    ///
    /// # Arguments
    /// * `table_name` - Name of the table
    /// * `category` - Category of the table (if any)
    /// * `step_path` - Step path in multi-step processing (empty for first step)
    /// * `results` - Vector of row results to store
    pub fn store_results(
        &mut self,
        table_name: &str,
        category: Option<&str>,
        step_path: Vec<usize>,
        results: Vec<StoredRowResult>,
    ) {
        let key = TableKey::new(
            table_name.to_string(),
            category.map(|s| s.to_string()),
            step_path,
        );

        let entry = self.results.entry(key).or_insert_with(Vec::new);
        entry.extend(results);
    }

    /// Get results for a specific table and step path
    pub fn get_results_for_table(
        &self,
        table_name: &str,
        category: Option<&str>,
        step_path: &[usize],
    ) -> Option<&Vec<StoredRowResult>> {
        let key = TableKey::new(
            table_name.to_string(),
            category.map(|s| s.to_string()),
            step_path.to_vec(),
        );
        self.results.get(&key)
    }

    /// Iterate over all table keys and their results
    /// This allows access to step_path information for each result group
    pub fn iter_by_table(&self) -> impl Iterator<Item = (&TableKey, &Vec<StoredRowResult>)> {
        self.results.iter()
    }

    /// Store the list of parents that were processed for a child step
    /// This enables creating empty StructureReviewEntry items when AI returns 0 results
    pub fn store_processed_parents(
        &mut self,
        table_name: &str,
        category: Option<&str>,
        step_path: Vec<usize>,
        parents: Vec<ProcessedParent>,
    ) {
        let key = TableKey::new(
            table_name.to_string(),
            category.map(|s| s.to_string()),
            step_path,
        );
        self.processed_parents.insert(key, parents);
    }

    /// Get processed parents for a child step
    pub fn get_processed_parents(
        &self,
        table_name: &str,
        category: Option<&str>,
        step_path: &[usize],
    ) -> Option<&Vec<ProcessedParent>> {
        let key = TableKey::new(
            table_name.to_string(),
            category.map(|s| s.to_string()),
            step_path.to_vec(),
        );
        self.processed_parents.get(&key)
    }

    /// Clear all stored results
    pub fn clear(&mut self) {
        self.results.clear();
        self.processed_parents.clear();
    }

    /// Get AI-modified display value for a specific row by stable_index
    /// 
    /// Searches all step paths for this table to find the AI-modified value
    /// of the first data column (display column). Returns None if the row
    /// hasn't been processed by AI yet.
    /// 
    /// # Arguments
    /// * `table_name` - Name of the table
    /// * `category` - Category of the table (if any)
    /// * `stable_index` - The stable index of the row (DB row_index for Original, Navigator index for AI-added)
    /// * `first_data_col_index` - Grid index of the first data column (display column)
    /// 
    /// # Returns
    /// The AI-modified value for the display column, or None if not found
    pub fn get_ai_display_value(
        &self,
        table_name: &str,
        category: Option<&str>,
        stable_index: usize,
        first_data_col_index: usize,
    ) -> Option<String> {
        // Search all step_paths for this table
        for (key, results) in &self.results {
            if key.table_name == table_name && key.category.as_deref() == category {
                for result in results {
                    if result.stable_id.stable_index == stable_index {
                        // Found the row - get the display column's AI value
                        bevy::log::debug!(
                            "get_ai_display_value: Found row in storage: table='{}' stable_index={} col_keys={:?}",
                            table_name,
                            stable_index,
                            result.columns.keys().collect::<Vec<_>>()
                        );
                        if let Some(col_result) = result.columns.get(&first_data_col_index) {
                            let ai_val = &col_result.ai_value;
                            if !ai_val.is_empty() {
                                bevy::log::debug!(
                                    "get_ai_display_value: Found AI value='{}' at col_index={}",
                                    ai_val,
                                    first_data_col_index
                                );
                                return Some(ai_val.clone());
                            }
                        } else {
                            bevy::log::debug!(
                                "get_ai_display_value: Column {} not found in stored columns",
                                first_data_col_index
                            );
                        }
                    }
                }
            }
        }
        bevy::log::debug!(
            "get_ai_display_value: No result found for table='{}' category={:?} stable_index={} col_index={}",
            table_name,
            category,
            stable_index,
            first_data_col_index
        );
        None
    }

    /// Get a stored result for a specific row by stable_index
    /// 
    /// Searches all step paths for this table to find the stored result.
    /// 
    /// # Arguments
    /// * `table_name` - Name of the table
    /// * `category` - Category of the table (if any)
    /// * `stable_index` - The stable index of the row
    /// 
    /// # Returns
    /// Reference to the StoredRowResult if found
    #[allow(dead_code)]
    pub fn get_result_by_stable_index(
        &self,
        table_name: &str,
        category: Option<&str>,
        stable_index: usize,
    ) -> Option<&StoredRowResult> {
        for (key, results) in &self.results {
            if key.table_name == table_name && key.category.as_deref() == category {
                for result in results {
                    if result.stable_id.stable_index == stable_index {
                        return Some(result);
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheets::systems::ai::processor::navigator::RowOrigin;

    fn make_stable_id(table: &str, index: usize, display: &str, origin: RowOrigin) -> StableRowId {
        StableRowId {
            table_name: table.to_string(),
            category: None,
            stable_index: index,
            display_value: display.to_string(),
            origin,
            parent_stable_index: None,
            parent_table_name: None,
        }
    }

    #[test]
    fn test_store_and_retrieve() {
        let mut storage = ResultStorage::new();
        storage.start_session(1);

        let stable_id = make_stable_id("Aircraft", 0, "MiG-25PD", RowOrigin::Original);
        let columns = vec![
            ColumnResult::new(0, "Name".to_string(), "MiG-25PD".to_string(), "MiG-25PD Foxbat".to_string()),
        ];
        let result = StoredRowResult::new_original(stable_id, columns);

        storage.store_results("Aircraft", None, vec![], vec![result]);

        let all_results: Vec<_> = storage.iter_by_table().flat_map(|(_, v)| v).collect();
        assert_eq!(all_results.len(), 1);
        assert_eq!(all_results[0].category(), RowCategory::Original);
    }

    #[test]
    fn test_category_tracking() {
        let mut storage = ResultStorage::new();
        storage.start_session(1);

        // Add an original row
        let orig_id = make_stable_id("Aircraft", 0, "MiG-25PD", RowOrigin::Original);
        let orig_result = StoredRowResult::new_original(orig_id, vec![]);
        storage.store_results("Aircraft", None, vec![], vec![orig_result]);

        // Add an AI-added row
        let ai_id = make_stable_id("Aircraft", 1, "Su-27", RowOrigin::AiAdded);
        let ai_result = StoredRowResult::new_ai_added(ai_id, vec![], true, vec![]);
        storage.store_results("Aircraft", None, vec![], vec![ai_result]);

        // Add a lost row
        let lost_id = make_stable_id("Aircraft", 2, "F-16", RowOrigin::Original);
        let lost_result = StoredRowResult::new_lost(lost_id);
        storage.store_results("Aircraft", None, vec![], vec![lost_result]);

        let all_results: Vec<_> = storage.iter_by_table().flat_map(|(_, v)| v).collect();
        assert_eq!(all_results.len(), 3);

        let originals: Vec<_> = all_results.iter().filter(|r| r.category() == RowCategory::Original).collect();
        assert_eq!(originals.len(), 1);

        let ai_added: Vec<_> = all_results.iter().filter(|r| r.category() == RowCategory::AiAdded).collect();
        assert_eq!(ai_added.len(), 1);

        let lost: Vec<_> = all_results.iter().filter(|r| r.category() == RowCategory::Lost).collect();
        assert_eq!(lost.len(), 1);
    }

    #[test]
    fn test_parent_validation_stored() {
        let mut storage = ResultStorage::new();
        storage.start_session(1);

        // Add row with invalid parent
        let ai_id = make_stable_id("Engines", 0, "Engine1", RowOrigin::AiAdded);
        let ai_result = StoredRowResult::new_ai_added(
            ai_id,
            vec![],
            false, // Invalid parent
            vec!["ValidParent1".to_string(), "ValidParent2".to_string()],
        );
        storage.store_results("Engines", None, vec![], vec![ai_result]);

        let all_results: Vec<_> = storage.iter_by_table().flat_map(|(_, v)| v).collect();
        assert_eq!(all_results.len(), 1);
        assert!(!all_results[0].parent_valid);
        assert_eq!(all_results[0].parent_suggestions.len(), 2);
    }
}
