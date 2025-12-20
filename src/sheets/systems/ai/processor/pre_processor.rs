// src/sheets/systems/ai/processor/pre_processor.rs
//! Pre-Processor
//!
//! This module prepares data BEFORE AI calls.
//!
//! ## Responsibilities
//!
//! - Read original rows from grid/DB
//! - Extract display values (human-readable names) for key columns
//! - Register indexes with Navigator
//! - Build row data for AI request
//! - Handle structure vs root table differences
//!
//! ## Display Value Resolution
//!
//! For original rows: read from grid at key_col_index
//! For AI-added rows: get from Navigator (stored from previous step)
//!
//! Key column index:
//! - Structure tables: column 2 (after row_index, parent_key)
//! - Root tables: column 1 (after row_index or id)

use std::collections::HashSet;

use super::navigator::{IndexMapper, StableRowId};

/// Configuration for pre-processing a batch
#[derive(Debug, Clone)]
pub struct PreProcessConfig {
    /// Table name
    pub table_name: String,
    /// Category (if any)
    pub category: Option<String>,
    /// Index of the key/display column (for human-readable names)
    pub key_column_index: usize,
    /// Column indices to include in AI request
    pub included_column_indices: Vec<usize>,
    /// Parent stable index (for structure tables)
    pub parent_stable_index: Option<usize>,
    /// Parent table name (for structure tables)
    pub parent_table_name: Option<String>,
}

impl PreProcessConfig {
    /// Create config for a root table
    pub fn for_root_table(
        table_name: String,
        category: Option<String>,
        key_column_index: usize,
        included_column_indices: Vec<usize>,
        _column_names: Vec<String>,
    ) -> Self {
        Self {
            table_name,
            category,
            key_column_index,
            included_column_indices,
            parent_stable_index: None,
            parent_table_name: None,
        }
    }

    /// Create config for a child table
    pub fn for_child_table(
        table_name: String,
        category: Option<String>,
        key_column_index: usize,
        included_column_indices: Vec<usize>,
        _column_names: Vec<String>,
        _step_path: Vec<usize>,
        parent_stable_index: usize,
        parent_table_name: String,
    ) -> Self {
        Self {
            table_name,
            category,
            key_column_index,
            included_column_indices,
            parent_stable_index: Some(parent_stable_index),
            parent_table_name: Some(parent_table_name),
        }
    }
}

/// A prepared row ready for AI request
#[derive(Debug, Clone)]
pub struct PreparedRow {
    /// Stable row ID (registered in Navigator)
    pub stable_id: StableRowId,
    /// Column values in order of included_column_indices
    pub column_values: Vec<String>,
}

impl PreparedRow {
    /// Get the display value for this row
    #[allow(dead_code)] // May be used for debugging/logging
    pub fn display_value(&self) -> &str {
        &self.stable_id.display_value
    }
}

/// Result of pre-processing a batch
#[derive(Debug, Clone)]
pub struct PreparedBatch {
    /// Prepared rows
    pub rows: Vec<PreparedRow>,
    /// Set of display values (kept for potential future use in merge detection)
    #[allow(dead_code)]
    pub sent_display_values: HashSet<String>,
}

