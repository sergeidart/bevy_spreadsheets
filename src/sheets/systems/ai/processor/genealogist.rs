// src/sheets/systems/ai/processor/genealogist.rs
//! Genealogist - Parent Lineage & Ancestry Builder
//!
//! This module builds ancestry/lineage context for structure tables.
//! It walks up the parent chain to provide context for AI prompts.
//!
//! ## Responsibilities
//!
//! - Walk parent chain using parent_key references
//! - Build context strings for AI prompts
//! - Gather per-row ancestry with caching
//! - Validate ancestry chains against database
//!
//! ## Key Concepts
//!
//! - **Ancestry**: The chain of parent rows from root to current row
//! - **Per-row ancestry**: Each row in a child table may have different ancestry
//! - **Per-table context**: AI context is gathered once per table level (cached)
//!
//! ## Lineage Format
//!
//! Lineage is built in root-to-leaf order:
//! Example: Games → Games_Platforms → Games_Platforms_Store

use std::collections::HashMap;
use bevy::prelude::*;
use super::navigator::{IndexMapper, StableRowId};
use super::storager::ResultStorage;
use crate::sheets::resources::SheetRegistry;

// ============================================================================
// Ancestry Types (for per-row ancestry gathering from DB)
// ============================================================================

/// A single level of ancestry info
#[derive(Debug, Clone)]
pub struct AncestryLevel {
    /// Table name at this level
    pub table_name: String,
    /// Human-readable display value (e.g., "F-16C")
    pub display_value: String,
    /// Row index at this level
    pub row_index: usize,
}

/// Complete ancestry chain from root to immediate parent
#[derive(Debug, Clone, Default)]
pub struct Ancestry {
    /// Levels in root-to-leaf order (NOT including current table)
    pub levels: Vec<AncestryLevel>,
}

impl Ancestry {
    /// Create empty ancestry (for root tables)
    pub fn empty() -> Self {
        Self { levels: Vec::new() }
    }

    /// Check if this is a root table (no ancestry)
    pub fn is_root(&self) -> bool {
        self.levels.is_empty()
    }

    /// Get the depth (number of ancestor levels)
    pub fn depth(&self) -> usize {
        self.levels.len()
    }

    /// Get display values as a vector (for prepending to AI payload)
    pub fn display_values(&self) -> Vec<String> {
        self.levels.iter().map(|l| l.display_value.clone()).collect()
    }
    
    /// Get table names for each ancestry level (root-to-leaf order)
    pub fn table_names(&self) -> Vec<String> {
        self.levels.iter().map(|l| l.table_name.clone()).collect()
    }
    
    /// Get row indices for each ancestry level (for parent_key chain reconstruction)
    pub fn row_indices(&self) -> Vec<usize> {
        self.levels.iter().map(|l| l.row_index).collect()
    }
    
    /// Get the immediate parent's row_index (for parent_key column value)
    /// Returns None if this is a root table (no ancestry)
    pub fn parent_row_index(&self) -> Option<usize> {
        self.levels.last().map(|l| l.row_index)
    }
    
    /// Get the immediate parent's table name
    /// Returns None if this is a root table (no ancestry)
    pub fn parent_table_name(&self) -> Option<&str> {
        self.levels.last().map(|l| l.table_name.as_str())
    }
    
    /// Format ancestry as a human-readable path string
    /// Example: "Aircraft > Aircraft_Pylons > Weapons"
    pub fn format_path(&self) -> String {
        if self.levels.is_empty() {
            return "(root)".to_string();
        }
        self.levels.iter()
            .map(|l| format!("{}:{}", l.table_name, l.display_value))
            .collect::<Vec<_>>()
            .join(" > ")
    }
}

/// Result of gathering AI contexts for ancestry chain
#[derive(Debug, Clone, Default)]
pub struct AncestryContexts {
    /// AI context strings for each ancestry level (root-to-leaf order)
    /// If ai_context is None, a fallback context with table name is provided
    pub contexts: Vec<Option<String>>,
    /// Table names for each ancestry level (root-to-leaf order)
    /// Used for debugging and when generating fallback contexts
    pub table_names: Vec<String>,
}

impl AncestryContexts {
    /// Create empty contexts
    pub fn empty() -> Self {
        Self { 
            contexts: Vec::new(),
            table_names: Vec::new(),
        }
    }
    
