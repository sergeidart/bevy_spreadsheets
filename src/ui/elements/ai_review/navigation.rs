// src/ui/elements/ai_review/navigation.rs
// Navigation logic for AI review drill-down into child tables

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{
    EditorWindowState, NavigationContext, ParentFilter, RowReview, ReviewChoice,
};
use bevy::prelude::*;

/// Navigate into a child table structure column
/// Saves current state to navigation stack and loads child rows filtered by parent_key
pub fn drill_into_structure(
    state: &mut EditorWindowState,
    column_idx: usize,
    row_idx: usize,
    registry: &SheetRegistry,
) {
    // Get current sheet metadata to find column header
    let Some(current_sheet) = registry.get_sheet(&state.ai_current_category, &state.ai_current_sheet) else {
        error!("Cannot drill into structure: current sheet '{}' not found", state.ai_current_sheet);
        return;
    };
    
    let Some(metadata) = &current_sheet.metadata else {
        error!("Cannot drill into structure: current sheet '{}' has no metadata", state.ai_current_sheet);
        return;
    };
    
    let Some(column_def) = metadata.columns.get(column_idx) else {
        error!("Cannot drill into structure: column index {} not found", column_idx);
        return;
    };
    
    let column_header = &column_def.header;
    
    // Compute child table name
    let child_sheet_name = format!("{}_{}", state.ai_current_sheet, column_header);
    
    // Check if child table exists
    if registry.get_sheet(&state.ai_current_category, &child_sheet_name).is_none() {
        error!("Cannot drill into structure: child table '{}' not found", child_sheet_name);
        return;
    };
    
    // Get parent row display name for breadcrumb
    // Parent-level reviews don't include technical columns (row_index) in review.ai array,
    // so the first element is the first data column (e.g., Name)
    let parent_display_name = if let Some(review) = state.ai_row_reviews.iter().find(|r| r.row_index == row_idx) {
        // Use first data column value for display (index 0 in review.ai, which is Name)
        review.ai.get(0).cloned()
    } else if let Some(new_review) = state.ai_new_row_reviews.iter().enumerate().find(|(idx, _)| *idx == row_idx) {
        // Use first value from ai row
        new_review.1.ai.get(0).cloned()
    } else {
        None
    };
    
    // CRITICAL: Extract the actual database row_index from parent review's column 0
    // row_idx is the review array index (0, 1, 2...), but we need the database row_index
    // Parent-level AI reviews don't include row_index column in review.ai, so look it up from cache
    let actual_parent_db_row_index = if let Some(_review) = state.ai_row_reviews.iter().find(|r| r.row_index == row_idx) {
        // Try to get the database row_index from the cached full row data
        // The cache key is (Some(row_idx), None) for existing rows
        if let Some(cached_full_row) = state.ai_original_row_snapshot_cache.get(&(Some(row_idx), None)) {
            // Column 0 is row_index
            cached_full_row.get(0)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(row_idx)
        } else {
            row_idx
        }
    } else if let Some(_new_review) = state.ai_new_row_reviews.iter().enumerate().find(|(idx, _)| *idx == row_idx) {
        // For new rows, they don't have database row_index yet, use review index
        row_idx
    } else {
        // Fallback to review index (shouldn't happen)
        warn!("Could not find parent review for row_idx={}, using review index as fallback", row_idx);
        row_idx
    };
    
    info!(
        "drill_into_structure: review_index={}, actual_parent_db_row_index={}",
        row_idx, actual_parent_db_row_index
    );
    
    // First check if there's a StructureReviewEntry with AI data for this structure
    // (check BEFORE switching context)
    let structure_path = vec![column_idx];
    
    info!(
        "Looking for StructureReviewEntry: parent_sheet='{}', parent_category={:?}, row_idx={}, structure_path={:?}",
        state.ai_current_sheet, state.ai_current_category, row_idx, structure_path
    );
    
    // Log all available structure reviews
    for (idx, entry) in state.ai_structure_reviews.iter().enumerate() {
        info!(
            "  Entry[{}]: root_category={:?}, root_sheet='{}', parent_row_index={}, structure_path={:?}",
            idx, entry.root_category, entry.root_sheet, entry.parent_row_index, entry.structure_path
        );
    }
    
    let has_structure_review = state.ai_structure_reviews.iter().any(|entry| {
        &entry.root_category == &state.ai_current_category
            && entry.root_sheet == state.ai_current_sheet.as_str()
            && entry.parent_row_index == row_idx
            && entry.structure_path == structure_path
    });
    
    info!(
        "StructureReviewEntry found: {}",
        has_structure_review
    );
    
    // Push current location to navigation stack with cached reviews
    state.ai_navigation_stack.push(NavigationContext {
        sheet_name: state.ai_current_sheet.clone(),
        category: state.ai_current_category.clone(),
        parent_row_index: Some(actual_parent_db_row_index),
        parent_display_name: parent_display_name.clone(),
        cached_row_reviews: state.ai_row_reviews.clone(),
        cached_new_row_reviews: state.ai_new_row_reviews.clone(),
    });
    
    info!(
        "Drilling into structure: {} (parent row {}) -> {}",
        state.ai_current_sheet, row_idx, child_sheet_name
    );
    
    // Switch to child sheet context
    state.ai_current_sheet = child_sheet_name.clone();
    state.ai_parent_filter = Some(ParentFilter {
        parent_row_index: actual_parent_db_row_index,
    });
    
    // Load child rows into ai_row_reviews (filtered by parent_key or from StructureReviewEntry)
    if has_structure_review {
        // Use review index (row_idx) for StructureReviewEntry lookup,
        // but pass actual database row index for database queries
        load_child_reviews_from_structure_entry(
            state,
            &child_sheet_name,
            row_idx,  // StructureReviewEntry uses review index for storage
            actual_parent_db_row_index,  // Database queries need actual row_index
            column_idx,
            registry,
            &parent_display_name,
        );
    } else {
        load_child_reviews(state, &child_sheet_name, actual_parent_db_row_index, registry);
    }
}

