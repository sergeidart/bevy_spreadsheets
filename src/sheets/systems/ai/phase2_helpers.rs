// src/sheets/systems/ai/phase2_helpers.rs
// Duplicate detection helpers for AI batch processing

use bevy::prelude::*;

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

use super::row_helpers::{
    extract_ai_snapshot_from_new_row, normalize_cell_value, skip_key_prefix,
};
use super::column_helpers::calculate_dynamic_prefix;
use super::duplicate_map_helpers::build_composite_duplicate_map_for_parents;

/// Detect which new rows are duplicates of existing rows (by first column)
pub fn detect_duplicate_indices(
    extra_slice: &[Vec<String>],
    included: &[usize],
    _key_prefix_count: usize,
    state: &EditorWindowState,
    registry: &SheetRegistry,
) -> Vec<usize> {
    let mut duplicate_indices = Vec::new();
    let (cat_ctx, sheet_ctx) = state.current_sheet_context();

    info!(
        "detect_duplicate_indices: extra_slice.len()={}, included={:?}, sheet_ctx={:?}",
        extra_slice.len(),
        included,
        sheet_ctx
    );

    // Check each new row using parent context from navigation state
    for (new_idx, new_row_full) in extra_slice.iter().enumerate() {
        // Infer per-row prefix count based on inbound row length
        let dynamic_prefix = calculate_dynamic_prefix(new_row_full.len(), included.len());
        
        // Extract parent prefix values from AI response (for logging/debugging only)
        // NOTE: These are NOT used for filtering when navigation_stack exists!
        // The actual parent filtering uses ancestor_row_indices from navigation state.
        let ai_provided_parent_names: Vec<String> = new_row_full
            .iter()
            .take(dynamic_prefix)
            .cloned()
            .collect();
        
        let new_row = skip_key_prefix(new_row_full, dynamic_prefix);

        if new_row.len() < included.len() {
            continue;
        }

        let ai_snapshot = extract_ai_snapshot_from_new_row(new_row, included);

        // Build a composite duplicate map using actual parent context from navigation state
        // When navigation_stack exists, convert_parent_names_to_row_indices ignores ai_provided_parent_names
        // and uses ancestor_row_indices directly for correct parent filtering
        let composite_map = build_composite_duplicate_map_for_parents(
            &ai_provided_parent_names,  // Passed but ignored when navigation_stack exists
            included,
            &cat_ctx,
            &sheet_ctx,
            registry,
            state,
        );

        if new_idx == 0 {
            info!(
                "Composite map for AI-provided parents {:?}: {} existing rows (actual filtering uses navigation state)",
                ai_provided_parent_names,
                composite_map.len()
            );
        }

        // Build composite key from AI values: just the data columns (normalized)
        // Note: parent filtering is done by the map builder, not by adding parent indices to the key
        let mut ai_composite_parts: Vec<String> = Vec::with_capacity(ai_snapshot.len());

        // Add AI data column values to the key (normalized)
        for ai_val in &ai_snapshot {
            ai_composite_parts.push(normalize_cell_value(ai_val));
        }

        let ai_composite = ai_composite_parts.join("||");

        info!(
            "Row {}: ai_provided_parents={:?}, ai_snapshot={:?}, ai_composite='{}', in_map={}",
            new_idx,
            ai_provided_parent_names,
            ai_snapshot,
            ai_composite,
            composite_map.contains_key(&ai_composite)
        );

        if composite_map.contains_key(&ai_composite) {
            duplicate_indices.push(new_idx);
        }
    }

    info!("Final duplicate_indices={:?}", duplicate_indices);
    duplicate_indices
}