    /// Check if this represents a root table (no ancestry)
    pub fn is_root(&self) -> bool {
        self.table_names.is_empty()
    }
    
    /// Get the depth (number of ancestor levels)
    pub fn depth(&self) -> usize {
        self.table_names.len()
    }
}

// ============================================================================
// Lineage Types (for Navigator-based lineage from registered rows)
// ============================================================================

/// A single ancestor in the lineage chain
#[derive(Debug, Clone)]
pub struct Ancestor {
    /// Table name of the ancestor
    pub table_name: String,
    /// Human-readable display value (e.g., "MiG-25PD")
    pub display_value: String,
}

impl Ancestor {
    /// Create a new ancestor entry
    pub fn new(table_name: String, display_value: String) -> Self {
        Self { table_name, display_value }
    }
}

/// Complete lineage chain from root to current row (Navigator-based)
#[derive(Debug, Clone, Default)]
pub struct Lineage {
    /// Ancestors in root-to-leaf order
    pub ancestors: Vec<Ancestor>,
}

impl Lineage {
    /// Create lineage with ancestors
    pub fn new(ancestors: Vec<Ancestor>) -> Self {
        Self { ancestors }
    }
    
    /// Check if lineage is empty
    pub fn is_empty(&self) -> bool {
        self.ancestors.is_empty()
    }
    
    /// Convert Lineage to Ancestry
    ///
    /// Navigator-based Lineage doesn't have row indices, so they default to 0.
    /// This is fine for AI-added parents where row_index isn't meaningful.
    pub fn to_ancestry(&self) -> Ancestry {
        Ancestry {
            levels: self.ancestors.iter().map(|a| AncestryLevel {
                table_name: a.table_name.clone(),
                display_value: a.display_value.clone(),
                row_index: 0, // Not available from Navigator
            }).collect(),
        }
    }
}

// ============================================================================
// Cache Key
// ============================================================================

/// Cache key for ancestry lookups
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct AncestryKey {
    table_name: String,
    category: Option<String>,
    parent_key: usize,
}

// ============================================================================
// Genealogist
// ============================================================================

/// Genealogist - builds parent lineage and ancestry for structure tables
///
/// Provides:
/// - Navigator-based lineage (using registered StableRowIds)
/// - DB-based ancestry gathering (walking parent_key directly)
/// - Caching for repeated lookups
#[derive(Debug, Default)]
pub struct Genealogist {
    /// Safety limit for depth traversal (prevents infinite loops)
    max_depth: usize,
    /// Cache of ancestry by (table, category, parent_key)
    ancestry_cache: HashMap<AncestryKey, Ancestry>,
    /// Cache of contexts by table chain
    context_cache: HashMap<Vec<String>, AncestryContexts>,
}

impl Genealogist {
    /// Create a new genealogist with default settings
    pub fn new() -> Self {
        Self {
            max_depth: 10,
            ancestry_cache: HashMap::new(),
            context_cache: HashMap::new(),
        }
    }

    /// Clear all caches (call at start of new processing session)
    pub fn clear(&mut self) {
        self.ancestry_cache.clear();
        self.context_cache.clear();
    }

    // ========================================================================
    // Navigator-based Lineage (uses registered rows)
    // ========================================================================

