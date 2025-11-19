use crate::sheets::definitions::ColumnValidator;
use crate::sheets::events::{AddSheetRowRequest, UpdateCellEvent};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai_review::cache_handlers::cancel_batch;
use crate::sheets::systems::ai_review::structure_persistence::persist_structure_detail_changes;
use crate::ui::elements::ai_review::handlers::{
    process_existing_accept, process_new_accept,
};
use crate::ui::elements::ai_review::structure_review_helpers::{
    build_structure_columns,
};
use crate::ui::elements::editor::state::{EditorWindowState, StructureDetailContext};
use bevy::prelude::*;

#[derive(Debug, Clone, Copy)]
pub enum ColumnEntry {
    Regular(usize),   // Index into non_structure_columns
    Structure(usize), // Original column index from sheet metadata
}

/// NOTE: Legacy structure detail hydration removed.
/// The AI review now always renders a unified table and
/// uses `ai_structure_detail_context` only as a filter
/// when preparing row plans and display context.
pub fn hydrate_structure_detail_if_needed(_state: &mut EditorWindowState) {}

/// Checks if the review should auto-exit due to no remaining items
pub fn should_auto_exit(state: &EditorWindowState, in_structure_mode: bool) -> bool {
    let has_undecided_structures = state
        .ai_structure_reviews
        .iter()
        .any(|entry| entry.is_undecided());
    
    // Don't auto-exit if in navigation drilldown mode (child table view)
    let in_navigation_drilldown = !state.ai_navigation_stack.is_empty();
    
    state.ai_row_reviews.is_empty()
        && state.ai_new_row_reviews.is_empty()
        && !in_structure_mode
        && !in_navigation_drilldown
        && !has_undecided_structures
}

/// Resolves the active sheet name from state
pub fn resolve_active_sheet_name(
    state: &mut EditorWindowState,
    selected_sheet_name_clone: &Option<String>,
    _in_structure_mode: bool,
) -> Option<String> {
    // If we're in navigation drill-down mode (child table view), use ai_current_sheet
    if !state.ai_navigation_stack.is_empty() {
        return Some(state.ai_current_sheet.clone());
    }
    
    // Otherwise use the selected sheet
    if let Some(s) = selected_sheet_name_clone {
        Some(s.clone())
    } else {
        None
    }
}

/// Builds the union of non-structure columns from reviews
pub fn build_union_columns(state: &EditorWindowState) -> Vec<usize> {
    let mut union_cols: Vec<usize> = state
        .ai_row_reviews
        .first()
        .map(|r| r.non_structure_columns.clone())
        .unwrap_or_else(|| {
            state
                .ai_new_row_reviews
                .first()
                .map(|r| r.non_structure_columns.clone())
                .unwrap_or_default()
        });
    union_cols.sort_unstable();
    union_cols.dedup();
    union_cols
}

