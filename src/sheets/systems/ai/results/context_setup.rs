// src/sheets/systems/ai/results/context_setup.rs
// Setup AI context prefixes for key columns

use crate::sheets::events::AiBatchTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

/// Setup AI context prefixes for key columns
/// Builds ancestor key column context for virtual structure navigation
pub fn setup_context_prefixes(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ev: &AiBatchTaskResult,
) {
    state.ai_context_only_prefix_count = ev.key_prefix_count;
    state.ai_context_prefix_by_row.clear();

    if state.virtual_structure_stack.is_empty() {
        return;
    }

    let mut key_headers: Vec<String> = Vec::new();
    let mut ancestors_with_keys: Vec<(Option<String>, String, usize, usize)> = Vec::new();

    for vctx in &state.virtual_structure_stack {
        let anc_cat = vctx.parent.parent_category.clone();
        let anc_sheet = vctx.parent.parent_sheet.clone();
        let anc_row_idx = vctx.parent.parent_row;

        if let Some(sheet) = registry.get_sheet(&anc_cat, &anc_sheet) {
            if let Some(meta) = &sheet.metadata {
                if let Some(key_col_index) = meta.columns.iter().find_map(|c| {
                    if matches!(
                        c.validator,
                        Some(crate::sheets::definitions::ColumnValidator::Structure)
                    ) {
                        c.structure_key_parent_column_index
                    } else {
                        None
                    }
                }) {
                    if let Some(col_def) = meta.columns.get(key_col_index) {
                        key_headers.push(col_def.header.clone());
                    }
                    ancestors_with_keys.push((anc_cat, anc_sheet, anc_row_idx, key_col_index));
                }
            }
        }
    }

    if !ancestors_with_keys.is_empty() && !key_headers.is_empty() {
        for &row_index in ev.original_row_indices.iter() {
            let mut pairs: Vec<(String, String)> = Vec::with_capacity(key_headers.len());
            for (idx, (anc_cat, anc_sheet, anc_row_idx, key_col_index)) in
                ancestors_with_keys.iter().enumerate()
            {
                let header = key_headers.get(idx).cloned().unwrap_or_default();
                let val = registry
                    .get_sheet(anc_cat, anc_sheet)
                    .and_then(|s| s.grid.get(*anc_row_idx))
                    .and_then(|r| r.get(*key_col_index))
                    .cloned()
                    .unwrap_or_default();
                pairs.push((header, val));
            }
            state.ai_context_prefix_by_row.insert(row_index, pairs);
        }
    }
}