    /// Build lineage for a row using Navigator data
    ///
    /// This is the preferred method when all rows are registered in Navigator.
    /// It walks up the parent_stable_index chain to build the lineage.
    ///
    /// # Arguments
    /// * `stable_id` - The row to build lineage for
    /// * `navigator` - Index mapper with registered rows
    ///
    /// # Returns
    /// Lineage with ancestors in root-to-leaf order
    #[allow(dead_code)]
    pub fn build_lineage_from_navigator(
        &self,
        stable_id: &StableRowId,
        navigator: &IndexMapper,
    ) -> Lineage {
        let mut ancestors = Vec::new();

        // Add the immediate parent (stable_id itself) to the lineage
        ancestors.push(Ancestor::new(
            stable_id.table_name.clone(),
            stable_id.display_value.clone(),
        ));

        let mut current_parent_idx = stable_id.parent_stable_index;
        let mut current_parent_table = stable_id.parent_table_name.clone();
        let mut current_category = stable_id.category.clone();
        let mut depth = 0;

        while let Some(parent_idx) = current_parent_idx {
            if depth >= self.max_depth {
                error!(
                    "Genealogist: Hit depth limit ({}) - possible circular reference",
                    self.max_depth
                );
                break;
            }

            let parent_table = match &current_parent_table {
                Some(t) => t,
                None => {
                    warn!("Genealogist: Missing parent table name for index {}", parent_idx);
                    break;
                }
            };

            // Look up parent in navigator using parent_table context
            if let Some(parent_id) = navigator.get(parent_table, current_category.as_deref(), parent_idx) {
                ancestors.push(Ancestor::new(
                    parent_id.table_name.clone(),
                    parent_id.display_value.clone(),
                ));

                current_parent_idx = parent_id.parent_stable_index;
                current_parent_table = parent_id.parent_table_name.clone();
                current_category = parent_id.category.clone();
                depth += 1;
            } else {
                // Parent not found in navigator
                warn!(
                    "Genealogist: Parent {} not found in navigator for table '{}'",
                    parent_idx, parent_table
                );
                break;
            }
        }

        // Reverse to get root-to-leaf order
        ancestors.reverse();

        Lineage::new(ancestors)
    }

    // ========================================================================
    // DB-based Ancestry (walks parent_key directly from grid data)
    // ========================================================================

    /// Get ancestry depth for a table (0 for root, 1 for child, 2 for grandchild, etc.)
    ///
    /// This walks the table chain without looking at specific rows.
    pub fn get_table_depth(
        registry: &SheetRegistry,
        category: &Option<String>,
        table_name: &str,
    ) -> usize {
        let mut depth = 0;
        let mut current_table = table_name.to_string();
        let mut current_category = category.clone();
        let max_depth = 10;

        while depth < max_depth {
            let sheet = match registry.get_sheet(&current_category, &current_table) {
                Some(s) => s,
                None => break,
            };

            let metadata = match &sheet.metadata {
                Some(m) => m,
                None => break,
            };

            // Check if this table has parent_key
            let has_pk = metadata.columns.iter()
                .any(|c| c.header.eq_ignore_ascii_case("parent_key"));

            if !has_pk {
                break; // This is a root table
            }

            depth += 1;

            // Get parent table from structure_parent or parse from name
            if let Some(parent_link) = &metadata.structure_parent {
                current_category = parent_link.parent_category.clone();
                current_table = parent_link.parent_sheet.clone();
            } else if let Some((parent, _)) = current_table.rsplit_once('_') {
                current_table = parent.to_string();
            } else {
                break;
            }
        }

        depth
    }

    /// Build table chain from child to root
    ///
    /// Returns table names in root-to-leaf order.
    fn build_table_chain(
        registry: &SheetRegistry,
        category: &Option<String>,
        table_name: &str,
    ) -> Vec<String> {
        let mut chain = Vec::new();
        let mut current_table = table_name.to_string();
        let mut current_category = category.clone();
        let max_depth = 10;
        let mut depth = 0;

        while depth < max_depth {
            chain.push(current_table.clone());

            let sheet = match registry.get_sheet(&current_category, &current_table) {
                Some(s) => s,
                None => break,
            };

            let metadata = match &sheet.metadata {
                Some(m) => m,
                None => break,
            };

            // Check if this table has parent_key
            let has_pk = metadata.columns.iter()
                .any(|c| c.header.eq_ignore_ascii_case("parent_key"));

            if !has_pk {
                break; // This is a root table
            }

            // Get parent table
            if let Some(parent_link) = &metadata.structure_parent {
                current_category = parent_link.parent_category.clone();
                current_table = parent_link.parent_sheet.clone();
            } else if let Some((parent, _)) = current_table.rsplit_once('_') {
                current_table = parent.to_string();
            } else {
                break;
            }

            depth += 1;
        }

        // Reverse to get root-to-leaf order
        chain.reverse();
        chain
    }