/// Builds merged column list based on review mode
pub fn build_merged_columns(
    state: &EditorWindowState,
    union_cols: &[usize],
    in_structure_mode: bool,
    in_virtual_structure_review: bool,
    active_sheet_name: &str,
    selected_category_clone: &Option<String>,
    registry: &SheetRegistry,
) -> (Vec<ColumnEntry>, Vec<crate::sheets::definitions::StructureFieldDefinition>) {
    if in_structure_mode {
        // In structure detail mode OR navigation drill-down: build columns from child schema
        let detail_context = if let Some(ctx) = &state.ai_structure_detail_context {
            // Use existing structure detail context
            Some(ctx.clone())
        } else if !state.ai_navigation_stack.is_empty() {
            // We're in navigation drill-down - build virtual context from navigation stack
            let parent_ctx = state.ai_navigation_stack.last().unwrap();
            
            // Extract column_idx from child sheet name (format: "ParentSheet_ColumnName")
            // Parse the current sheet name to determine which structure column we drilled into
            let column_idx = if let Some(parent_sheet) = registry.get_sheet(&parent_ctx.category, &parent_ctx.sheet_name) {
                parent_sheet.metadata.as_ref().and_then(|meta| {
                    // Find column by reconstructing child table name pattern
                    meta.columns.iter().enumerate().find(|(_, col_def)| {
                        let expected_child_name = format!("{}_{}", parent_ctx.sheet_name, col_def.header);
                        expected_child_name == active_sheet_name
                    }).map(|(idx, _)| idx)
                })
            } else {
                None
            };
            
            if let Some(col_idx) = column_idx {
                Some(StructureDetailContext {
                    root_category: parent_ctx.category.clone(),
                    root_sheet: parent_ctx.sheet_name.clone(),
                    parent_row_index: parent_ctx.parent_row_index,
                    parent_new_row_index: None,
                    structure_path: vec![col_idx],
                    hydrated: true,
                    saved_row_reviews: Vec::new(),
                    saved_new_row_reviews: Vec::new(),
                })
            } else {
                warn!("Could not determine column index for navigation drill-down into '{}'", active_sheet_name);
                None
            }
        } else {
            None
        };
        
        build_structure_columns(
            union_cols,
            &detail_context,
            false,
            "",
            selected_category_clone,
            registry,
        )
    } else if in_virtual_structure_review {
        // In virtual structure review: filter out structure columns
        build_structure_columns(
            union_cols,
            &None,
            true,
            active_sheet_name,
            selected_category_clone,
            registry,
        )
    } else {
        // Normal mode: build columns from sheet metadata
        // IMPORTANT: Only show columns that are BOTH included in metadata AND present in union_cols (actually sent)
        let cols =
            if let Some(sheet) = registry.get_sheet(selected_category_clone, active_sheet_name) {
                if let Some(metadata) = &sheet.metadata {
                    let mut result = Vec::new();
                    for (col_idx, col_def) in metadata.columns.iter().enumerate() {
                        let is_structure =
                            matches!(col_def.validator, Some(ColumnValidator::Structure));

                        // Skip technical columns entirely
                        let is_technical = col_def.header.eq_ignore_ascii_case("row_index")
                            || col_def.header.eq_ignore_ascii_case("parent_key");
                        if is_technical {
                            continue;
                        }

                        // Only show columns that were actually sent
                        if is_structure {
                            // Structure columns: Check if this structure path was planned for sending
                            // Structure data is sent separately via ai_planned_structure_paths
                            let structure_path = vec![col_idx];
                            if state.ai_planned_structure_paths.contains(&structure_path) {
                                result.push(ColumnEntry::Structure(col_idx));
                            }
                        } else if union_cols.contains(&col_idx) {
                            // Regular columns: only show if present in union_cols (actually sent to AI)
                            result.push(ColumnEntry::Regular(col_idx));
                        }
                    }
                    result
                } else {
                    union_cols
                        .iter()
                        .map(|&idx| ColumnEntry::Regular(idx))
                        .collect()
                }
            } else {
                union_cols
                    .iter()
                    .map(|&idx| ColumnEntry::Regular(idx))
                    .collect()
            };
        (cols, Vec::new())
    }
}

/// Gathers ancestor key columns for display
/// For navigation drill-down mode, builds from navigation stack
/// For legacy structure detail mode, returns empty (deprecated)
pub fn gather_ancestor_key_columns(
    state: &EditorWindowState,
    in_structure_mode: bool,
    active_sheet_name: &str,
    selected_category_clone: &Option<String>,
    registry: &SheetRegistry,
) -> Vec<(String, String)> {
    if in_structure_mode {
        // Navigation drill-down mode: build ancestor keys from navigation stack
        if !state.ai_navigation_stack.is_empty() {
            // We're in a child table - show parent_key column as ancestor
            let parent_ctx = state.ai_navigation_stack.last().unwrap();
            
            // Get parent_key display value from cached parent display name
            let parent_key_value = parent_ctx.parent_display_name.clone()
                .unwrap_or_else(|| "?".to_string());
            
            // Get child sheet metadata to get parent_key column header
            if let Some(child_sheet) = registry.get_sheet(selected_category_clone, active_sheet_name) {
                if let Some(metadata) = &child_sheet.metadata {
                    // parent_key is always at column index 1 in structure tables
                    if let Some(col_def) = metadata.columns.get(1) {
                        return vec![(col_def.header.clone(), parent_key_value)];
                    }
                }
            }
            
            // Fallback
            return vec![("parent_key".to_string(), parent_key_value)];
        }
        
        // Old structure detail mode - deprecated, returns empty
        Vec::new()
    } else {
        // Fallback: if we have stored context prefixes for the current reviews, use them
        if let Some(rr) = state.ai_row_reviews.first() {
            if let Some(pairs) = state.ai_context_prefix_by_row.get(&rr.row_index) {
                return pairs.clone();
            }
        }
        Vec::new()
    }
}

