// src/sheets/systems/ai/results/row_processors.rs
// Row processing logic for AI batch results

use bevy::prelude::*;
use std::collections::HashMap;

use crate::sheets::events::AiBatchTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::definitions::ColumnValidator;
use crate::ui::elements::editor::state::{EditorWindowState, NewRowReview, RowReview};

use crate::sheets::systems::ai::row_helpers::{
    create_row_snapshots, extract_ai_snapshot_from_new_row, extract_original_snapshot_for_merge,
    generate_review_choices, normalize_cell_value, skip_key_prefix,
};

use super::duplicate_detection::check_for_duplicate;

/// Process original (existing) rows from batch result
pub fn process_original_rows(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ev: &AiBatchTaskResult,
    orig_slice: &[Vec<String>],
) {
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    state.ai_structure_reviews.clear();

    let included = &ev.included_non_structure_columns;
    let (cat_ctx, sheet_ctx) = state.current_sheet_context();

    for (i, &row_index) in ev.original_row_indices.iter().enumerate() {
        let suggestion_full = &orig_slice[i];
        // Infer prefix count from inbound row: total_len - included_len
        let dynamic_prefix = suggestion_full
            .len()
            .saturating_sub(included.len());
        let suggestion = skip_key_prefix(suggestion_full, dynamic_prefix);

        if suggestion.len() < included.len() {
            warn!(
                "Skipping malformed original suggestion row {}: suggestion_cols={} < included_cols={} (full_len={}, key_prefix_count={})",
                row_index,
                suggestion.len(),
                included.len(),
                suggestion_full.len(),
                ev.key_prefix_count
            );
            continue;
        }

        let Some(sheet_name) = &sheet_ctx else {
            continue;
        };

        let (original_snapshot, ai_snapshot) = create_row_snapshots(
            registry, &cat_ctx, sheet_name, row_index, suggestion, included,
        );

        let choices = generate_review_choices(&original_snapshot, &ai_snapshot);

        state.ai_row_reviews.push(RowReview {
            row_index,
            original: original_snapshot,
            ai: ai_snapshot,
            choices,
            non_structure_columns: included.clone(),
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: Vec::new(),
            ancestor_dropdown_cache: std::collections::HashMap::new(),
        });

        // CACHE POPULATION: Store full grid row for rendering original previews
        // Includes raw structure JSON for on-demand parsing or lookup in StructureReviewEntry
        if let Some(sheet_name) = &sheet_ctx {
            if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                if let Some(full_row) = sheet_ref.grid.get(row_index) {
                    state
                        .ai_original_row_snapshot_cache
                        .insert((Some(row_index), None), full_row.clone());
                }
            }
        }
    }
}

