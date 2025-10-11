// src/sheets/systems/ui_handlers/ai_include_handlers.rs
//! Handlers for AI include checkbox logic and cache management.

use crate::sheets::definitions::{ColumnValidator, SheetMetadata};
use crate::sheets::events::{
    RequestBatchUpdateColumnAiInclude, RequestUpdateAiSendSchema, RequestUpdateColumnAiInclude,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::log::warn;
use bevy::prelude::EventWriter;

use super::structure_handlers::derive_structure_context;

/// Update AI include cache for root context (non-structure columns).
/// Applies overrides and respects parent_key special handling.
pub fn update_ai_include_cache_root(
    state: &mut EditorWindowState,
    metadata: &SheetMetadata,
    headers: &[String],
    category: &Option<String>,
    sheet_name: &str,
    overrides: &[(usize, bool)],
) {
    let mut flags: Vec<bool> = Vec::with_capacity(metadata.columns.len());
    for (idx, column) in metadata.columns.iter().enumerate() {
        let is_parent_key = idx == 1
            && headers
                .get(1)
                .map(|h| h.eq_ignore_ascii_case("parent_key"))
                .unwrap_or(false);
        let mut include = if matches!(column.validator, Some(ColumnValidator::Structure)) {
            false
        } else {
            !matches!(column.ai_include_in_send, Some(false))
        };
        if let Some((_, override_value)) = overrides.iter().find(|(o_idx, _)| *o_idx == idx) {
            include = *override_value;
        }
        if is_parent_key {
            include = true;
        }
        flags.push(include);
    }

    state.ai_cached_included_columns = flags;
    state.ai_cached_included_columns_category = category.clone();
    state.ai_cached_included_columns_sheet = Some(sheet_name.to_string());
    state.ai_cached_included_columns_path.clear();
    state.ai_cached_included_columns_dirty = false;
    state.ai_cached_included_columns_valid = true;
}

/// Get all currently included column indices (excluding Structure validators).
/// Respects parent_key special handling and optionally applies a single column override.
pub fn get_included_columns(
    metadata: &SheetMetadata,
    headers: &[String],
    override_col: Option<(usize, bool)>,
) -> Vec<usize> {
    metadata
        .columns
        .iter()
        .enumerate()
        .filter_map(|(idx, col)| {
            if matches!(col.validator, Some(ColumnValidator::Structure)) {
                return None;
            }
            let is_parent_key = idx == 1
                && headers
                    .get(1)
                    .map(|h| h.eq_ignore_ascii_case("parent_key"))
                    .unwrap_or(false);
            let include = if is_parent_key {
                true
            } else if let Some((override_idx, override_val)) = override_col {
                if idx == override_idx {
                    override_val
                } else {
                    !matches!(col.ai_include_in_send, Some(false))
                }
            } else {
                !matches!(col.ai_include_in_send, Some(false))
            };
            if include {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

/// Handle AI include checkbox change in structure context.
/// Sends RequestUpdateAiSendSchema event and updates cache.
pub fn handle_ai_include_change_structure(
    state: &mut EditorWindowState,
    metadata: &SheetMetadata,
    headers: &[String],
    category: &Option<String>,
    sheet_name: &str,
    c_idx: usize,
    is_included: bool,
    send_schema_writer: &mut EventWriter<RequestUpdateAiSendSchema>,
) {
    let (root_category, root_sheet, structure_path) =
        derive_structure_context(state, category, sheet_name);

    if root_sheet.is_empty() {
        warn!(
            "Skipping AI send schema update: missing root sheet for {:?}/{}",
            root_category, sheet_name
        );
        return;
    }

    let included_indices = get_included_columns(metadata, headers, Some((c_idx, is_included)));

    let structure_path_opt = if structure_path.is_empty() {
        None
    } else {
        Some(structure_path.clone())
    };

    send_schema_writer.write(RequestUpdateAiSendSchema {
        category: root_category.clone(),
        sheet_name: root_sheet.clone(),
        structure_path: structure_path_opt,
        included_columns: included_indices.clone(),
    });

    // Update cache
    state.ai_cached_included_columns_category = category.clone();
    state.ai_cached_included_columns_sheet = Some(sheet_name.to_string());
    state.ai_cached_included_columns_path = structure_path;
    let mut flags = vec![false; metadata.columns.len()];
    for &idx in &included_indices {
        if let Some(slot) = flags.get_mut(idx) {
            *slot = true;
        }
    }
    state.ai_cached_included_columns = flags;
    state.ai_cached_included_columns_dirty = false;
    state.ai_cached_included_columns_valid = true;
}

/// Handle AI include checkbox change in root context (no structure).
/// Sends RequestUpdateColumnAiInclude event and updates cache.
pub fn handle_ai_include_change_root(
    state: &mut EditorWindowState,
    metadata: &SheetMetadata,
    headers: &[String],
    category: &Option<String>,
    sheet_name: &str,
    c_idx: usize,
    is_included: bool,
    column_include_writer: &mut EventWriter<RequestUpdateColumnAiInclude>,
) {
    column_include_writer.write(RequestUpdateColumnAiInclude {
        category: category.clone(),
        sheet_name: sheet_name.to_string(),
        column_index: c_idx,
        include: is_included,
    });
    update_ai_include_cache_root(
        state,
        metadata,
        headers,
        category,
        sheet_name,
        &[(c_idx, is_included)],
    );
}

/// Handle "Select all" for visible non-structure columns in structure context.
pub fn handle_select_all_structure(
    state: &mut EditorWindowState,
    metadata: &SheetMetadata,
    _headers: &[String],
    category: &Option<String>,
    sheet_name: &str,
    visible_columns: &[usize],
    send_schema_writer: &mut EventWriter<RequestUpdateAiSendSchema>,
) {
    let (root_category, root_sheet, structure_path) =
        derive_structure_context(state, category, sheet_name);

    let mut included_set: std::collections::HashSet<usize> = metadata
        .columns
        .iter()
        .enumerate()
        .filter_map(|(idx, col)| {
            if matches!(col.validator, Some(ColumnValidator::Structure)) {
                return None;
            }
            if matches!(col.ai_include_in_send, Some(false)) {
                None
            } else {
                Some(idx)
            }
        })
        .collect();

    // Add all visible non-structure columns
    for &idx in visible_columns {
        if let Some(col) = metadata.columns.get(idx) {
            if !matches!(col.validator, Some(ColumnValidator::Structure)) {
                included_set.insert(idx);
            }
        }
    }

    let mut included_indices: Vec<usize> = included_set.into_iter().collect();
    included_indices.sort_unstable();

    let structure_path_opt = if structure_path.is_empty() {
        None
    } else {
        Some(structure_path.clone())
    };

    send_schema_writer.write(RequestUpdateAiSendSchema {
        category: root_category.clone(),
        sheet_name: root_sheet.clone(),
        structure_path: structure_path_opt,
        included_columns: included_indices.clone(),
    });

    // Update cache
    state.ai_cached_included_columns_category = category.clone();
    state.ai_cached_included_columns_sheet = Some(sheet_name.to_string());
    state.ai_cached_included_columns_path = structure_path;
    let mut flags = vec![false; metadata.columns.len()];
    for &idx in &included_indices {
        if let Some(slot) = flags.get_mut(idx) {
            *slot = true;
        }
    }
    state.ai_cached_included_columns = flags;
    state.ai_cached_included_columns_dirty = false;
    state.ai_cached_included_columns_valid = true;
}

/// Handle "De-select all" for visible non-structure columns in structure context.
pub fn handle_deselect_all_structure(
    state: &mut EditorWindowState,
    metadata: &SheetMetadata,
    headers: &[String],
    category: &Option<String>,
    sheet_name: &str,
    visible_columns: &[usize],
    send_schema_writer: &mut EventWriter<RequestUpdateAiSendSchema>,
) {
    let (root_category, root_sheet, structure_path) =
        derive_structure_context(state, category, sheet_name);

    let mut included_indices: Vec<usize> = metadata
        .columns
        .iter()
        .enumerate()
        .filter_map(|(idx, col)| {
            if matches!(col.validator, Some(ColumnValidator::Structure)) {
                return None;
            }
            if visible_columns.contains(&idx) {
                return None;
            }
            if matches!(col.ai_include_in_send, Some(false)) {
                return None;
            }
            Some(idx)
        })
        .collect();

    // Ensure parent_key is always included
    if headers
        .get(1)
        .map(|h| h.eq_ignore_ascii_case("parent_key"))
        .unwrap_or(false)
    {
        if !included_indices.contains(&1) {
            included_indices.push(1);
        }
    }

    let structure_path_opt = if structure_path.is_empty() {
        None
    } else {
        Some(structure_path.clone())
    };

    send_schema_writer.write(RequestUpdateAiSendSchema {
        category: root_category.clone(),
        sheet_name: root_sheet.clone(),
        structure_path: structure_path_opt,
        included_columns: included_indices.clone(),
    });

    // Update cache
    state.ai_cached_included_columns_category = category.clone();
    state.ai_cached_included_columns_sheet = Some(sheet_name.to_string());
    state.ai_cached_included_columns_path = structure_path;
    let mut flags = vec![false; metadata.columns.len()];
    for &idx in &included_indices {
        if let Some(slot) = flags.get_mut(idx) {
            *slot = true;
        }
    }
    state.ai_cached_included_columns = flags;
    state.ai_cached_included_columns_dirty = false;
    state.ai_cached_included_columns_valid = true;
}

/// Handle "Select all" for visible non-structure columns in root context.
pub fn handle_select_all_root(
    state: &mut EditorWindowState,
    metadata: &SheetMetadata,
    headers: &[String],
    category: &Option<String>,
    sheet_name: &str,
    visible_columns: &[usize],
    batch_include_writer: &mut EventWriter<RequestBatchUpdateColumnAiInclude>,
) {
    let mut updates: Vec<(usize, bool)> = Vec::new();
    for &idx in visible_columns {
        if let Some(col) = metadata.columns.get(idx) {
            if matches!(col.validator, Some(ColumnValidator::Structure)) {
                continue;
            }
            let is_parent = idx == 1
                && headers
                    .get(1)
                    .map(|h| h.eq_ignore_ascii_case("parent_key"))
                    .unwrap_or(false);
            if is_parent {
                continue;
            }
            let currently_included = !matches!(col.ai_include_in_send, Some(false));
            if !currently_included {
                updates.push((idx, true));
            }
        }
    }
    if !updates.is_empty() {
        batch_include_writer.write(RequestBatchUpdateColumnAiInclude {
            category: category.clone(),
            sheet_name: sheet_name.to_string(),
            updates: updates.clone(),
        });
    }
    update_ai_include_cache_root(state, metadata, headers, category, sheet_name, &updates);
}

/// Handle "De-select all" for visible non-structure columns in root context.
pub fn handle_deselect_all_root(
    state: &mut EditorWindowState,
    metadata: &SheetMetadata,
    headers: &[String],
    category: &Option<String>,
    sheet_name: &str,
    visible_columns: &[usize],
    batch_include_writer: &mut EventWriter<RequestBatchUpdateColumnAiInclude>,
) {
    let mut updates: Vec<(usize, bool)> = Vec::new();
    for &idx in visible_columns {
        if let Some(col) = metadata.columns.get(idx) {
            if matches!(col.validator, Some(ColumnValidator::Structure)) {
                continue;
            }
            let is_parent = idx == 1
                && headers
                    .get(1)
                    .map(|h| h.eq_ignore_ascii_case("parent_key"))
                    .unwrap_or(false);
            if is_parent {
                continue;
            }
            let currently_included = !matches!(col.ai_include_in_send, Some(false));
            if currently_included {
                updates.push((idx, false));
            }
        }
    }
    if !updates.is_empty() {
        batch_include_writer.write(RequestBatchUpdateColumnAiInclude {
            category: category.clone(),
            sheet_name: sheet_name.to_string(),
            updates: updates.clone(),
        });
    }
    update_ai_include_cache_root(state, metadata, headers, category, sheet_name, &updates);
}
