use bevy::prelude::*;
use crate::sheets::{
    resources::SheetRegistry,
    events::SheetDataModifiedInRegistryEvent,
    definitions::{ColumnValidator, SheetMetadata, StructureFieldDefinition},
};
use serde_json::{Value};

// NEW: Resource to queue cascade events emitted after parent sync to avoid borrow conflict
#[derive(Resource, Default, Debug)]
pub struct PendingStructureCascade(pub Vec<(Option<String>, String)>);

/// Listens for SheetDataModifiedInRegistryEvent and if the modified sheet is a virtual
/// structure sheet (structure_parent set) it synchronizes:
/// 1. Parent ColumnDefinition.structure_schema with the virtual sheet's columns
/// 2. Rewrites every row's JSON object in the parent sheet for that structure column
///    so all structure cells share the same ordered keys & updated metadata (keys = headers)
pub fn handle_sync_virtual_structure_sheet(
    mut events: EventReader<SheetDataModifiedInRegistryEvent>,
    mut registry: ResMut<SheetRegistry>,
    mut pending: ResMut<PendingStructureCascade>,
) {
    let mut parents_to_update: Vec<(Option<String>, String, usize, SheetMetadata)> = Vec::new();

    for ev in events.read() {
        // Get modified sheet
        let Some(sheet_data) = registry.get_sheet(&ev.category, &ev.sheet_name) else { continue; };
        let Some(meta) = &sheet_data.metadata else { continue; };
        let Some(parent_link) = &meta.structure_parent else { continue; };

        // Clone virtual columns as structure schema fields
    let _structure_fields: Vec<StructureFieldDefinition> = meta.columns.iter().map(|c| StructureFieldDefinition {
            header: c.header.clone(),
            validator: c.validator.clone(),
            data_type: c.data_type,
            filter: c.filter.clone(),
            ai_context: c.ai_context.clone(),
            width: c.width,
            structure_schema: c.structure_schema.clone(),
        }).collect();

        // Fetch parent sheet mutably later; store intent now to avoid double borrow
        parents_to_update.push((parent_link.parent_category.clone(), parent_link.parent_sheet.clone(), parent_link.parent_column_index, SheetMetadata { ..meta.clone() }));

    // Apply schema to parent AFTER loop
    }

    if parents_to_update.is_empty() { return; }

    for (parent_cat, parent_sheet_name, parent_col_index, virtual_meta_clone) in parents_to_update {
        // Get mutable parent sheet
        let Some(parent_sheet) = registry.get_sheet_mut(&parent_cat, &parent_sheet_name) else { continue; };
        let Some(parent_metadata) = &mut parent_sheet.metadata else { continue; };
        if parent_col_index >= parent_metadata.columns.len() { continue; }
        let parent_col = &mut parent_metadata.columns[parent_col_index];
        if !matches!(parent_col.validator, Some(ColumnValidator::Structure)) { continue; }

        // Prepare key and capture old schema (if any) before rebuilding to enable value reordering
    let old_headers: Vec<String> = parent_col.structure_schema.as_ref().map(|v| v.iter().map(|c| c.header.clone()).collect()).unwrap_or_default();

        // Build new structure schema (ordered as virtual sheet currently presents)
        let new_schema: Vec<StructureFieldDefinition> = virtual_meta_clone.columns.iter().map(|c| StructureFieldDefinition {
            header: c.header.clone(),
            validator: c.validator.clone(),
            data_type: c.data_type,
            filter: c.filter.clone(),
            ai_context: c.ai_context.clone(),
            width: c.width,
            structure_schema: c.structure_schema.clone(),
        }).collect();
    // Maintain inline mirror for user visibility / persistence preferences
    parent_col.structure_schema = Some(new_schema.clone());
    // Preserve existing key/ancestor selections already stored inline
    if parent_col.structure_column_order.is_none() { parent_col.structure_column_order = Some((0..new_schema.len()).collect()); } else { parent_col.structure_column_order = Some((0..new_schema.len()).collect()); }

        // Collect ordered headers for rewriting cell JSON
    let ordered_headers: Vec<String> = new_schema.iter().map(|f| f.header.clone()).collect();
    // Mapping: header -> old index
    use std::collections::{HashMap, HashSet};
    let mut old_index_by_header: HashMap<&str, usize> = old_headers.iter().enumerate().map(|(i,h)| (h.as_str(), i)).collect();
    // --- Rename Preservation Logic ---
    // If exactly one header changed (pure rename, no add/remove), map the new name to the old index
    if old_headers.len() == ordered_headers.len() {
        let old_set: HashSet<&str> = old_headers.iter().map(|s| s.as_str()).collect();
        let new_set: HashSet<&str> = ordered_headers.iter().map(|s| s.as_str()).collect();
        if old_set != new_set {
            let removed: Vec<&str> = old_set.difference(&new_set).copied().collect();
            let added: Vec<&str> = new_set.difference(&old_set).copied().collect();
            if removed.len() == 1 && added.len() == 1 {
                if let Some(old_idx) = old_headers.iter().position(|h| h == removed[0]) {
                    // Map the newly added (renamed) header to the old index so value is preserved
                    old_index_by_header.insert(added[0], old_idx);
                }
            }
        }
    }

        // Rewrite each row's JSON cell in parent grid at parent_col_index
        for row in parent_sheet.grid.iter_mut() {
            if row.len() <= parent_col_index { row.resize(parent_col_index + 1, String::new()); }
            let cell = &mut row[parent_col_index];
            let trimmed = cell.trim();
            let mut rows_vec: Vec<Vec<String>> = Vec::new();
            if trimmed.is_empty() { rows_vec.push(vec![String::new(); ordered_headers.len()]); }
            else if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                match val {
                    Value::Object(map) => { // legacy single object
                        let mut row_vals = Vec::with_capacity(ordered_headers.len());
                        for h in &ordered_headers { row_vals.push(map.get(h).and_then(|v| v.as_str()).unwrap_or("").to_string()); }
                        rows_vec.push(row_vals);
                    }
                    Value::Array(arr) => {
                        if arr.iter().all(|v| v.is_object()) { // legacy multi-row objects
                            for obj in arr.into_iter() { if let Value::Object(m) = obj { let mut row_vals = Vec::with_capacity(ordered_headers.len()); for h in &ordered_headers { row_vals.push(m.get(h).and_then(|v| v.as_str()).unwrap_or("").to_string()); } rows_vec.push(row_vals); } }
                        } else if arr.iter().all(|v| v.is_array()) { // already new format multi-row (positional arrays)
                            // Reorder each inner row based on header mapping if old headers available
                            for inner in arr.into_iter() {
                                if let Value::Array(inner_vals)=inner {
                                    let old_vals: Vec<String> = inner_vals.into_iter().map(|v| v.as_str().unwrap_or("").to_string()).collect();
                                    let mut reordered: Vec<String> = Vec::with_capacity(ordered_headers.len());
                                    for (new_pos, h) in ordered_headers.iter().enumerate() {
                                        if let Some(old_i) = old_index_by_header.get(h.as_str()) { reordered.push(old_vals.get(*old_i).cloned().unwrap_or_default()); }
                                        else if old_headers.len() == ordered_headers.len() { // positional fallback for unexpected mismatch
                                            reordered.push(old_vals.get(new_pos).cloned().unwrap_or_default());
                                        } else { reordered.push(String::new()); }
                                    }
                                    rows_vec.push(reordered);
                                }
                            }
                        } else if arr.iter().all(|v| v.is_string()) { // single row new format (positional array)
                            let old_vals: Vec<String> = arr.into_iter().map(|v| v.as_str().unwrap_or("").to_string()).collect();
                            let mut reordered: Vec<String> = Vec::with_capacity(ordered_headers.len());
                            for (new_pos, h) in ordered_headers.iter().enumerate() {
                                if let Some(old_i) = old_index_by_header.get(h.as_str()) { reordered.push(old_vals.get(*old_i).cloned().unwrap_or_default()); }
                                else if old_headers.len() == ordered_headers.len() { reordered.push(old_vals.get(new_pos).cloned().unwrap_or_default()); }
                                else { reordered.push(String::new()); }
                            }
                            rows_vec.push(reordered);
                        } else { rows_vec.push(vec![String::new(); ordered_headers.len()]); }
                    }
                    _ => rows_vec.push(vec![String::new(); ordered_headers.len()])
                }
            } else { rows_vec.push(vec![String::new(); ordered_headers.len()]); }
            // Normalize to new storage: single row => [] of strings; multi-row => [[]]
            if rows_vec.len() == 1 { *cell = Value::Array(rows_vec.remove(0).into_iter().map(Value::String).collect()).to_string(); }
            else { let outer: Vec<Value> = rows_vec.into_iter().map(|r| Value::Array(r.into_iter().map(Value::String).collect())).collect(); *cell = Value::Array(outer).to_string(); }
        }
        let parent_changed = parent_metadata.ensure_column_consistency();
        // Cascade event upward if parent itself is virtual (nested) and we changed it
        if parent_changed || !new_schema.is_empty() {
            if let Some(pmeta) = &parent_sheet.metadata { if pmeta.structure_parent.is_some() { pending.0.push((parent_cat.clone(), parent_sheet_name.clone())); } }
        }
    }
}

/// Emit queued cascade events collected by handle_sync_virtual_structure_sheet
pub fn handle_emit_structure_cascade_events(
    mut pending: ResMut<PendingStructureCascade>,
    mut writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    if pending.0.is_empty() { return; }
    // Deduplicate to avoid spamming
    pending.0.sort_by(|a,b| a.cmp(b));
    pending.0.dedup();
    for (cat, name) in pending.0.drain(..) {
        writer.write(SheetDataModifiedInRegistryEvent { category: cat, sheet_name: name });
    }
}