    /// Gather ancestry for a specific row by walking up the parent_key chain
    ///
    /// Uses caching: if ancestry for this (table, category, parent_key) was already
    /// computed, returns cached result.
    ///
    /// # Arguments
    /// * `registry` - Sheet registry for lookups
    /// * `category` - Category of the table
    /// * `table_name` - Current table name
    /// * `parent_key` - Value of parent_key column for the row
    ///
    /// # Returns
    /// Ancestry chain from root to immediate parent (not including current table)
    #[allow(dead_code)]
    pub fn gather_row_ancestry(
        &mut self,
        registry: &SheetRegistry,
        category: &Option<String>,
        table_name: &str,
        parent_key: usize,
    ) -> Ancestry {
        let cache_key = AncestryKey {
            table_name: table_name.to_string(),
            category: category.clone(),
            parent_key,
        };

        // Check cache first
        if let Some(cached) = self.ancestry_cache.get(&cache_key) {
            return cached.clone();
        }

        // Build ancestry by walking up
        let ancestry = self.walk_ancestry(registry, category, table_name, parent_key);

        // Cache and return
        self.ancestry_cache.insert(cache_key, ancestry.clone());
        ancestry
    }

    /// Internal: Walk up the parent chain to build ancestry
    #[allow(dead_code)]
    fn walk_ancestry(
        &self,
        registry: &SheetRegistry,
        category: &Option<String>,
        table_name: &str,
        parent_key: usize,
    ) -> Ancestry {
        let mut levels = Vec::new();
        let mut current_row_idx = parent_key;

        // Get parent table info
        let sheet = match registry.get_sheet(category, table_name) {
            Some(s) => s,
            None => return Ancestry::empty(),
        };

        let metadata = match &sheet.metadata {
            Some(m) => m,
            None => return Ancestry::empty(),
        };

        // Determine parent table
        let (mut current_table, mut current_category) = if let Some(parent_link) = &metadata.structure_parent {
            (parent_link.parent_sheet.clone(), parent_link.parent_category.clone())
        } else if let Some((parent, _)) = table_name.rsplit_once('_') {
            (parent.to_string(), category.clone())
        } else {
            return Ancestry::empty(); // No parent
        };

        let mut depth = 0;

        while depth < self.max_depth {
            let parent_sheet = match registry.get_sheet(&current_category, &current_table) {
                Some(s) => s,
                None => {
                    warn!("Genealogist: Parent sheet '{}' not found", current_table);
                    break;
                }
            };

            let parent_meta = match &parent_sheet.metadata {
                Some(m) => m,
                None => {
                    warn!("Genealogist: Parent sheet '{}' has no metadata", current_table);
                    break;
                }
            };

            // Find row by row_index
            let row = parent_sheet.grid.iter().find(|r| {
                r.get(0)
                    .and_then(|idx_str| idx_str.parse::<usize>().ok())
                    .map(|idx| idx == current_row_idx)
                    .unwrap_or(false)
            });

            let Some(row) = row else {
                warn!(
                    "Genealogist: Row with row_index={} not found in '{}'",
                    current_row_idx, current_table
                );
                break;
            };

            // Get display value
            let display_value = parent_meta.get_first_data_column_value(row);

            levels.push(AncestryLevel {
                table_name: current_table.clone(),
                display_value,
                row_index: current_row_idx,
            });

            // Check if parent has parent_key (to continue walking)
            let parent_key_col = parent_meta.columns.iter()
                .position(|c| c.header.eq_ignore_ascii_case("parent_key"));

            if let Some(pk_col) = parent_key_col {
                let pk_str = row.get(pk_col).cloned().unwrap_or_default();

                if pk_str.is_empty() {
                    break; // Root reached
                }

                let Ok(next_parent_key) = pk_str.parse::<usize>() else {
                    warn!(
                        "Genealogist: Invalid parent_key '{}' in '{}'",
                        pk_str, current_table
                    );
                    break;
                };

                current_row_idx = next_parent_key;

                // Get next parent table
                if let Some(parent_link) = &parent_meta.structure_parent {
                    current_category = parent_link.parent_category.clone();
                    current_table = parent_link.parent_sheet.clone();
                } else if let Some((parent, _)) = current_table.rsplit_once('_') {
                    current_table = parent.to_string();
                } else {
                    break;
                }
            } else {
                break; // Root table (no parent_key)
            }

            depth += 1;
        }

        // Reverse to get root-to-leaf order
        levels.reverse();

        Ancestry { levels }
    }