/// Navigate back to parent level
/// Pops navigation stack and restores parent sheet context
pub fn navigate_back(state: &mut EditorWindowState, registry: &SheetRegistry) {
    let Some(parent_ctx) = state.ai_navigation_stack.pop() else {
        warn!("Cannot navigate back: navigation stack is empty");
        return;
    };
    
    info!(
        "Navigating back from {} to {}",
        state.ai_current_sheet, parent_ctx.sheet_name
    );
    
    // Restore parent sheet context
    state.ai_current_sheet = parent_ctx.sheet_name.clone();
    state.ai_current_category = parent_ctx.category.clone();
    state.ai_parent_filter = None;
    
    // Reload parent reviews
    restore_parent_reviews(state, &parent_ctx, registry);
}

/// Load child table rows into ai_row_reviews, filtered by parent_key
fn load_child_reviews(
    state: &mut EditorWindowState,
    child_sheet_name: &str,
    parent_row_idx: usize,
    registry: &SheetRegistry,
) {
    // Clear existing reviews
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    
    let Some(child_sheet) = registry.get_sheet(&state.ai_current_category, child_sheet_name) else {
        error!("Child sheet '{}' not found in registry", child_sheet_name);
        return;
    };
    
    let Some(metadata) = &child_sheet.metadata else {
        error!("Child sheet '{}' has no metadata", child_sheet_name);
        return;
    };
    
    // Filter child rows by parent_key (column 1)
    for (grid_idx, row) in child_sheet.grid.iter().enumerate() {
        if let Some(parent_key_str) = row.get(1) {
            if let Ok(parent_key_val) = parent_key_str.parse::<usize>() {
                if parent_key_val == parent_row_idx {
                    // Create RowReview for this child row
                    let row_index = child_sheet.row_indices.get(grid_idx).copied().unwrap_or(0) as usize;
                    
                    // Build non_structure_columns list (skip only row_index, include parent_key as data column)
                    let mut non_structure_columns = Vec::new();
                    for (col_idx, col_def) in metadata.columns.iter().enumerate() {
                        if col_idx == 0 {
                            continue; // Skip row_index only
                        }
                        if !matches!(col_def.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) {
                            non_structure_columns.push(col_idx);
                        }
                    }
                    
                    let review = RowReview {
                        row_index,
                        original: row.clone(),
                        ai: row.clone(), // No AI changes for drill-down (yet)
                        choices: vec![ReviewChoice::Original; non_structure_columns.len()],
                        non_structure_columns,
                        key_overrides: std::collections::HashMap::new(),
                        ancestor_key_values: Vec::new(),
                        ancestor_dropdown_cache: std::collections::HashMap::new(),
                    };
                    
                    state.ai_row_reviews.push(review);
                }
            }
        }
    }
    
    info!(
        "Loaded {} child rows from {} (filtered by parent_key={})",
        state.ai_row_reviews.len(),
        child_sheet_name,
        parent_row_idx
    );
}

