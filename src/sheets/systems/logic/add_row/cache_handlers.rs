// src/sheets/systems/logic/add_row_handlers/cache_handlers.rs
// Cache invalidation and management for add_row operations

use crate::ui::elements::editor::state::EditorWindowState;

/// Invalidates filtered row indices cache for a specific sheet
pub(super) fn invalidate_sheet_cache(
    editor_state: &mut Option<bevy::prelude::ResMut<EditorWindowState>>,
    category: &Option<String>,
    sheet_name: &str,
) {
    if let Some(state_mut) = editor_state.as_mut() {
        state_mut.force_filter_recalculation = true;
        
        // Ensure we scroll to the newly added row at the top
        state_mut.request_scroll_to_new_row = true;
        
        // Remove cached entries for this specific sheet
        let keys_to_remove: Vec<_> = state_mut
            .filtered_row_indices_cache
            .keys()
            .filter(|(cat_opt, s_name)| cat_opt == category && s_name == sheet_name)
            .cloned()
            .collect();
            
        for k in keys_to_remove {
            state_mut.filtered_row_indices_cache.remove(&k);
        }
    }
}

/// Structure context for new row creation (parent_key + ancestor keys)
#[derive(Debug, Clone)]
pub(super) struct StructureContext {
    /// Ancestor row_index values (numeric strings)
    /// Example: ["3770", "1234"]
    /// These are ROW_INDEX VALUES for database persistence (numeric)
    pub ancestor_row_indices: Vec<String>,
}

/// Extracts structure context from editor state (parent_key + ancestor_keys for structure sheets)
pub(super) fn get_structure_context(
    editor_state: &Option<bevy::prelude::ResMut<EditorWindowState>>,
    sheet_name: &str,
    category: &Option<String>,
    registry: &crate::sheets::resources::SheetRegistry,
) -> Option<StructureContext> {
    let state = editor_state.as_ref()?;
    
    // First check virtual_structure_stack (for AI operations in virtual sheets)
    if let Some(vctx) = state.virtual_structure_stack.last() {
        if &vctx.virtual_sheet_name == sheet_name {
            // Extract parent_key from the parent context
            // For virtual structure sheets, we need to get the parent row's key value
            if let Some(parent_sheet) = registry.get_sheet(&vctx.parent.parent_category, &vctx.parent.parent_sheet) {
                if let Some(parent_row) = parent_sheet.grid.get(vctx.parent.parent_row) {
                    if let Some(meta) = &parent_sheet.metadata {
                        // Find the key column index for the structure column
                        if let Some(struct_col) = meta.columns.get(vctx.parent.parent_col) {
                            if let Some(key_col_idx) = struct_col.structure_key_parent_column_index {
                                if let Some(_key_value) = parent_row.get(key_col_idx) {
                                    // For virtual sheets, ancestor_row_indices would need to be extracted from parent row
                                    // TODO: Extract ancestor row indices from parent row
                                    return Some(StructureContext {
                                        ancestor_row_indices: Vec::new(), // TODO: populate from parent row
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Fall back to structure_navigation_stack (for regular structure navigation)
    let nav_ctx = state.structure_navigation_stack.last()?;
    
    if &nav_ctx.structure_sheet_name == sheet_name && &nav_ctx.parent_category == category {
        Some(StructureContext {
            ancestor_row_indices: nav_ctx.ancestor_row_indices.clone(),
        })
    } else {
        None
    }
}

/// Resolves the target sheet name and category from virtual structure context if active
pub(super) fn resolve_virtual_context(
    editor_state: &Option<bevy::prelude::ResMut<EditorWindowState>>,
    mut category: Option<String>,
    mut sheet_name: String,
) -> (Option<String>, String) {
    if let Some(state) = editor_state.as_ref() {
        if let Some(vctx) = state.virtual_structure_stack.last() {
            sheet_name = vctx.virtual_sheet_name.clone();
            category = vctx.parent.parent_category.clone();
        }
    }
    (category, sheet_name)
}