    /// Gather ancestry with AI-modified display values
    ///
    /// This method is similar to `gather_row_ancestry` but checks `ResultStorage`
    /// for AI-modified display values first, falling back to the original DB values
    /// only if no AI modification exists.
    ///
    /// This is essential for multi-step processing where a parent row may have been
    /// modified by AI in a previous step, and child rows need to use the modified
    /// display value in their ancestry prefix.
    ///
    /// # Arguments
    /// * `registry` - Sheet registry for table access
    /// * `category` - Table category
    /// * `table_name` - Current table name (child table)
    /// * `parent_key` - Row index of the parent (from child's parent_key column)
    /// * `storage` - ResultStorage containing AI modifications from previous steps
    ///
    /// # Returns
    /// Ancestry with AI-modified display values where available
    pub fn gather_row_ancestry_with_ai_overrides(
        &mut self,
        registry: &SheetRegistry,
        category: &Option<String>,
        table_name: &str,
        parent_key: usize,
        storage: &ResultStorage,
    ) -> Ancestry {
        // Check cache first - all rows with same parent_key in same table have identical ancestry
        let cache_key = AncestryKey {
            table_name: table_name.to_string(),
            category: category.clone(),
            parent_key,
        };
        
        if let Some(cached) = self.ancestry_cache.get(&cache_key) {
            return cached.clone();
        }

        let mut levels = Vec::new();
        let mut current_row_idx = parent_key;

        // Get parent table info
        let sheet = match registry.get_sheet(category, table_name) {
            Some(s) => s,
            None => return Ancestry::empty(),
        };

        let metadata = match &sheet.metadata {
            Some(m) => m,
            None => return Ancestry::empty(),
        };

        // Determine parent table
        let (mut current_table, mut current_category) = if let Some(parent_link) = &metadata.structure_parent {
            (parent_link.parent_sheet.clone(), parent_link.parent_category.clone())
        } else if let Some((parent, _)) = table_name.rsplit_once('_') {
            (parent.to_string(), category.clone())
        } else {
            return Ancestry::empty(); // No parent
        };

        let mut depth = 0;

        while depth < self.max_depth {
            let parent_sheet = match registry.get_sheet(&current_category, &current_table) {
                Some(s) => s,
                None => {
                    warn!("Genealogist: Parent sheet '{}' not found", current_table);
                    break;
                }
            };

            let parent_meta = match &parent_sheet.metadata {
                Some(m) => m,
                None => {
                    warn!("Genealogist: Parent sheet '{}' has no metadata", current_table);
                    break;
                }
            };

            // Find row by row_index in DB
            let row = parent_sheet.grid.iter().find(|r| {
                r.get(0)
                    .and_then(|idx_str| idx_str.parse::<usize>().ok())
                    .map(|idx| idx == current_row_idx)
                    .unwrap_or(false)
            });

            let Some(row) = row else {
                warn!(
                    "Genealogist: Row with row_index={} not found in '{}'",
                    current_row_idx, current_table
                );
                break;
            };

            // Try to get AI-modified display value first, fall back to original
            let first_data_col_idx = parent_meta.find_first_data_column_index().unwrap_or(0);
            let display_value = storage
                .get_ai_display_value(
                    &current_table,
                    current_category.as_deref(),
                    current_row_idx,
                    first_data_col_idx,
                )
                .unwrap_or_else(|| {
                    // No AI modification - use original DB value
                    parent_meta.get_first_data_column_value(row)
                });

            debug!(
                "Genealogist: Ancestry level table='{}' row_idx={} display='{}'",
                current_table, current_row_idx, display_value
            );

            levels.push(AncestryLevel {
                table_name: current_table.clone(),
                display_value,
                row_index: current_row_idx,
            });

            // Check if parent has parent_key (to continue walking)
            let parent_key_col = parent_meta.columns.iter()
                .position(|c| c.header.eq_ignore_ascii_case("parent_key"));

            if let Some(pk_col) = parent_key_col {
                let pk_str = row.get(pk_col).cloned().unwrap_or_default();

                if pk_str.is_empty() {
                    break; // Root reached
                }

                let Ok(next_parent_key) = pk_str.parse::<usize>() else {
                    warn!(
                        "Genealogist: Invalid parent_key '{}' in '{}'",
                        pk_str, current_table
                    );
                    break;
                };

                current_row_idx = next_parent_key;

                // Get next parent table
                if let Some(parent_link) = &parent_meta.structure_parent {
                    current_category = parent_link.parent_category.clone();
                    current_table = parent_link.parent_sheet.clone();
                } else if let Some((parent, _)) = current_table.rsplit_once('_') {
                    current_table = parent.to_string();
                } else {
                    break;
                }
            } else {
                break; // Root table (no parent_key)
            }

            depth += 1;
        }

        // Reverse to get root-to-leaf order
        levels.reverse();

        let ancestry = Ancestry { levels };
        
        // Cache the result for reuse by other rows with same parent_key
        self.ancestry_cache.insert(cache_key, ancestry.clone());
        
        ancestry
    }