/// Load child reviews from StructureReviewEntry (with AI data)
fn load_child_reviews_from_structure_entry(
    state: &mut EditorWindowState,
    child_sheet_name: &str,
    parent_review_idx: usize,  // Review array index for StructureReviewEntry lookup
    actual_parent_db_row_index: usize,  // Database row_index for fallback queries
    column_idx: usize,
    registry: &SheetRegistry,
    parent_display_name: &Option<String>,
) {
    // Clear existing reviews
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    
    let Some(child_sheet) = registry.get_sheet(&state.ai_current_category, child_sheet_name) else {
        error!("Child sheet '{}' not found in registry", child_sheet_name);
        return;
    };
    
    let Some(metadata) = &child_sheet.metadata else {
        error!("Child sheet '{}' has no metadata", child_sheet_name);
        return;
    };
    
    // Find the StructureReviewEntry using review index
    let structure_path = vec![column_idx];
    let Some(entry) = state.ai_structure_reviews.iter().find(|e| {
        e.root_category == state.ai_current_category
            && e.root_sheet == state.ai_navigation_stack.last().map(|ctx| ctx.sheet_name.as_str()).unwrap_or("")
            && e.parent_row_index == parent_review_idx  // Use review index for lookup
            && e.structure_path == structure_path
    }).cloned() else {
        warn!("No StructureReviewEntry found for parent_review_idx={}, falling back to database", parent_review_idx);
        load_child_reviews(state, child_sheet_name, actual_parent_db_row_index, registry);
        return;
    };
    
    // Build non_structure_columns list (skip only row_index, include parent_key as data column)
    let mut non_structure_columns = Vec::new();
    for (col_idx, col_def) in metadata.columns.iter().enumerate() {
        if col_idx == 0 {
            continue; // Skip row_index only
        }
        if !matches!(col_def.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) {
            non_structure_columns.push(col_idx);
        }
    }
    
    info!(
        "Converting StructureReviewEntry to RowReviews: {} original rows, {} AI rows, non_structure_columns={:?}",
        entry.original_rows.len(),
        entry.ai_rows.len(),
        non_structure_columns
    );
    
    // Convert each row pair to RowReview
    let parent_key_value = parent_display_name.as_ref().map(|s| s.as_str()).unwrap_or("");
    for (row_idx, (original, ai_row)) in entry.original_rows.iter().zip(entry.ai_rows.iter()).enumerate() {
        
        // Prepend technical columns (row_index, parent_key) to match full grid format
        // parent_key should be the parent's display name (e.g., "The Mighty Nein"), not the database row index
        let mut full_original = vec![row_idx.to_string(), parent_key_value.to_string()];
        full_original.extend_from_slice(original);
        
        let mut full_ai = vec![row_idx.to_string(), parent_key_value.to_string()];
        full_ai.extend_from_slice(ai_row);
        
        // Build choices from differences
        // entry.original_rows, entry.ai_rows, and entry.differences don't include technical columns (row_index, parent_key)
        // They only contain data columns (col 2+), so differences[i] corresponds to non_structure_columns that are >= 2
        let choices: Vec<ReviewChoice> = entry.differences.get(row_idx)
            .map(|diff_row| {
                non_structure_columns.iter().enumerate().map(|(pos, &col_idx)| {
                    // If this is parent_key (col 1), it's not in diff_row, default to Original
                    // Otherwise, col_idx 2+ maps to diff_row starting from index 0
                    if col_idx == 1 {
                        ReviewChoice::Original  // parent_key not in AI differences
                    } else if col_idx >= 2 {
                        let diff_idx = pos.saturating_sub(if non_structure_columns.contains(&1) { 1 } else { 0 });
                        if diff_row.get(diff_idx).copied().unwrap_or(false) {
                            ReviewChoice::AI
                        } else {
                            ReviewChoice::Original
                        }
                    } else {
                        ReviewChoice::Original
                    }
                }).collect()
            })
            .unwrap_or_else(|| vec![ReviewChoice::Original; non_structure_columns.len()]);
        
        let review = RowReview {
            row_index: row_idx,
            original: full_original,
            ai: full_ai,
            choices,
            non_structure_columns: non_structure_columns.clone(),
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: Vec::new(),
            ancestor_dropdown_cache: std::collections::HashMap::new(),
        };
        
        state.ai_row_reviews.push(review);
    }
    
    info!(
        "Loaded {} child RowReviews from StructureReviewEntry (parent_review_idx={}, actual_db_row_index={}, parent_key={})",
        state.ai_row_reviews.len(),
        parent_review_idx,
        actual_parent_db_row_index,
        parent_display_name.as_ref().map(|s| s.as_str()).unwrap_or("")
    );
}

/// Restore parent level reviews from cached state
fn restore_parent_reviews(
    state: &mut EditorWindowState,
    parent_ctx: &NavigationContext,
    _registry: &SheetRegistry,
) {
    // Restore cached reviews from navigation context
    state.ai_row_reviews = parent_ctx.cached_row_reviews.clone();
    state.ai_new_row_reviews = parent_ctx.cached_new_row_reviews.clone();
    
    info!(
        "Restored parent reviews: {} existing rows, {} new rows",
        state.ai_row_reviews.len(),
        state.ai_new_row_reviews.len()
    );
}
