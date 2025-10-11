// src/sheets/sheet_metadata/ai_schema_helpers.rs
use std::collections::HashSet;

use crate::sheets::ai_schema::{AiSchemaGroup, AiSchemaGroupStructureOverride};
use crate::sheets::column_validator::ColumnValidator;

use super::structure_helpers;
use super::SheetMetadata;

pub fn ensure_ai_schema_groups_initialized(meta: &mut SheetMetadata) {
    let column_count = meta.columns.len();
    let valid_structure_paths = structure_helpers::collect_all_structure_paths(&meta.columns);

    for group in meta.ai_schema_groups.iter_mut() {
        group.included_columns.retain(|idx| {
            *idx < column_count
                && !matches!(
                    meta.columns[*idx].validator,
                    Some(ColumnValidator::Structure)
                )
        });
        group.included_columns.sort_unstable();
        group.included_columns.dedup();

        group
            .structure_row_generation_overrides
            .retain(|override_entry| valid_structure_paths.contains(&override_entry.path));
        group
            .structure_row_generation_overrides
            .sort_by(|a, b| a.path.cmp(&b.path));

        group
            .included_structures
            .retain(|path| valid_structure_paths.contains(path));
        group.included_structures.sort();
        group.included_structures.dedup();
    }

    if meta.ai_schema_groups.is_empty() {
        let default_name = "Default".to_string();
        meta.ai_schema_groups.push(AiSchemaGroup {
            name: default_name.clone(),
            included_columns: meta.ai_included_column_indices(),
            allow_add_rows: meta.ai_enable_row_generation,
            structure_row_generation_overrides: meta
                .collect_structure_row_generation_overrides(),
            included_structures: meta.ai_included_structure_paths(),
        });
        meta.ai_active_schema_group = Some(default_name);
        return;
    }

    if let Some(active_name) = meta.ai_active_schema_group.clone() {
        if !meta
            .ai_schema_groups
            .iter()
            .any(|group| group.name == active_name)
        {
            meta.ai_active_schema_group = None;
        }
    }

    if meta.ai_active_schema_group.is_none() {
        if let Some(first) = meta.ai_schema_groups.first() {
            meta.ai_active_schema_group = Some(first.name.clone());
        }
    }
}

pub fn set_active_ai_schema_group_included_columns(
    meta: &mut SheetMetadata,
    included: &[usize],
) -> bool {
    let filtered: Vec<usize> = included
        .iter()
        .copied()
        .filter(|idx| {
            *idx < meta.columns.len()
                && !matches!(
                    meta.columns[*idx].validator,
                    Some(ColumnValidator::Structure)
                )
        })
        .collect();

    if let Some(active_name) = meta.ai_active_schema_group.clone() {
        if let Some(group) = meta
            .ai_schema_groups
            .iter_mut()
            .find(|g| g.name == active_name)
        {
            if group.included_columns != filtered {
                group.included_columns = filtered;
                return true;
            }
        }
    }
    false
}

pub fn set_active_ai_schema_group_included_structures(
    meta: &mut SheetMetadata,
    included_paths: &[Vec<usize>],
) -> bool {
    let valid_paths = structure_helpers::collect_all_structure_paths(&meta.columns);
    let mut filtered: Vec<Vec<usize>> = included_paths
        .iter()
        .filter(|path| valid_paths.contains(*path))
        .cloned()
        .collect();
    filtered.sort();
    filtered.dedup();

    if let Some(active_name) = meta.ai_active_schema_group.clone() {
        if let Some(group) = meta
            .ai_schema_groups
            .iter_mut()
            .find(|g| g.name == active_name)
        {
            if group.included_structures != filtered {
                group.included_structures = filtered;
                return true;
            }
        }
    }
    false
}

pub fn set_active_ai_schema_group_allow_rows(meta: &mut SheetMetadata, allow: bool) -> bool {
    if let Some(active_name) = meta.ai_active_schema_group.clone() {
        if let Some(group) = meta
            .ai_schema_groups
            .iter_mut()
            .find(|g| g.name == active_name)
        {
            if group.allow_add_rows != allow {
                group.allow_add_rows = allow;
                return true;
            }
        }
    }
    false
}

pub fn set_active_ai_schema_group_structure_override(
    meta: &mut SheetMetadata,
    path: &[usize],
    override_value: Option<bool>,
) -> bool {
    if path.is_empty() || !meta.structure_path_exists(path) {
        return false;
    }

    let Some(active_name) = meta.ai_active_schema_group.clone() else {
        return false;
    };

    let Some(group) = meta
        .ai_schema_groups
        .iter_mut()
        .find(|g| g.name == active_name)
    else {
        return false;
    };

    if let Some(value) = override_value {
        if let Some(entry) = group
            .structure_row_generation_overrides
            .iter_mut()
            .find(|entry| entry.path == path)
        {
            if entry.allow_add_rows != value {
                entry.allow_add_rows = value;
                return true;
            }
            return false;
        }

        group
            .structure_row_generation_overrides
            .push(AiSchemaGroupStructureOverride {
                path: path.to_vec(),
                allow_add_rows: value,
            });
        group
            .structure_row_generation_overrides
            .sort_by(|a, b| a.path.cmp(&b.path));
        true
    } else {
        let original_len = group.structure_row_generation_overrides.len();
        group
            .structure_row_generation_overrides
            .retain(|entry| entry.path != path);
        original_len != group.structure_row_generation_overrides.len()
    }
}

pub fn apply_ai_schema_group(meta: &mut SheetMetadata, group_name: &str) -> Result<bool, String> {
    let group = meta
        .ai_schema_groups
        .iter()
        .find(|g| g.name == group_name)
        .cloned()
        .ok_or_else(|| format!("AI schema group '{}' not found", group_name))?;

    let included_set: HashSet<usize> = group.included_columns.iter().copied().collect();
    let mut changed = false;

    for (idx, column) in meta.columns.iter_mut().enumerate() {
        if matches!(column.validator, Some(ColumnValidator::Structure)) {
            continue;
        }
        let should_include = included_set.contains(&idx);
        if should_include {
            if column.ai_include_in_send.is_some() {
                column.ai_include_in_send = None;
                changed = true;
            }
        } else if column.ai_include_in_send != Some(false) {
            column.ai_include_in_send = Some(false);
            changed = true;
        }
    }

    if meta.ai_enable_row_generation != group.allow_add_rows {
        meta.ai_enable_row_generation = group.allow_add_rows;
        changed = true;
    }

    if meta.apply_structure_row_generation_overrides(&group.structure_row_generation_overrides) {
        changed = true;
    }

    if meta.apply_structure_send_inclusion(&group.included_structures) {
        changed = true;
    }

    if meta.ai_active_schema_group.as_deref() != Some(group_name) {
        meta.ai_active_schema_group = Some(group_name.to_string());
        changed = true;
    }

    Ok(changed)
}

pub fn ensure_unique_schema_group_name(meta: &SheetMetadata, desired: &str) -> String {
    if !meta
        .ai_schema_groups
        .iter()
        .any(|g| g.name.eq_ignore_ascii_case(desired))
    {
        return desired.to_string();
    }

    let mut counter = 2usize;
    let base = desired.trim();
    loop {
        let candidate = format!("{} {}", base, counter);
        if !meta
            .ai_schema_groups
            .iter()
            .any(|g| g.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        counter += 1;
    }
}