    /// Build ancestry from Navigator with AI-modified display values
    ///
    /// This is an enhanced version of `build_lineage_from_navigator` that also
    /// checks `ResultStorage` for AI-modified display values. Use this for
    /// AI-added parent rows that were registered in Navigator.
    ///
    /// # Arguments
    /// * `stable_id` - The StableRowId of the parent row (from Navigator)
    /// * `navigator` - The Navigator containing registered rows
    /// * `storage` - ResultStorage containing AI modifications
    /// * `registry` - Sheet registry for getting first data column index
    ///
    /// # Returns
    /// Lineage with AI-modified display values where available
    pub fn build_lineage_from_navigator_with_ai_overrides(
        &self,
        stable_id: &StableRowId,
        navigator: &IndexMapper,
        storage: &ResultStorage,
        registry: &SheetRegistry,
    ) -> Lineage {
        let mut ancestors = Vec::new();

        // Get first data column index for the starting table
        let first_data_col_idx = registry
            .get_sheet(&stable_id.category, &stable_id.table_name)
            .and_then(|s| s.metadata.as_ref())
            .and_then(|m| m.find_first_data_column_index())
            .unwrap_or(0);

        // Try to get AI-modified display value for the starting row
        let display_value = storage
            .get_ai_display_value(
                &stable_id.table_name,
                stable_id.category.as_deref(),
                stable_id.stable_index,
                first_data_col_idx,
            )
            .unwrap_or_else(|| stable_id.display_value.clone());

        // Add the immediate parent (stable_id itself) to the lineage
        ancestors.push(Ancestor::new(
            stable_id.table_name.clone(),
            display_value,
        ));

        let mut current_parent_idx = stable_id.parent_stable_index;
        let mut current_parent_table = stable_id.parent_table_name.clone();
        let mut current_category = stable_id.category.clone();
        let mut depth = 0;

        while let Some(parent_idx) = current_parent_idx {
            if depth >= self.max_depth {
                error!(
                    "Genealogist: Hit depth limit ({}) - possible circular reference",
                    self.max_depth
                );
                break;
            }

            let parent_table = match &current_parent_table {
                Some(t) => t,
                None => {
                    warn!("Genealogist: Missing parent table name for index {}", parent_idx);
                    break;
                }
            };

            // Look up parent in navigator using parent_table context
            if let Some(parent_id) = navigator.get(parent_table, current_category.as_deref(), parent_idx) {
                // Get first data column index for this parent table
                let parent_first_data_col = registry
                    .get_sheet(&parent_id.category, &parent_id.table_name)
                    .and_then(|s| s.metadata.as_ref())
                    .and_then(|m| m.find_first_data_column_index())
                    .unwrap_or(0);

                // Try to get AI-modified display value
                let parent_display = storage
                    .get_ai_display_value(
                        &parent_id.table_name,
                        parent_id.category.as_deref(),
                        parent_id.stable_index,
                        parent_first_data_col,
                    )
                    .unwrap_or_else(|| parent_id.display_value.clone());

                ancestors.push(Ancestor::new(
                    parent_id.table_name.clone(),
                    parent_display,
                ));

                current_parent_idx = parent_id.parent_stable_index;
                current_parent_table = parent_id.parent_table_name.clone();
                current_category = parent_id.category.clone();
                depth += 1;
            } else {
                // Parent not found in navigator
                warn!(
                    "Genealogist: Parent {} not found in navigator for table '{}'",
                    parent_idx, parent_table
                );
                break;
            }
        }

        // Reverse to get root-to-leaf order
        ancestors.reverse();

        Lineage::new(ancestors)
    }

