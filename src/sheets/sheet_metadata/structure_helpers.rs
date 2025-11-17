// src/sheets/sheet_metadata/structure_helpers.rs
use bevy::prelude::warn;
use std::collections::{HashMap, HashSet};

use crate::sheets::ai_schema::AiSchemaGroupStructureOverride;
use crate::sheets::column_definition::ColumnDefinition;
use crate::sheets::column_validator::ColumnValidator;
use crate::sheets::structure_field::StructureFieldDefinition;

pub fn collect_included_structure_paths(columns: &[ColumnDefinition]) -> Vec<Vec<usize>> {
    let mut paths: Vec<Vec<usize>> = Vec::new();
    for (column_index, column) in columns.iter().enumerate() {
        // Only process columns that are actually Structure validators
        if matches!(column.validator, Some(ColumnValidator::Structure)) {
            // Skip if no schema (not migrated from old JSON or child table doesn't exist)
            let Some(schema) = column.structure_schema.as_ref() else {
                continue;
            };
            let mut path = vec![column_index];
            if matches!(column.ai_include_in_send, Some(true)) {
                paths.push(path.clone());
            }
            collect_included_structure_paths_from_fields(schema, &mut path, &mut paths);
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn collect_included_structure_paths_from_fields(
    fields: &[StructureFieldDefinition],
    path: &mut Vec<usize>,
    output: &mut Vec<Vec<usize>>,
) {
    for (index, field) in fields.iter().enumerate() {
        path.push(index);
        // Only process Structure validators with schemas
        // Skip non-Structure fields even if they have leftover structure_schema data (legacy JSON)
        if matches!(field.validator, Some(ColumnValidator::Structure)) {
            if matches!(field.ai_include_in_send, Some(true)) {
                output.push(path.clone());
            }
            if let Some(schema) = field.structure_schema.as_ref() {
                collect_included_structure_paths_from_fields(schema, path, output);
            }
        }
        path.pop();
    }
}

pub fn collect_all_structure_paths(columns: &[ColumnDefinition]) -> HashSet<Vec<usize>> {
    let mut paths: HashSet<Vec<usize>> = HashSet::new();
    for (column_index, column) in columns.iter().enumerate() {
        // Only process columns that are actually Structure validators WITH schemas
        // Skip legacy Structure columns without schemas (not properly migrated from old JSON)
        if !matches!(column.validator, Some(ColumnValidator::Structure)) {
            continue;
        }
        let Some(schema) = column.structure_schema.as_ref() else {
            continue;  // Skip Structure columns without schemas
        };
        let mut path = vec![column_index];
        paths.insert(path.clone());
        collect_structure_paths_from_fields(schema, &mut path, &mut paths);
    }
    paths
}

fn collect_structure_paths_from_fields(
    fields: &[StructureFieldDefinition],
    path: &mut Vec<usize>,
    output: &mut HashSet<Vec<usize>>,
) {
    for (index, field) in fields.iter().enumerate() {
        path.push(index);
        output.insert(path.clone());
        if let Some(schema) = field.structure_schema.as_ref() {
            collect_structure_paths_from_fields(schema, path, output);
        }
        path.pop();
    }
}

pub fn apply_structure_send_inclusion(
    columns: &mut [ColumnDefinition],
    included_paths: &[Vec<usize>],
) -> bool {
    let included_set: HashSet<Vec<usize>> = included_paths.iter().cloned().collect();
    let mut changed = false;
    for (idx, column) in columns.iter_mut().enumerate() {
        let mut path = vec![idx];
        if apply_structure_send_flag_column(column, &mut path, &included_set) {
            changed = true;
        }
    }
    changed
}

fn apply_structure_send_flag_column(
    column: &mut ColumnDefinition,
    path: &mut Vec<usize>,
    included: &HashSet<Vec<usize>>,
) -> bool {
    let mut changed = false;
    if matches!(column.validator, Some(ColumnValidator::Structure)) {
        if included.contains(path) {
            if column.ai_include_in_send != Some(true) {
                column.ai_include_in_send = Some(true);
                changed = true;
            }
        } else if column.ai_include_in_send != Some(false) {
            column.ai_include_in_send = Some(false);
            changed = true;
        }
    }
    if let Some(schema) = column.structure_schema.as_mut() {
        for (idx, field) in schema.iter_mut().enumerate() {
            path.push(idx);
            if apply_structure_send_flag_field(field, path, included) {
                changed = true;
            }
            path.pop();
        }
    }
    changed
}

fn apply_structure_send_flag_field(
    field: &mut StructureFieldDefinition,
    path: &mut Vec<usize>,
    included: &HashSet<Vec<usize>>,
) -> bool {
    let mut changed = false;
    if matches!(field.validator, Some(ColumnValidator::Structure)) {
        if included.contains(path) {
            if field.ai_include_in_send != Some(true) {
                field.ai_include_in_send = Some(true);
                changed = true;
            }
        } else if field.ai_include_in_send != Some(false) {
            field.ai_include_in_send = Some(false);
            changed = true;
        }
    }
    if let Some(schema) = field.structure_schema.as_mut() {
        for (idx, child) in schema.iter_mut().enumerate() {
            path.push(idx);
            if apply_structure_send_flag_field(child, path, included) {
                changed = true;
            }
            path.pop();
        }
    }
    changed
}

pub fn describe_structure_path(columns: &[ColumnDefinition], path: &[usize]) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let mut names: Vec<String> = Vec::new();
    let column = columns.get(path[0])?;
    names.push(column.header.clone());
    if path.len() == 1 {
        return Some(names.join(" -> "));
    }
    let mut field = column.structure_schema.as_ref()?.get(path[1])?;
    names.push(field.header.clone());
    for idx in path.iter().skip(2) {
        field = field.structure_schema.as_ref()?.get(*idx)?;
        names.push(field.header.clone());
    }
    Some(names.join(" -> "))
}

pub fn structure_fields_for_path(
    columns: &[ColumnDefinition],
    path: &[usize],
) -> Option<Vec<StructureFieldDefinition>> {
    if path.is_empty() {
        return None;
    }
    let column = columns.get(path[0])?;
    if path.len() == 1 {
        return column.structure_schema.clone();
    }
    let mut field = column.structure_schema.as_ref()?.get(path[1])?;
    if path.len() == 2 {
        return field.structure_schema.clone();
    }
    for idx in path.iter().skip(2) {
        field = field.structure_schema.as_ref()?.get(*idx)?;
    }
    field.structure_schema.clone()
}

pub fn collect_structure_row_generation_overrides(
    columns: &[ColumnDefinition],
) -> Vec<AiSchemaGroupStructureOverride> {
    let mut overrides: Vec<AiSchemaGroupStructureOverride> = Vec::new();
    for (column_index, column) in columns.iter().enumerate() {
        collect_structure_row_overrides_from_column(column, column_index, &mut overrides);
    }
    overrides.sort_by(|a, b| a.path.cmp(&b.path));
    overrides
}

fn collect_structure_row_overrides_from_column(
    column: &ColumnDefinition,
    column_index: usize,
    output: &mut Vec<AiSchemaGroupStructureOverride>,
) {
    if let Some(value) = column.ai_enable_row_generation {
        output.push(AiSchemaGroupStructureOverride {
            path: vec![column_index],
            allow_add_rows: value,
        });
    }

    if let Some(schema) = column.structure_schema.as_ref() {
        for (field_index, field) in schema.iter().enumerate() {
            let mut path = vec![column_index, field_index];
            collect_structure_row_overrides_from_field(field, &mut path, output);
        }
    }
}

fn collect_structure_row_overrides_from_field(
    field: &StructureFieldDefinition,
    path: &mut Vec<usize>,
    output: &mut Vec<AiSchemaGroupStructureOverride>,
) {
    if let Some(value) = field.ai_enable_row_generation {
        output.push(AiSchemaGroupStructureOverride {
            path: path.clone(),
            allow_add_rows: value,
        });
    }

    if let Some(schema) = field.structure_schema.as_ref() {
        for (child_index, child_field) in schema.iter().enumerate() {
            path.push(child_index);
            collect_structure_row_overrides_from_field(child_field, path, output);
            path.pop();
        }
    }
}

pub fn apply_structure_row_generation_overrides(
    columns: &mut [ColumnDefinition],
    overrides: &[AiSchemaGroupStructureOverride],
) -> bool {
    let mut desired: HashMap<Vec<usize>, bool> = overrides
        .iter()
        .filter_map(|entry| {
            if structure_path_exists(columns, &entry.path) {
                Some((entry.path.clone(), entry.allow_add_rows))
            } else {
                warn!(
                    "Skipping AI schema group structure override with invalid path: {:?}",
                    entry.path
                );
                None
            }
        })
        .collect();

    let mut changed = false;

    for (column_index, column) in columns.iter_mut().enumerate() {
        let mut path = vec![column_index];
        if reconcile_column_structure_overrides(column, &mut path, &mut desired) {
            changed = true;
        }
    }

    for (path, value) in desired.drain() {
        if apply_structure_row_generation_override_to_columns(columns, &path, value) {
            changed = true;
        }
    }

    changed
}

fn reconcile_column_structure_overrides(
    column: &mut ColumnDefinition,
    path: &mut Vec<usize>,
    desired: &mut HashMap<Vec<usize>, bool>,
) -> bool {
    let mut changed = false;
    let key = path.clone();
    if let Some(&target) = desired.get(&key) {
        if column.ai_enable_row_generation != Some(target) {
            column.ai_enable_row_generation = Some(target);
            changed = true;
        }
        desired.remove(&key);
    } else if column.ai_enable_row_generation.is_some() {
        column.ai_enable_row_generation = None;
        changed = true;
    }

    if let Some(schema) = column.structure_schema.as_mut() {
        for (field_index, field) in schema.iter_mut().enumerate() {
            path.push(field_index);
            if reconcile_field_structure_overrides(field, path, desired) {
                changed = true;
            }
            path.pop();
        }
    }

    changed
}

fn reconcile_field_structure_overrides(
    field: &mut StructureFieldDefinition,
    path: &mut Vec<usize>,
    desired: &mut HashMap<Vec<usize>, bool>,
) -> bool {
    let mut changed = false;
    let key = path.clone();
    if let Some(&target) = desired.get(&key) {
        if field.ai_enable_row_generation != Some(target) {
            field.ai_enable_row_generation = Some(target);
            changed = true;
        }
        desired.remove(&key);
    } else if field.ai_enable_row_generation.is_some() {
        field.ai_enable_row_generation = None;
        changed = true;
    }

    if let Some(schema) = field.structure_schema.as_mut() {
        for (child_index, child_field) in schema.iter_mut().enumerate() {
            path.push(child_index);
            if reconcile_field_structure_overrides(child_field, path, desired) {
                changed = true;
            }
            path.pop();
        }
    }

    changed
}

pub fn structure_path_exists(columns: &[ColumnDefinition], path: &[usize]) -> bool {
    let (first, rest) = match path.split_first() {
        Some(split) => split,
        None => return false,
    };
    let Some(column) = columns.get(*first) else {
        return false;
    };

    if rest.is_empty() {
        matches!(column.validator, Some(ColumnValidator::Structure))
            || column.structure_schema.is_some()
    } else if let Some(schema) = column.structure_schema.as_ref() {
        structure_path_exists_in_fields(schema, rest)
    } else {
        false
    }
}

fn structure_path_exists_in_fields(fields: &[StructureFieldDefinition], path: &[usize]) -> bool {
    let (first, rest) = match path.split_first() {
        Some(split) => split,
        None => return true,
    };
    let Some(field) = fields.get(*first) else {
        return false;
    };

    if rest.is_empty() {
        true
    } else if let Some(schema) = field.structure_schema.as_ref() {
        structure_path_exists_in_fields(schema, rest)
    } else {
        false
    }
}

fn apply_structure_row_generation_override_to_columns(
    columns: &mut [ColumnDefinition],
    path: &[usize],
    allow: bool,
) -> bool {
    let (first, rest) = match path.split_first() {
        Some(split) => split,
        None => return false,
    };
    let Some(column) = columns.get_mut(*first) else {
        return false;
    };

    if rest.is_empty() {
        if column.ai_enable_row_generation != Some(allow) {
            column.ai_enable_row_generation = Some(allow);
            return true;
        }
        return false;
    }

    let Some(schema) = column.structure_schema.as_mut() else {
        return false;
    };
    apply_structure_row_generation_override_to_fields(schema, rest, allow)
}

fn apply_structure_row_generation_override_to_fields(
    fields: &mut [StructureFieldDefinition],
    path: &[usize],
    allow: bool,
) -> bool {
    let (first, rest) = match path.split_first() {
        Some(split) => split,
        None => return false,
    };
    let Some(field) = fields.get_mut(*first) else {
        return false;
    };

    if rest.is_empty() {
        if field.ai_enable_row_generation != Some(allow) {
            field.ai_enable_row_generation = Some(allow);
            return true;
        }
        return false;
    }

    let Some(schema) = field.structure_schema.as_mut() else {
        return false;
    };
    apply_structure_row_generation_override_to_fields(schema, rest, allow)
}