impl PreparedBatch {
    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Pre-processor for preparing data before AI calls
#[derive(Debug, Default)]
pub struct PreProcessor {
    /// Navigator reference for index management
    /// Note: In actual usage, this will be passed as mutable reference
    _marker: std::marker::PhantomData<()>,
}

impl PreProcessor {
    /// Create a new pre-processor
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// Prepare a batch of rows for AI request
    ///
    /// # Arguments
    /// * `config` - Pre-processing configuration
    /// * `grid` - The grid data (rows x columns)
    /// * `row_indices` - DB row indices for each grid row
    /// * `selected_rows` - Which grid rows to include (indices into grid)
    /// * `navigator` - Index mapper for stable ID management
    ///
    /// # Returns
    /// PreparedBatch with rows ready for AI request
    pub fn prepare_batch(
        &self,
        config: PreProcessConfig,
        grid: &[Vec<String>],
        row_indices: &[i64],
        selected_rows: &[usize],
        navigator: &mut IndexMapper,
    ) -> PreparedBatch {
        let mut rows = Vec::with_capacity(selected_rows.len());
        let mut sent_display_values = HashSet::new();

        // Collect original row data for registration
        let mut original_rows_data: Vec<(usize, String)> = Vec::new();

        for &grid_row_idx in selected_rows {
            // Get the row from grid
            let row = match grid.get(grid_row_idx) {
                Some(r) => r,
                None => continue,
            };

            // Get DB row index
            let db_row_index = row_indices
                .get(grid_row_idx)
                .copied()
                .unwrap_or(grid_row_idx as i64) as usize;

            // Extract display value from key column
            let display_value = row
                .get(config.key_column_index)
                .cloned()
                .unwrap_or_else(|| format!("Row{}", db_row_index));

            original_rows_data.push((db_row_index, display_value));
        }

        // Register all original rows with navigator
        let stable_ids = navigator.register_original_rows(
            &config.table_name,
            config.category.as_deref(),
            original_rows_data,
            config.parent_stable_index,
            config.parent_table_name,
        );

        // Now build prepared rows using the stable IDs
        for (idx, &grid_row_idx) in selected_rows.iter().enumerate() {
            let row = match grid.get(grid_row_idx) {
                Some(r) => r,
                None => continue,
            };

            let stable_id = match stable_ids.get(idx) {
                Some(id) => id.clone(),
                None => continue,
            };

            // Extract only included columns
            let column_values: Vec<String> = config
                .included_column_indices
                .iter()
                .map(|&col_idx| row.get(col_idx).cloned().unwrap_or_default())
                .collect();

            sent_display_values.insert(stable_id.display_value.clone());

            rows.push(PreparedRow {
                stable_id,
                column_values,
            });
        }

        PreparedBatch {
            rows,
            sent_display_values,
        }
    }

    /// Get the appropriate key column index for a table
    ///
    /// - Child tables: column 2 (after row_index, parent_key)
    /// - Parent/root tables: column 1 (after row_index or first non-technical column)
    pub fn get_key_column_index(is_child_table: bool, explicit_key_col: Option<usize>) -> usize {
        explicit_key_col.unwrap_or_else(|| {
            if is_child_table {
                2 // After row_index (0), parent_key (1)
            } else {
                1 // After row_index/id (0)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_grid() -> Vec<Vec<String>> {
        vec![
            vec!["0".to_string(), "MiG-25PD".to_string(), "3000".to_string()],
            vec!["1".to_string(), "LaGG-3".to_string(), "500".to_string()],
            vec!["2".to_string(), "F-16C".to_string(), "2100".to_string()],
        ]
    }

    #[test]
    fn test_prepare_batch() {
        let processor = PreProcessor::new();
        let mut navigator = IndexMapper::new();

        let config = PreProcessConfig::for_root_table(
            "Aircraft".to_string(),
            None,
            1, // Key column is "Name" at index 1
            vec![1, 2], // Include Name and Speed
            vec!["Name".to_string(), "Speed".to_string()],
        );

        let grid = make_grid();
        let row_indices: Vec<i64> = vec![0, 1, 2];
        let selected_rows = vec![0, 2]; // Select rows 0 and 2

        let batch = processor.prepare_batch(config, &grid, &row_indices, &selected_rows, &mut navigator);

        assert_eq!(batch.rows.len(), 2);
        assert!(batch.sent_display_values.contains("MiG-25PD"));
        assert!(batch.sent_display_values.contains("F-16C"));
        assert!(!batch.sent_display_values.contains("LaGG-3"));

        // Check stable IDs were registered
        assert!(navigator.get("Aircraft", None, 0).is_some());
        assert!(navigator.get("Aircraft", None, 2).is_some());
        assert!(navigator.get("Aircraft", None, 1).is_none()); // Not selected
    }

    #[test]
    fn test_key_column_index() {
        // Parent/root table (is_child_table = false)
        assert_eq!(PreProcessor::get_key_column_index(false, None), 1);
        assert_eq!(PreProcessor::get_key_column_index(false, Some(3)), 3);

        // Child table (is_child_table = true)
        assert_eq!(PreProcessor::get_key_column_index(true, None), 2);
        assert_eq!(PreProcessor::get_key_column_index(true, Some(5)), 5);
    }
}
