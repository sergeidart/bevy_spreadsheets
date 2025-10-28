// src/sheets/systems/ai/original_cache.rs
// Helper functions for caching original row data during AI review processing

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

/// Cache original row data for AI review
/// 
/// This helper caches the full grid row for rendering original previews in the UI.
/// It includes raw structure JSON for on-demand parsing or lookup in StructureReviewEntry.
/// 
/// # Arguments
/// * `state` - The editor window state containing the cache
/// * `registry` - The sheet registry to look up rows
/// * `cat_ctx` - The category context (optional)
/// * `sheet_ctx` - The sheet name context (optional)
/// * `existing_row_idx` - Row index if caching an existing row (for RowReview)
/// * `new_row_idx` - New row index if caching for a new row (for NewRowReview)
pub fn cache_original_row_for_review(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    cat_ctx: &Option<String>,
    sheet_ctx: &Option<String>,
    existing_row_idx: Option<usize>,
    new_row_idx: Option<usize>,
) {
    let Some(sheet_name) = sheet_ctx else {
        return;
    };

    let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_name) else {
        return;
    };

    if let Some(row_idx) = existing_row_idx {
        // Cache existing row for review
        if let Some(full_row) = sheet_ref.grid.get(row_idx) {
            state
                .ai_original_row_snapshot_cache
                .insert((Some(row_idx), new_row_idx), full_row.clone());
        }
    } else if new_row_idx.is_some() {
        // Cache empty row for new additions
        if let Some(meta) = &sheet_ref.metadata {
            let empty_row = vec![String::new(); meta.columns.len()];
            state
                .ai_original_row_snapshot_cache
                .insert((None, new_row_idx), empty_row);
        }
    }
}