    /// Build ancestry for a batch parent (unified method for Director)
    ///
    /// This is the main method Director should call when building ancestry for
    /// child table batches. It handles both AI-added and original parents:
    ///
    /// - **AI-added parent**: Uses Navigator to walk up registered parent chain,
    ///   then converts the Lineage to Ancestry
    /// - **Original parent**: Uses DB parent_key chain with AI-modified display values
    ///
    /// # Arguments
    /// * `category` - Table category
    /// * `table_name` - Current child table name
    /// * `parent_stable_index` - Stable index of the parent row
    /// * `parent_table_name` - Table name of the parent
    /// * `parent_display_value` - Display value of the parent (fallback)
    /// * `is_ai_added` - Whether the parent was added by AI in a previous step
    /// * `navigator` - IndexMapper with registered rows
    /// * `storage` - ResultStorage with AI modifications
    /// * `registry` - Sheet registry for lookups
    ///
    /// # Returns
    /// Complete Ancestry chain from root to immediate parent
    pub fn gather_ancestry_for_batch(
        &mut self,
        category: &Option<String>,
        table_name: &str,
        parent_stable_index: usize,
        parent_table_name: &str,
        parent_display_value: &str,
        is_ai_added: bool,
        navigator: &IndexMapper,
        storage: &ResultStorage,
        registry: &SheetRegistry,
    ) -> Ancestry {
        if is_ai_added {
            // AI-added parent: use Navigator to build full ancestry chain
            // The parent was registered in Navigator during a previous step
            info!(
                "Genealogist: Looking up AI-added parent in Navigator: table='{}' stable_index={} display='{}'",
                parent_table_name,
                parent_stable_index,
                parent_display_value
            );
            
            if let Some(parent_stable_id) = navigator.get(
                parent_table_name,
                category.as_deref(),
                parent_stable_index,
            ) {
                info!(
                    "Genealogist: Found parent in Navigator: table='{}' stable_index={} display='{}' parent_table={:?} parent_idx={:?}",
                    parent_stable_id.table_name,
                    parent_stable_id.stable_index,
                    parent_stable_id.display_value,
                    parent_stable_id.parent_table_name,
                    parent_stable_id.parent_stable_index
                );
                
                // Build full lineage using Navigator with AI-modified display values
                let lineage = self.build_lineage_from_navigator_with_ai_overrides(
                    parent_stable_id,
                    navigator,
                    storage,
                    registry,
                );
                
                info!(
                    "Genealogist: Lineage built: {} ancestors: {:?}",
                    lineage.ancestors.len(),
                    lineage.ancestors.iter().map(|a| format!("{}:{}", a.table_name, a.display_value)).collect::<Vec<_>>()
                );
                
                // Convert Lineage to Ancestry
                lineage.to_ancestry()
            } else {
                // Fallback: parent not found in Navigator, use display value only
                warn!(
                    "Genealogist: AI-added parent '{}' not found in Navigator for table '{}' at index {}, using display value only",
                    parent_display_value,
                    parent_table_name,
                    parent_stable_index
                );
                Ancestry {
                    levels: vec![AncestryLevel {
                        table_name: parent_table_name.to_string(),
                        display_value: parent_display_value.to_string(),
                        row_index: parent_stable_index,
                    }],
                }
            }
        } else {
            // Original parent: gather full ancestry from DB with AI-modified values
            self.gather_row_ancestry_with_ai_overrides(
                registry,
                category,
                table_name,
                parent_stable_index,
                storage,
            )
        }
    }