/// Process new (AI-added) rows from batch result
pub fn process_new_rows(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ev: &AiBatchTaskResult,
    extra_slice: &[Vec<String>],
) {
    let included = &ev.included_non_structure_columns;
    let (cat_ctx, sheet_ctx) = state.current_sheet_context();

    // Build duplicate detection map constrained to the same ancestor chain when in a structure sheet
    // When AI sends rows with parent prefixes, we need to:
    // 1. Extract parent names from the prefix columns (human-readable)
    // 2. Convert parent names to row_index values
    // 3. Filter existing rows to only those matching ALL parent row_index values

    // Choose a key column to detect duplicates that is NOT a technical column; fallback to first included
    let key_actual_col_opt = included.iter().copied().find(|&c| c != 1).or_else(|| included.first().copied());
    let mut first_col_value_to_row: HashMap<String, usize> = HashMap::new();
    if let Some(first_col_actual) = key_actual_col_opt {
        if let Some(sheet_name) = &sheet_ctx {
            if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                let meta_opt = sheet_ref.metadata.as_ref();

                // Check if we're in a virtual structure (child table from JSON)
                let is_virtual_sheet = !state.virtual_structure_stack.is_empty()
                    && state.virtual_structure_stack.iter().any(|vctx| &vctx.virtual_sheet_name == sheet_name);

                // For virtual sheets: All rows belong to the same parent, so check against all
                // For real tables: Check parent_key columns if they exist
                if is_virtual_sheet {
                    // Virtual sheet - all rows already belong to same parent, check against all rows
                    for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
                        if let Some(val) = row.get(first_col_actual) {
                            let norm = normalize_cell_value(val);
                            if !norm.is_empty() {
                                first_col_value_to_row.entry(norm).or_insert(row_idx);
                            }
                        }
                    }
                } else {
                    // Real table (not virtual) - check parent_key columns to filter by ancestor chain
                    let has_ancestors = !state.virtual_structure_stack.is_empty();
                    let expected_parent_indices: Vec<usize> = if has_ancestors {
                        state.virtual_structure_stack.iter().map(|vctx| vctx.parent.parent_row).collect()
                    } else {
                        Vec::new()
                    };

                    let (parent_key_col, grand_cols): (Option<usize>, Vec<usize>) = if let Some(meta) = meta_opt {
                        let pk = meta.columns.iter().position(|c| c.header.eq_ignore_ascii_case("parent_key"));
                        let gcols = meta
                            .columns
                            .iter()
                            .enumerate()
                            .filter(|(_, c)| c.header.starts_with("grand_") && c.header.ends_with("_parent"))
                            .map(|(i, _)| i)
                            .collect();
                        (pk, gcols)
                    } else { (None, Vec::new()) };

                    for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
                        // If we have ancestors, only include rows matching ALL parent row_index values
                        if has_ancestors && !expected_parent_indices.is_empty() {
                            let mut all_parents_match = true;

                            // Check parent_key (immediate parent - last in stack)
                            if let Some(pk_idx) = parent_key_col {
                                if let Some(&expected_parent_idx) = expected_parent_indices.last() {
                                    let actual_value = row.get(pk_idx).cloned().unwrap_or_default();
                                    let expected_value = expected_parent_idx.to_string();
                                    if actual_value != expected_value {
                                        all_parents_match = false;
                                    }
                                }
                            }

                            // Check grand_N_parent columns
                            if all_parents_match && expected_parent_indices.len() > 1 {
                                for (grand_n, &gcol) in grand_cols.iter().enumerate() {
                                    let n = grand_n + 1;
                                    if let Some(&expected_idx) = expected_parent_indices.get(expected_parent_indices.len().saturating_sub(n + 1)) {
                                        let actual_value = row.get(gcol).cloned().unwrap_or_default();
                                        let expected_value = expected_idx.to_string();
                                        if actual_value != expected_value {
                                            all_parents_match = false;
                                            break;
                                        }
                                    }
                                }
                            }

                            if !all_parents_match {
                                continue;
                            }
                        }

                        if let Some(val) = row.get(first_col_actual) {
                            let norm = normalize_cell_value(val);
                            if !norm.is_empty() {
                                first_col_value_to_row.entry(norm).or_insert(row_idx);
                            }
                        }
                    }
                }
            }
        }
    }

    for new_row_full in extra_slice.iter() {
        // Extract parent prefix values (human-readable names like "Battlefield 2142")
        let dynamic_prefix = new_row_full
            .len()
            .saturating_sub(included.len());
        let parent_prefix_values: Vec<String> = new_row_full
            .iter()
            .take(dynamic_prefix)
            .cloned()
            .collect();

        info!(
            "Processing new AI row: key_prefix_count={}, parent_prefix_values={:?}, full_row={:?}",
            ev.key_prefix_count, parent_prefix_values, new_row_full
        );

        let new_row = skip_key_prefix(new_row_full, dynamic_prefix);

        if new_row.len() < included.len() {
            warn!(
                "Skipping malformed new suggestion row (cols {} < included_cols={} full_len={} key_prefix_count={})",
                new_row.len(),
                included.len(),
                new_row_full.len(),
                ev.key_prefix_count
            );
            continue;
        }

        let ai_snapshot = extract_ai_snapshot_from_new_row(new_row, included);

        // Build composite duplicate map for this row's parent chain
        let composite_map = build_composite_duplicate_map_for_parents(
            &parent_prefix_values,
            included,
            &cat_ctx,
            &sheet_ctx,
            registry,
            state,
        );

        // Build composite key from AI values
        let ai_composite = ai_snapshot
            .iter()
            .map(|s| normalize_cell_value(s))
            .collect::<Vec<_>>()
            .join("||");

        let duplicate_match_row = composite_map.get(&ai_composite).copied();
        let (choices, original_for_merge, merge_selected) = if let Some(matched_idx) = duplicate_match_row {
            // Extract original snapshot and build choices
            if let Some(sheet_name) = &sheet_ctx {
                if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                    if let Some(existing_row) = sheet_ref.grid.get(matched_idx) {
                        let orig_vec = extract_original_snapshot_for_merge(existing_row, included);
                        let choices = generate_review_choices(&orig_vec, &ai_snapshot);
                        (Some(choices), Some(orig_vec), true)
                    } else {
                        (None, None, true)
                    }
                } else {
                    (None, None, true)
                }
            } else {
                (None, None, true)
            }
        } else {
            (None, None, false)
        };

        info!(
            "Duplicate check (composite) result: duplicate_match_row={:?}",
            duplicate_match_row,
        );

        let new_row_idx = state.ai_new_row_reviews.len();
        state.ai_new_row_reviews.push(NewRowReview {
            ai: ai_snapshot.clone(),
            non_structure_columns: included.clone(),
            duplicate_match_row,
            choices,
            merge_selected,
            merge_decided: false,
            original_for_merge: original_for_merge.clone(),
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: Vec::new(),
            ancestor_dropdown_cache: std::collections::HashMap::new(),
        });

        // CACHE POPULATION: Store snapshot for new/duplicate rows
        // - Duplicates: Use matched existing row (includes structure JSON)
        // - New rows: Empty snapshot (no original content)
        // This unifies preview rendering across all row types (existing/new/duplicate)
        if let Some(matched_idx) = duplicate_match_row {
            if let Some(sheet_name) = &sheet_ctx {
                if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                    if let Some(full_row) = sheet_ref.grid.get(matched_idx) {
                        state
                            .ai_original_row_snapshot_cache
                            .insert((None, Some(new_row_idx)), full_row.clone());
                    }
                }
            }
        } else {
            // Truly new rows (no duplicate): empty snapshot matching column count
            if let Some(sheet_name) = &sheet_ctx {
                if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                    if let Some(meta) = &sheet_ref.metadata {
                        let empty_row = vec![String::new(); meta.columns.len()];
                        state
                            .ai_original_row_snapshot_cache
                            .insert((None, Some(new_row_idx)), empty_row);
                    }
                }
            }
        }
    }
}

