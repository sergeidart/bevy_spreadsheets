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

/// Extracts structure context from editor state (parent_key for structure sheets)
pub(super) fn get_structure_context(
    editor_state: &Option<bevy::prelude::ResMut<EditorWindowState>>,
    sheet_name: &str,
    category: &Option<String>,
) -> Option<String> {
    let state = editor_state.as_ref()?;
    let nav_ctx = state.structure_navigation_stack.last()?;
    
    if &nav_ctx.structure_sheet_name == sheet_name && &nav_ctx.parent_category == category {
        Some(nav_ctx.parent_row_key.clone())
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