    /// Gather AI contexts for a table chain (cached per chain)
    ///
    /// Called once per table to get contexts for all ancestry levels.
    /// These are prepended to column_contexts in the AI payload.
    pub fn gather_ancestry_contexts(
        &mut self,
        registry: &SheetRegistry,
        category: &Option<String>,
        table_name: &str,
    ) -> AncestryContexts {
        let table_chain = Self::build_table_chain(registry, category, table_name);

        // Check cache
        if let Some(cached) = self.context_cache.get(&table_chain) {
            return cached.clone();
        }

        // Gather contexts (skip last element as that's the current table)
        let ancestor_tables = if table_chain.len() > 1 {
            &table_chain[..table_chain.len() - 1]
        } else {
            return AncestryContexts::empty(); // Root table
        };

        let mut contexts = Vec::new();
        let mut table_names = Vec::new();

        for ancestor_table in ancestor_tables {
            table_names.push(ancestor_table.clone());
            
            let ai_context = registry
                .get_sheet(category, ancestor_table)
                .and_then(|s| s.metadata.as_ref())
                .and_then(|m| m.find_first_data_column_index().and_then(|idx| m.columns.get(idx)))
                .and_then(|col| col.ai_context.clone());

            // Use ai_context if available, otherwise generate fallback with table name
            let context = ai_context.or_else(|| {
                Some(format!("Parent row from '{}' table", ancestor_table))
            });

            contexts.push(context);
        }

        let result = AncestryContexts { contexts, table_names };
        self.context_cache.insert(table_chain, result.clone());
        result
    }

    /// Validate that an ancestry chain exists in the database
    ///
    /// Given display values from AI response, verify the chain is valid.
    /// Returns Some(parent_key) if valid, None if invalid.
    pub fn validate_ancestry_chain(
        registry: &SheetRegistry,
        category: &Option<String>,
        table_name: &str,
        ancestry_values: &[String],
    ) -> Option<usize> {
        use crate::sheets::systems::logic::lineage_helpers::resolve_parent_key_from_lineage;

        if ancestry_values.is_empty() {
            return None;
        }

        // Get parent table name
        let parent_table = {
            let sheet = registry.get_sheet(category, table_name)?;
            let meta = sheet.metadata.as_ref()?;

            if let Some(parent_link) = &meta.structure_parent {
                parent_link.parent_sheet.clone()
            } else if let Some((parent, _)) = table_name.rsplit_once('_') {
                parent.to_string()
            } else {
                return None;
            }
        };

        // Use existing lineage resolver
        resolve_parent_key_from_lineage(registry, category, &parent_table, ancestry_values)
    }

    /// Get valid parent display values for a table (for suggestions when validation fails)
    pub fn get_valid_parent_values(
        registry: &SheetRegistry,
        category: &Option<String>,
        table_name: &str,
    ) -> Vec<String> {
        use crate::sheets::systems::logic::lineage_helpers::get_parent_sheet_options;

        // Get parent table name
        let parent_table = {
            let Some(sheet) = registry.get_sheet(category, table_name) else {
                return Vec::new();
            };
            let Some(meta) = &sheet.metadata else {
                return Vec::new();
            };

            if let Some(parent_link) = &meta.structure_parent {
                parent_link.parent_sheet.clone()
            } else if let Some((parent, _)) = table_name.rsplit_once('_') {
                parent.to_string()
            } else {
                return Vec::new();
            }
        };

        get_parent_sheet_options(registry, category, &parent_table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ancestor() {
        let ancestor = Ancestor::new(
            "Aircraft".to_string(),
            "MiG-25PD".to_string(),
        );

        assert_eq!(ancestor.display_value, "MiG-25PD");
    }

    #[test]
    fn test_ancestry_empty() {
        let ancestry = Ancestry::empty();
        assert!(ancestry.is_root());
        assert_eq!(ancestry.depth(), 0);
        assert!(ancestry.display_values().is_empty());
    }

    #[test]
    fn test_ancestry_with_levels() {
        let ancestry = Ancestry {
            levels: vec![
                AncestryLevel {
                    table_name: "Aircraft".to_string(),
                    display_value: "F-16C".to_string(),
                    row_index: 42,
                },
                AncestryLevel {
                    table_name: "Aircraft_Pylons".to_string(),
                    display_value: "Underwing Pylon 1".to_string(),
                    row_index: 7,
                },
            ],
        };

        assert!(!ancestry.is_root());
        assert_eq!(ancestry.depth(), 2);
        assert_eq!(
            ancestry.display_values(),
            vec!["F-16C".to_string(), "Underwing Pylon 1".to_string()]
        );
    }

    #[test]
    fn test_genealogist_caching() {
        let mut genealogist = Genealogist::new();
        assert!(genealogist.ancestry_cache.is_empty());

        genealogist.clear();
        assert!(genealogist.ancestry_cache.is_empty());
        assert!(genealogist.context_cache.is_empty());
    }
}