/// Build a duplicate detection map filtered by parent context
/// This ensures duplicate checking only considers rows with matching parent chain
pub(crate) fn build_duplicate_map_for_parents(
    parent_prefix_values: &[String],
    key_actual_col_opt: Option<usize>,
    cat_ctx: &Option<String>,
    sheet_ctx: &Option<String>,
    registry: &SheetRegistry,
    state: &EditorWindowState,
    _included: &[usize],
) -> HashMap<String, usize> {
    let mut map: HashMap<String, usize> = HashMap::new();

    let Some(first_col_actual) = key_actual_col_opt else {
        return map;
    };

    let Some(sheet_name) = sheet_ctx else {
        return map;
    };

    let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_name) else {
        return map;
    };

    // If no parent prefix, check against all rows (base table)
    if parent_prefix_values.is_empty() {
        // Determine if key column is a Linked column to resolve comparable text
        let linked_info = sheet_ref
            .metadata
            .as_ref()
            .and_then(|meta| meta.columns.get(first_col_actual))
            .and_then(|c| match &c.validator {
                Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                    Some((target_sheet_name.clone(), *target_column_index))
                }
                _ => None,
            });

        for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
            if let Some(val) = row.get(first_col_actual) {
                // Insert raw normalized value
                let norm_raw = normalize_cell_value(val);
                if !norm_raw.is_empty() {
                    map.entry(norm_raw).or_insert(row_idx);
                }
                // If linked, also resolve to display text and insert that key too
                if let Some((ref target_sheet_name, target_col_idx)) = linked_info {
                    if let Some(display_norm) = resolve_linked_display_value(
                        registry,
                        cat_ctx,
                        target_sheet_name,
                        target_col_idx,
                        val,
                    ) {
                        if !display_norm.is_empty() {
                            map.entry(display_norm).or_insert(row_idx);
                        }
                    }
                }
            }
        }
        return map;
    }

    // Convert parent names to row_index values by looking them up in parent tables
    let parent_row_indices = convert_parent_names_to_row_indices(parent_prefix_values, state, registry);

    // Get parent column indices
    let meta_opt = sheet_ref.metadata.as_ref();
    let (parent_key_col, grand_cols): (Option<usize>, Vec<usize>) = if let Some(meta) = meta_opt {
        let pk = meta.columns.iter().position(|c| c.header.eq_ignore_ascii_case("parent_key"));
        let gcols = meta
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.header.starts_with("grand_") && c.header.ends_with("_parent"))
            .map(|(i, _)| i)
            .collect();
        (pk, gcols)
    } else {
        return map;
    };

    // Determine if key column is a Linked column to resolve comparable text
    let linked_info = sheet_ref
        .metadata
        .as_ref()
        .and_then(|meta| meta.columns.get(first_col_actual))
        .and_then(|c| match &c.validator {
            Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                Some((target_sheet_name.clone(), *target_column_index))
            }
            _ => None,
        });

    // Filter rows to only those matching ALL parent row_index values
    for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
        if !parent_row_indices.is_empty() {
            let mut all_parents_match = true;

            // Check parent_key (immediate parent - last in chain)
            if let Some(pk_idx) = parent_key_col {
                if let Some(&expected_parent_idx) = parent_row_indices.last() {
                    let actual_value = row.get(pk_idx).cloned().unwrap_or_default();
                    let expected_value = expected_parent_idx.to_string();
                    if actual_value != expected_value {
                        all_parents_match = false;
                    }
                }
            }

            // Check grand_N_parent columns
            if all_parents_match && parent_row_indices.len() > 1 {
                for (grand_n, &gcol) in grand_cols.iter().enumerate() {
                    let n = grand_n + 1;
                    if let Some(&expected_idx) = parent_row_indices.get(parent_row_indices.len().saturating_sub(n + 1)) {
                        let actual_value = row.get(gcol).cloned().unwrap_or_default();
                        let expected_value = expected_idx.to_string();
                        if actual_value != expected_value {
                            all_parents_match = false;
                            break;
                        }
                    }
                }
            }

            if !all_parents_match {
                continue;
            }
        }

        if let Some(val) = row.get(first_col_actual) {
            // Raw value key
            let norm_raw = normalize_cell_value(val);
            if !norm_raw.is_empty() {
                map.entry(norm_raw).or_insert(row_idx);
            }
            // Linked display key (if applicable)
            if let Some((ref target_sheet_name, target_col_idx)) = linked_info {
                if let Some(display_norm) = resolve_linked_display_value(
                    registry,
                    cat_ctx,
                    target_sheet_name,
                    target_col_idx,
                    val,
                ) {
                    if !display_norm.is_empty() {
                        map.entry(display_norm).or_insert(row_idx);
                    }
                }
            }
        }
    }

    map
}

