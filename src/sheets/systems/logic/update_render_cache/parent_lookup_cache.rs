// src/sheets/systems/logic/update_render_cache/parent_lookup_cache.rs
//! Parent lookup cache for optimized ancestor resolution
//!
//! Builds a HashMap that maps (table_name, row_index) to display text,
//! enabling O(1) lookups instead of O(n) linear searches through parent tables.

use bevy::prelude::*;
use std::collections::HashMap;
use crate::sheets::resources::SheetRegistry;

/// Type alias for the parent lookup cache
///
/// Maps (table_name, row_index) → display_text
/// Example: ("Games", 1539) → "STALKER 2"
pub type ParentLookupCache = HashMap<(String, i64), String>;

/// Build a lookup cache for parent tables to enable O(1) row_index resolution
///
/// This is a major optimization: instead of linear searching for each cell,
/// we build HashMap<row_index, display_text> once and reuse it.
///
/// For a table like `Games_Platforms_Stores`, this will cache:
/// - Games table entries
/// - Games_Platforms table entries
///
/// So that both `parent_key` and `grand_*_parent` columns can be resolved quickly.
pub fn build_parent_lookup_cache(
    current_category: &Option<String>,
    current_sheet_name: &str,
    registry: &SheetRegistry,
) -> ParentLookupCache {
    let mut cache = HashMap::new();

    // Determine all possible parent tables by navigating up hierarchy
    // For Games_Platforms_Stores: need Games_Platforms and Games
    let mut table_name = current_sheet_name;
    let mut parent_tables = Vec::new();

    // Navigate up to collect all ancestor tables
    while let Some((parent, _)) = table_name.rsplit_once('_') {
        parent_tables.push(parent.to_string());
        table_name = parent;
    }

    // Build cache for each parent table
    for parent_table in parent_tables {
        if let Some(parent_sheet) = registry.get_sheet(current_category, &parent_table) {
            if let Some(parent_metadata) = &parent_sheet.metadata {
                // Find first non-technical column
                if let Some(first_data_col_idx) = parent_metadata.columns.iter().position(|col| {
                    let lower = col.header.to_lowercase();
                    lower != "row_index"
                        && lower != "parent_key"
                        && !lower.starts_with("grand_")
                        && lower != "id"
                        && lower != "created_at"
                        && lower != "updated_at"
                }) {
                    // Build map: row_index → display_text
                    for row in &parent_sheet.grid {
                        if let Some(row_index_str) = row.get(0) {
                            if let Ok(row_index) = row_index_str.parse::<i64>() {
                                if let Some(display_text) = row.get(first_data_col_idx) {
                                    if !display_text.is_empty() {
                                        cache.insert((parent_table.clone(), row_index), display_text.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    trace!("Built parent lookup cache with {} entries", cache.len());
    cache
}
