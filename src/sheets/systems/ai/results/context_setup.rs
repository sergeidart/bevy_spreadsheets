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
    // Do NOT clear existing prefixes here. For non-virtual reviews, prefixes were
    // stored at send time and should remain available for rendering.

    if state.virtual_structure_stack.is_empty() {
        // No virtual context to rebuild; keep any existing prefixes intact.
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

                // After migration, ancestor key values are row_index (numeric)
                // Resolve to display text for human-readable AI context
                let display_val = resolve_row_index_to_display_text(&val, anc_cat, anc_sheet, registry)
                    .unwrap_or(val);

                pairs.push((header, display_val));
            }
            state.ai_context_prefix_by_row.insert(row_index, pairs);
        }
    }
}

/// Resolves a row_index value to human-readable display text from the parent row
///
/// After migration, ancestor key columns store numeric row_index values.
/// This function looks up the row by row_index and returns the display value
/// from the first data column for AI context.
///
/// Returns Some(display_text) if successful, None if value is not numeric or row not found.
fn resolve_row_index_to_display_text(
    value: &str,
    parent_category: &Option<String>,
    parent_sheet: &str,
    registry: &SheetRegistry,
) -> Option<String> {
    // Try to parse as row_index (numeric)
    let row_index_value = value.parse::<i64>().ok()?;

    // Get parent sheet from registry
    let sheet = registry.get_sheet(parent_category, parent_sheet)?;

    // Find the row with matching row_index
    // row_index is stored in column 0 for all tables
    let row = sheet.grid.iter().find(|row| {
        row.get(0)
            .and_then(|idx_str| idx_str.parse::<i64>().ok())
            .map(|idx| idx == row_index_value)
            .unwrap_or(false)
    })?;

    // Get the display value from the first data column
    let metadata = sheet.metadata.as_ref()?;

    // Find first non-technical column
    let first_data_col_idx = metadata.columns.iter().position(|col| {
        let lower = col.header.to_lowercase();
        lower != "row_index"
            && lower != "parent_key"
            && !lower.starts_with("grand_")
            && lower != "id"
            && lower != "created_at"
            && lower != "updated_at"
    })?;

    // Get display text from that column
    let display_text = row.get(first_data_col_idx)?.clone();

    if display_text.is_empty() {
        None
    } else {
        Some(display_text)
    }
}