/// Resolve a linked cell's stored value (usually a row_index) to its display text in the target sheet,
/// and return the normalized comparable string.
fn resolve_linked_display_value(
    registry: &SheetRegistry,
    category: &Option<String>,
    target_sheet_name: &str,
    target_column_index: usize,
    stored_value: &str,
) -> Option<String> {
    // Parse stored value as row_index
    let row_index_value = stored_value.parse::<i64>().ok()?;
    let sheet = registry.get_sheet(category, target_sheet_name)?;
    // Find the row with matching row_index in column 0
    let row_opt = sheet.grid.iter().find(|r| {
        r.get(0)
            .and_then(|s| s.parse::<i64>().ok())
            .map(|idx| idx == row_index_value)
            .unwrap_or(false)
    })?;
    // Get display cell
    let display_val = row_opt.get(target_column_index)?.clone();
    Some(normalize_cell_value(&display_val))
}

/// Build a composite duplicate map using ALL included columns for the filtered parent context.
/// Key is the concatenation of normalized values for each included column (resolving Linked columns to display text).
pub(crate) fn build_composite_duplicate_map_for_parents(
    parent_prefix_values: &[String],
    included: &[usize],
    cat_ctx: &Option<String>,
    sheet_ctx: &Option<String>,
    registry: &SheetRegistry,
    state: &EditorWindowState,
) -> HashMap<String, usize> {
    let mut map: HashMap<String, usize> = HashMap::new();

    let Some(sheet_name) = sheet_ctx else {
        return map;
    };
    let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_name) else {
        return map;
    };
    let Some(meta) = &sheet_ref.metadata else {
        return map;
    };

    // Compute parent row indices to filter by full chain
    let parent_row_indices = convert_parent_names_to_row_indices(parent_prefix_values, state, registry);

    // Get parent_key and grand columns
    let parent_key_col = meta
        .columns
        .iter()
        .position(|c| c.header.eq_ignore_ascii_case("parent_key"))
        .or_else(|| if meta.is_structure_table() { Some(1) } else { None });
    let grand_cols: Vec<(usize, usize)> = meta
        .columns
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            if c.header.starts_with("grand_") && c.header.ends_with("_parent") {
                if let Some(n_str) = c
                    .header
                    .strip_prefix("grand_")
                    .and_then(|s| s.strip_suffix("_parent"))
                {
                    if let Ok(n) = n_str.parse::<usize>() {
                        return Some((i, n));
                    }
                }
            }
            None
        })
        .collect();

    // Pre-resolve linked info per included column
    let linked_targets: HashMap<usize, (String, usize)> = included
        .iter()
        .copied()
        .filter_map(|col| {
            meta.columns.get(col).and_then(|c| match &c.validator {
                Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                    Some((col, (target_sheet_name.clone(), *target_column_index)))
                }
                _ => None,
            })
        })
        .collect();

    // Iterate rows and filter by full parent chain; then build composite key
    'rows: for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
        if !parent_row_indices.is_empty() {
            // Check immediate parent_key
            if let (Some(pk_idx), Some(&expected_parent)) = (parent_key_col, parent_row_indices.last()) {
                let expected_parent_str = expected_parent.to_string();
                if row.get(pk_idx).map(|s| s.as_str()) != Some(expected_parent_str.as_str()) {
                    continue;
                }
            }
            // Check grand chain
            let clen = parent_row_indices.len();
            if clen > 1 {
                for (gcol, n) in &grand_cols {
                    if clen > *n {
                        let expected_idx = parent_row_indices[clen - 1 - *n];
                        let expected_str = expected_idx.to_string();
                        if row.get(*gcol).map(|s| s.as_str()) != Some(expected_str.as_str()) {
                            continue 'rows;
                        }
                    }
                }
            }
        }

        // Build composite key across included
        let mut parts: Vec<String> = Vec::with_capacity(included.len());
        for &col in included {
            if let Some(raw) = row.get(col) {
                if let Some((ref target_sheet, target_col)) = linked_targets.get(&col) {
                    if let Some(norm) = resolve_linked_display_value(
                        registry,
                        cat_ctx,
                        target_sheet,
                        *target_col,
                        raw,
                    ) {
                        parts.push(norm);
                        continue;
                    }
                }
                parts.push(normalize_cell_value(raw));
            } else {
                parts.push(String::new());
            }
        }
        let key = parts.join("||");
        if key.trim().is_empty() {
            continue;
        }
        map.entry(key).or_insert(row_idx);
    }

    map
}