/// Updates pending merge and structure state flags
pub fn update_review_state_flags(state: &mut EditorWindowState) {
    state.ai_batch_has_undecided_merge = state
        .ai_new_row_reviews
        .iter()
        .any(|nr| nr.duplicate_match_row.is_some() && !nr.merge_decided);
}

/// Checks if there are undecided structures
pub fn has_undecided_structures(state: &EditorWindowState) -> bool {
    state
        .ai_structure_reviews
        .iter()
        .any(|entry| entry.is_undecided())
}

/// Processes accept all action in structure mode
/// Marks the entry as accepted - actual database writes handled by base-level accept logic
pub fn process_accept_all_structure_mode(
    state: &mut EditorWindowState,
    detail_ctx: &StructureDetailContext,
) {
    // Persist respects user's cell-by-cell choices (Original vs AI)
    persist_structure_detail_changes(state, detail_ctx);

    // Mark the entry as accepted
    if let Some(entry) = state.ai_structure_reviews.iter_mut().find(|sr| {
        match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
            (Some(a), Some(b)) if a == b => {
                sr.structure_path == detail_ctx.structure_path
            }
            (None, None) => {
                sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX)
                    && sr.structure_path == detail_ctx.structure_path
            }
            _ => false,
        }
    }) {
        entry.accepted = true;
        entry.rejected = false;
        entry.decided = true;
    }
    
    // Clear detail context; unified table remains in place
    state.ai_structure_detail_context = None;
}

/// Processes accept all action in normal mode
pub fn process_accept_all_normal_mode(
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    active_sheet_name: &str,
    cell_update_writer: &mut EventWriter<UpdateCellEvent>,
    add_row_writer: &mut EventWriter<AddSheetRowRequest>,
    registry: &SheetRegistry,
) {
    let existing_indices: Vec<usize> = (0..state.ai_row_reviews.len()).collect();
    let new_indices: Vec<usize> = state
        .ai_new_row_reviews
        .iter()
        .enumerate()
        .filter(|(_, nr)| nr.duplicate_match_row.is_none() || nr.merge_decided)
        .map(|(i, _)| i)
        .collect();
    process_existing_accept(
        &existing_indices,
        state,
        selected_category_clone,
        active_sheet_name,
        cell_update_writer,
        registry,
    );
    process_new_accept(
        &new_indices,
        state,
        selected_category_clone,
        active_sheet_name,
        cell_update_writer,
        add_row_writer,
        registry,
    );
    cancel_batch(state);
}

/// Processes decline all action in structure mode
pub fn process_decline_all_structure_mode(state: &mut EditorWindowState) {
    if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
        if let Some(entry) = state.ai_structure_reviews.iter_mut().find(|sr| {
            match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
                (Some(a), Some(b)) if a == b => {
                    sr.structure_path == detail_ctx.structure_path
                }
                (None, None) => {
                    sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX)
                        && sr.structure_path == detail_ctx.structure_path
                }
                _ => false,
            }
        }) {
            entry.accepted = false;
            entry.rejected = true;
            entry.decided = true;
        }
    }
    state.ai_structure_detail_context = None;
}
