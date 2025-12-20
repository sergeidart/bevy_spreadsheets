// src/ui/elements/ai_review/navigation.rs
// Navigation logic for AI review drill-down into child tables

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{
    EditorWindowState, NavigationContext, NewRowReview, ParentFilter, RowReview, ReviewChoice,
};
use bevy::prelude::*;

/// Navigate into a child table structure column
/// Saves current state to navigation stack and loads child rows filtered by parent_key
/// 
/// # Arguments
/// * `parent_row_idx` - For existing rows, the row_index from ai_row_reviews
/// * `parent_new_row_idx` - For new rows, the array index in ai_new_row_reviews
pub fn drill_into_structure(
    state: &mut EditorWindowState,
    column_idx: usize,
    parent_row_idx: Option<usize>,
    parent_new_row_idx: Option<usize>,
    registry: &SheetRegistry,
) {
    // Determine which index to use for display/cache lookups
    let row_idx = parent_row_idx.or(parent_new_row_idx).unwrap_or(0);
    let is_new_row = parent_new_row_idx.is_some() && parent_row_idx.is_none();
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
    let parent_display_name = if is_new_row {
        // For new rows, look up by array index in ai_new_row_reviews
        parent_new_row_idx.and_then(|idx| {
            state.ai_new_row_reviews.get(idx).and_then(|nr| nr.ai.get(0).cloned())
        })
    } else {
        // For existing rows, look up by row_index in ai_row_reviews
        parent_row_idx.and_then(|idx| {
            state.ai_row_reviews.iter().find(|r| r.row_index == idx).and_then(|r| r.ai.get(0).cloned())
        })
    };
    
    // CRITICAL: Extract the actual database row_index from parent review's column 0
    // For existing rows: look up from cache using row_index
    // For new rows: use projected_row_index from NewRowReview
    let actual_parent_db_row_index = if is_new_row {
        // For new rows, get projected_row_index from NewRowReview
        parent_new_row_idx.and_then(|idx| {
            state.ai_new_row_reviews.get(idx).map(|nr| nr.projected_row_index)
        }).unwrap_or(row_idx)
    } else if let Some(existing_row_idx) = parent_row_idx {
        // For existing rows, try to get database row_index from cache
        if let Some(cached_full_row) = state.ai_original_row_snapshot_cache.get(&(Some(existing_row_idx), None)) {
            cached_full_row.get(0)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(existing_row_idx)
        } else {
            existing_row_idx
        }
    } else {
        warn!("Could not find parent review for row_idx={}, using review index as fallback", row_idx);
        row_idx
    };
    
    info!(
        "drill_into_structure: parent_row_idx={:?}, parent_new_row_idx={:?}, is_new_row={}, actual_parent_db_row_index={}",
        parent_row_idx, parent_new_row_idx, is_new_row, actual_parent_db_row_index
    );
    
    // First check if there's a StructureReviewEntry with AI data for this structure
    // (check BEFORE switching context)
    let structure_path = vec![column_idx];
    
    info!(
        "Looking for StructureReviewEntry: parent_sheet='{}', parent_category={:?}, parent_row_idx={:?}, parent_new_row_idx={:?}, structure_path={:?}",
        state.ai_current_sheet, state.ai_current_category, parent_row_idx, parent_new_row_idx, structure_path
    );
    
    // Log all available structure reviews
    for (idx, entry) in state.ai_structure_reviews.iter().enumerate() {
        info!(
            "  Entry[{}]: root_category={:?}, root_sheet='{}', parent_row_index={}, parent_new_row_index={:?}, structure_path={:?}",
            idx, entry.root_category, entry.root_sheet, entry.parent_row_index, entry.parent_new_row_index, entry.structure_path
        );
    }
    
    // Match StructureReviewEntry based on whether we're coming from an existing row or new row
    let has_structure_review = state.ai_structure_reviews.iter().any(|entry| {
        let category_matches = &entry.root_category == &state.ai_current_category;
        let sheet_matches = entry.root_sheet == state.ai_current_sheet.as_str();
        let path_matches = entry.structure_path == structure_path;
        
        let parent_matches = if is_new_row {
            // For new rows: match by parent_new_row_index
            entry.parent_new_row_index == parent_new_row_idx
        } else {
            // For existing rows: match by parent_row_index
            Some(entry.parent_row_index) == parent_row_idx && entry.parent_new_row_index.is_none()
        };
        
        category_matches && sheet_matches && path_matches && parent_matches
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
        parent_review_index: Some(row_idx),  // Store the review index for structure entry matching
        is_parent_new_row: is_new_row,       // Track whether parent is from ai_new_row_reviews
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
        parent_display_name: parent_display_name.clone(),
    });
    
    // Note: We don't set ai_structure_detail_context here because navigation drill-down
    // uses the navigation_stack to track state, not the old structure detail system.
    // The plan generation code will check ai_navigation_stack to determine drill-down mode.
    
    // Load child rows into ai_row_reviews (filtered by parent_key or from StructureReviewEntry)
    if has_structure_review {
        // Pass both indices to properly match StructureReviewEntry
        load_child_reviews_from_structure_entry(
            state,
            &child_sheet_name,
            parent_row_idx,
            parent_new_row_idx,
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
/// IMPORTANT: Does NOT persist changes - that happens when parent row is accepted
pub fn navigate_back(state: &mut EditorWindowState, registry: &SheetRegistry) {
    // Clone data we need before mutable borrows
    let parent_ctx_data = state.ai_navigation_stack.last().map(|ctx| {
        (ctx.sheet_name.clone(), ctx.category.clone(), ctx.parent_review_index)
    });
    
    let Some((parent_sheet_name, parent_category, _parent_review_idx_opt)) = parent_ctx_data else {
        warn!("Cannot navigate back: navigation stack is empty");
        return;
    };
    
    // DO NOT persist here - structure changes are saved when parent row is accepted
    // The structure entry remains in ai_structure_reviews with has_changes=true
    // and will be processed by process_existing_accept when parent is accepted
    
    // Now pop the stack and get the full context
    let parent_ctx = state.ai_navigation_stack.pop().unwrap();
    
    info!(
        "Navigating back from {} to {}",
        state.ai_current_sheet, parent_sheet_name
    );
    
    // Restore parent sheet context
    state.ai_current_sheet = parent_sheet_name;
    state.ai_current_category = parent_category;
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
    
    // Build full ancestor values from the ENTIRE navigation stack
    let ancestor_values: Vec<String> = state.ai_navigation_stack
        .iter()
        .filter_map(|nav_ctx| nav_ctx.parent_display_name.clone())
        .collect();
    
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
                        ancestor_key_values: ancestor_values.clone(),
                        ancestor_dropdown_cache: std::collections::HashMap::new(),
                        is_orphan: false,
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
    parent_row_idx: Option<usize>,      // For existing rows
    parent_new_row_idx: Option<usize>,  // For new rows
    actual_parent_db_row_index: usize,  // Database row_index for fallback queries
    column_idx: usize,
    registry: &SheetRegistry,
    parent_display_name: &Option<String>,
) {
    let is_new_row = parent_new_row_idx.is_some() && parent_row_idx.is_none();
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
    
    // Find the StructureReviewEntry using the appropriate index based on parent type
    let structure_path = vec![column_idx];
    let parent_sheet_name = state.ai_navigation_stack.last().map(|ctx| ctx.sheet_name.as_str()).unwrap_or("");
    
    let Some(entry) = state.ai_structure_reviews.iter().find(|e| {
        let category_matches = e.root_category == state.ai_current_category;
        let sheet_matches = e.root_sheet == parent_sheet_name;
        let path_matches = e.structure_path == structure_path;
        
        let parent_matches = if is_new_row {
            e.parent_new_row_index == parent_new_row_idx
        } else {
            Some(e.parent_row_index) == parent_row_idx && e.parent_new_row_index.is_none()
        };
        
        category_matches && sheet_matches && path_matches && parent_matches
    }).cloned() else {
        warn!("No StructureReviewEntry found for parent_row_idx={:?}, parent_new_row_idx={:?}, falling back to database", parent_row_idx, parent_new_row_idx);
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
        "Converting StructureReviewEntry to RowReviews: {} original rows, {} AI rows, {} merged rows, non_structure_columns={:?}",
        entry.original_rows.len(),
        entry.ai_rows.len(),
        entry.merged_rows.len(),
        non_structure_columns
    );
    
    // CRITICAL: Use actual_parent_db_row_index (numeric) for parent_key, NOT display name
    let parent_key_value = actual_parent_db_row_index.to_string();
    let max_rows = std::cmp::max(entry.original_rows.len(), entry.ai_rows.len());
    
    // Build full ancestor values from the ENTIRE navigation stack
    // This includes all ancestors in order from root to immediate parent
    let ancestor_values: Vec<String> = state.ai_navigation_stack
        .iter()
        .filter_map(|nav_ctx| nav_ctx.parent_display_name.clone())
        .collect();
    
    // Process all rows - some may have original, some may have AI, some may have both
    for row_idx in 0..max_rows {
        let original_opt = entry.original_rows.get(row_idx);
        let ai_opt = entry.ai_rows.get(row_idx);
        
        match (original_opt, ai_opt) {
            // Case 1: Both original and AI rows exist - create RowReview
            (Some(original), Some(ai_row)) => {
                // Prepend technical columns (row_index, parent_key) to match full grid format
                let mut full_original = vec![row_idx.to_string(), parent_key_value.to_string()];
                full_original.extend_from_slice(original);
                
                let mut full_ai = vec![row_idx.to_string(), parent_key_value.to_string()];
                full_ai.extend_from_slice(ai_row);
                
                // Build choices from differences
                let choices: Vec<ReviewChoice> = entry.differences.get(row_idx)
                    .map(|diff_row| {
                        non_structure_columns.iter().enumerate().map(|(pos, &col_idx)| {
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
                    ancestor_key_values: ancestor_values.clone(),
                    ancestor_dropdown_cache: std::collections::HashMap::new(),
                    is_orphan: false,
                };
                
                state.ai_row_reviews.push(review);
            }
            
            // Case 2: Only AI row exists (new row added by AI) - create NewRowReview
            (None, Some(ai_row)) => {
                let mut full_ai = vec![row_idx.to_string(), parent_key_value.to_string()];
                full_ai.extend_from_slice(ai_row);
                
                info!(
                    "Creating NewRowReview for AI-only row {}: parent_key='{}', ai_row={:?}, full_ai={:?}",
                    row_idx, parent_key_value, ai_row, full_ai
                );
                
                let new_review = NewRowReview {
                    ai: full_ai,
                    non_structure_columns: non_structure_columns.clone(),
                    duplicate_match_row: None,
                    choices: None,
                    merge_selected: false,
                    merge_decided: false,
                    original_for_merge: None,
                    key_overrides: std::collections::HashMap::new(),
                    ancestor_key_values: ancestor_values.clone(),
                    ancestor_dropdown_cache: std::collections::HashMap::new(),
                    projected_row_index: row_idx, // Use the structure row_index
                    is_orphan: false,
                };
                
                state.ai_new_row_reviews.push(new_review);
            }
            
            // Case 3: Only original row exists (no AI changes) - create RowReview with same data
            (Some(original), None) => {
                let mut full_original = vec![row_idx.to_string(), parent_key_value.to_string()];
                full_original.extend_from_slice(original);
                
                let review = RowReview {
                    row_index: row_idx,
                    original: full_original.clone(),
                    ai: full_original, // AI same as original
                    choices: vec![ReviewChoice::Original; non_structure_columns.len()],
                    non_structure_columns: non_structure_columns.clone(),
                    key_overrides: std::collections::HashMap::new(),
                    ancestor_key_values: ancestor_values.clone(),
                    ancestor_dropdown_cache: std::collections::HashMap::new(),
                    is_orphan: false,
                };
                
                state.ai_row_reviews.push(review);
            }
            
            // Case 4: Neither exists (shouldn't happen) - skip
            (None, None) => {
                warn!("Unexpected: neither original nor AI row at index {}", row_idx);
            }
        }
    }
    
    // Load orphaned rows from this StructureReviewEntry
    // Orphans are displayed in red and can be re-parented via dropdown
    for (orphan_idx, orphan_row) in entry.orphaned_ai_rows.iter().enumerate() {
        // Skip already decided orphans
        if entry.orphaned_decided.get(orphan_idx).copied().unwrap_or(false) {
            continue;
        }
        
        // Get the claimed ancestry for this orphan (what the AI thought was the parent)
        let claimed_ancestry = entry.orphaned_claimed_ancestries
            .get(orphan_idx)
            .cloned()
            .unwrap_or_default();
        
        // Build full AI row with row_index and parent_key placeholders
        let orphan_row_index = 1000 + orphan_idx; // Use high index to avoid collision
        let mut full_ai = vec![orphan_row_index.to_string(), "?".to_string()];
        full_ai.extend_from_slice(orphan_row);
        
        info!(
            "Creating orphaned NewRowReview in child view: orphan_idx={}, claimed_ancestry={:?}",
            orphan_idx, claimed_ancestry
        );
        
        let orphan_review = NewRowReview {
            ai: full_ai,
            non_structure_columns: non_structure_columns.clone(),
            duplicate_match_row: None,
            choices: None,
            merge_selected: false,
            merge_decided: false,
            original_for_merge: None,
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: claimed_ancestry, // Use claimed ancestry (will be red)
            ancestor_dropdown_cache: std::collections::HashMap::new(),
            projected_row_index: orphan_row_index,
            is_orphan: true, // Mark as orphan for red rendering
        };
        
        state.ai_new_row_reviews.push(orphan_review);
    }
    
    info!(
        "Loaded {} existing + {} new child rows from StructureReviewEntry (parent_row_idx={:?}, parent_new_row_idx={:?}, actual_db_row_index={}, parent_key={}) [including {} orphans]",
        state.ai_row_reviews.len(),
        state.ai_new_row_reviews.len(),
        parent_row_idx,
        parent_new_row_idx,
        actual_parent_db_row_index,
        parent_display_name.as_ref().map(|s| s.as_str()).unwrap_or(""),
        entry.orphaned_ai_rows.iter().enumerate()
            .filter(|(i, _)| !entry.orphaned_decided.get(*i).copied().unwrap_or(false))
            .count()
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