/// Convert human-readable parent names to row_index values
/// Walks up the parent chain to find the row_index for each parent name
pub(crate) fn convert_parent_names_to_row_indices(
    parent_names: &[String],
    state: &EditorWindowState,
    registry: &SheetRegistry,
) -> Vec<usize> {
    let mut indices = Vec::new();

    info!(
        "convert_parent_names_to_row_indices: parent_names={:?}",
        parent_names
    );

    // Look up parent names in parent tables by walking the hierarchy
    // This is needed for physical child tables where we need to convert
    // human-readable names like "Battlefield 2142" to row_index values

    let (cat_ctx, sheet_ctx) = state.current_sheet_context();
    let Some(sheet_name) = sheet_ctx else {
        return indices;
    };

    let Some(sheet_ref) = registry.get_sheet(&cat_ctx, &sheet_name) else {
        return indices;
    };

    let Some(meta) = &sheet_ref.metadata else {
        return indices;
    };

    // Get parent table reference from metadata
    let parent_link = meta.structure_parent.as_ref();
    let Some(parent_info) = parent_link else {
        return indices;
    };

    // For each parent name, look it up in the parent table
    let mut current_parent_sheet = parent_info.parent_sheet.clone();
    let mut current_parent_category = parent_info.parent_category.clone();

    for parent_name in parent_names {
        let Some(parent_sheet_ref) = registry.get_sheet(&current_parent_category, &current_parent_sheet) else {
            break;
        };

        let Some(parent_meta) = &parent_sheet_ref.metadata else {
            break;
        };

        // Find the first data column index (skip technical columns)
        let first_data_col = parent_meta.columns.iter().position(|col| {
            let lower = col.header.to_lowercase();
            lower != "row_index"
                && lower != "parent_key"
                && !lower.starts_with("grand_")
                && lower != "id"
                && lower != "created_at"
                && lower != "updated_at"
        });

        let Some(data_col_idx) = first_data_col else {
            break;
        };

        // Find the row where the first data column matches the parent name
        let mut found_row_idx = None;
        for (row_idx, row) in parent_sheet_ref.grid.iter().enumerate() {
            if let Some(cell_value) = row.get(data_col_idx) {
                if cell_value == parent_name {
                    found_row_idx = Some(row_idx);
                    break;
                }
            }
        }

        let Some(row_idx) = found_row_idx else {
            // Parent name not found, can't continue chain
            warn!(
                "Parent name '{}' not found in table '{}', stopping chain lookup",
                parent_name, current_parent_sheet
            );
            break;
        };

        info!(
            "Found parent '{}' at row_index={} in table '{}'",
            parent_name, row_idx, current_parent_sheet
        );
        indices.push(row_idx);

        // Move up to next parent in chain if there is one
        if let Some(grandparent_link) = &parent_meta.structure_parent {
            current_parent_sheet = grandparent_link.parent_sheet.clone();
            current_parent_category = grandparent_link.parent_category.clone();
        } else {
            // No more parents in chain
            break;
        }
    }

    info!("Final parent_row_indices={:?}", indices);
    indices
}
