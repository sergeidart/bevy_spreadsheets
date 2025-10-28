// src/ui/elements/editor/structure_navigation.rs
use crate::sheets::definitions::{
    ColumnDataType, ColumnDefinition, ColumnValidator, SheetGridData, SheetMetadata,
};
use crate::sheets::{
    events::{CloseStructureViewEvent, OpenStructureViewEvent, SheetDataModifiedInRegistryEvent},
    resources::{SheetRegistry, SheetRenderCache},
};
use crate::ui::elements::editor::state::{
    EditorWindowState, StructureParentContext, VirtualStructureContext,
};
use bevy::prelude::*;

/// Collects ancestor keys from a parent row for structure navigation.
/// 
/// **Post-Refactor (2025-10-28):**
/// Now uses lineage walking instead of reading grand_N_parent columns.
/// Walks up the parent chain using parent_key references to build full lineage.
/// 
/// Returns (ancestor_keys, display_value) where:
/// - ancestor_keys: Display values from all ancestors (root to immediate parent)
/// - display_value: The display value from the current parent row
///
/// # Arguments
/// * `target_structure_name` - The name of the child structure table we're navigating to
pub fn collect_structure_ancestors(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    target_structure_name: &str,
    row_index: usize,
) -> (Vec<String>, String) {
    use crate::sheets::systems::logic::lineage_helpers;
    
    // Get the current row's display value
    let mut display_value = String::new();
    let mut parent_row_parent_key: Option<usize> = None;
    
    if let Some(parent_sheet) = registry.get_sheet(category, sheet_name) {
        if let Some(parent_meta) = &parent_sheet.metadata {
            if let Some(row) = parent_sheet.grid.get(row_index) {
                // Get display value from first content column
                display_value = get_first_content_column_value(parent_meta, row);
                
                // Get this row's parent_key (to walk its ancestors, not including itself)
                if let Some(pk_col) = parent_meta.columns.iter().position(|c| c.header.eq_ignore_ascii_case("parent_key")) {
                    if let Some(pk_str) = row.get(pk_col) {
                        if !pk_str.is_empty() {
                            parent_row_parent_key = pk_str.parse::<usize>().ok();
                        }
                    }
                }
            }
        }
    }
    
    // Walk up the lineage from the parent's parent (to get ancestors, not including current parent)
    let mut ancestor_keys: Vec<String> = if let Some(parent_pk) = parent_row_parent_key {
        // This row has a parent, walk from there
        if let Some(parent_link) = registry.get_sheet(category, sheet_name)
            .and_then(|sd| sd.metadata.as_ref())
            .and_then(|meta| meta.structure_parent.as_ref())
        {
            let lineage = lineage_helpers::walk_parent_lineage(
                registry,
                &parent_link.parent_category,
                &parent_link.parent_sheet,
                parent_pk
            );
            
            lineage.iter()
                .map(|(_, display_val, _)| display_val.clone())
                .collect()
        } else {
            Vec::new()
        }
    } else {
        // This row is at root level (no parent), so no ancestors
        Vec::new()
    };
    
    // Add the current parent's display value to the ancestor keys
    // This way, ancestor_keys contains the full lineage including the immediate parent
    if !display_value.is_empty() {
        ancestor_keys.push(display_value.clone());
    }
    
    bevy::log::info!(
        "Structure ancestors collected (lineage walk): {} -> {} | ancestors={:?}, display='{}'",
        sheet_name,
        target_structure_name,
        ancestor_keys,
        display_value
    );
    
    (ancestor_keys, display_value)
}

/// Get display value from first content column (skipping technical columns)
pub fn get_first_content_column_value(metadata: &SheetMetadata, row: &[String]) -> String {
    // Find first non-technical column
    for (idx, col) in metadata.columns.iter().enumerate() {
        if col.deleted || col.hidden {
            continue;
        }
        
        let header_lower = col.header.to_lowercase();
        if header_lower == "row_index" 
            || header_lower == "id" 
            || header_lower == "parent_key"
        {
            continue;
        }
        
        // This is a content column
        return row.get(idx).cloned().unwrap_or_default();
    }
    
    // Fallback: any non-empty value
    row.iter()
        .find(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| "?".to_string())
}

// Parse JSON object string into headers + single row
fn parse_structure_cell(json_str: &str) -> (Vec<String>, Vec<String>) {
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
    if let serde_json::Value::Object(map) = parsed {
        let mut headers: Vec<String> = map.keys().cloned().collect();
        headers.sort();
        let mut row: Vec<String> = Vec::with_capacity(headers.len());
        for h in &headers {
            let cell_str = map
                .get(h)
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    _ => v.to_string(),
                })
                .unwrap_or_default();
            row.push(cell_str);
        }
        (headers, row)
    } else {
        (Vec::new(), Vec::new())
    }
}

// Build a virtual sheet name based on parent context & nesting depth
fn make_virtual_sheet_name(parent: &StructureParentContext, depth: usize) -> String {
    format!(
        "__virtual__{}__r{}c{}__lvl{}",
        parent.parent_sheet, parent.parent_row, parent.parent_col, depth
    )
}

pub fn handle_open_structure_view(
    mut events: EventReader<OpenStructureViewEvent>,
    mut state: ResMut<EditorWindowState>,
    mut registry: ResMut<SheetRegistry>,
    mut render_cache: ResMut<SheetRenderCache>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    for ev in events.read() {
        // Determine base parent sheet (could itself be virtual if nested)
        if let Some(sheet) = registry.get_sheet(&ev.parent_category, &ev.parent_sheet) {
            if let Some(meta) = &sheet.metadata {
                if let Some(row) = sheet.grid.get(ev.row_index) {
                    if let Some(cell) = row.get(ev.col_index) {
                        let parent_col = meta.columns.get(ev.col_index);
                        let (headers, all_rows, schema_defs) = if let Some(parent_col_def) =
                            parent_col
                        {
                            if matches!(parent_col_def.validator, Some(ColumnValidator::Structure))
                            {
                                if let Some(schema) = &parent_col_def.structure_schema {
                                    let parsed: serde_json::Value = serde_json::from_str(cell)
                                        .unwrap_or(serde_json::Value::Null);
                                    let headers: Vec<String> =
                                        schema.iter().map(|f| f.header.clone()).collect();
                                    let mut rows: Vec<Vec<String>> = Vec::new();
                                    match parsed {
                                        serde_json::Value::Array(arr) => {
                                            if arr.iter().all(|v| v.is_object()) {
                                                // Legacy object form (array of objects)
                                                for obj_val in arr.iter() {
                                                    let obj = obj_val.as_object();
                                                    let mut row_vals: Vec<String> =
                                                        Vec::with_capacity(headers.len());
                                                    for h in headers.iter() {
                                                        let val = obj
                                                            .and_then(|m| m.get(h))
                                                            .cloned()
                                                            .unwrap_or(serde_json::Value::String(
                                                                String::new(),
                                                            ));
                                                        row_vals.push(match val {
                                                            serde_json::Value::String(s) => s,
                                                            other => other.to_string(),
                                                        });
                                                    }
                                                    rows.push(row_vals);
                                                }
                                            } else if arr.iter().all(|v| v.is_array()) {
                                                // New multi-row positional format: [[..],[..]]
                                                for inner in arr.iter() {
                                                    if let serde_json::Value::Array(inner_vals) =
                                                        inner
                                                    {
                                                        let mut row_vals: Vec<String> =
                                                            Vec::with_capacity(headers.len());
                                                        for (i, _h) in headers.iter().enumerate() {
                                                            let val = inner_vals
                                                                .get(i)
                                                                .cloned()
                                                                .unwrap_or(
                                                                    serde_json::Value::String(
                                                                        String::new(),
                                                                    ),
                                                                );
                                                            row_vals.push(match val {
                                                                serde_json::Value::String(s) => s,
                                                                other => other.to_string(),
                                                            });
                                                        }
                                                        rows.push(row_vals);
                                                    }
                                                }
                                            } else if arr.iter().all(|v| v.is_string()) {
                                                // New single-row positional format: [..]
                                                let mut row_vals: Vec<String> =
                                                    Vec::with_capacity(headers.len());
                                                for (i, _h) in headers.iter().enumerate() {
                                                    let val = arr.get(i).cloned().unwrap_or(
                                                        serde_json::Value::String(String::new()),
                                                    );
                                                    row_vals.push(match val {
                                                        serde_json::Value::String(s) => s,
                                                        other => other.to_string(),
                                                    });
                                                }
                                                rows.push(row_vals);
                                            } else {
                                                // Fallback: blank single row
                                                rows.push(vec![String::new(); headers.len()]);
                                            }
                                        }
                                        serde_json::Value::Object(map) => {
                                            // Legacy single object
                                            let mut row_vals: Vec<String> =
                                                Vec::with_capacity(headers.len());
                                            for h in headers.iter() {
                                                let val = map.get(h).cloned().unwrap_or(
                                                    serde_json::Value::String(String::new()),
                                                );
                                                row_vals.push(match val {
                                                    serde_json::Value::String(s) => s,
                                                    other => other.to_string(),
                                                });
                                            }
                                            rows.push(row_vals);
                                        }
                                        _ => {}
                                    }
                                    if rows.is_empty() {
                                        rows.push(vec![String::new(); headers.len()]);
                                    }
                                    (headers, rows, Some(schema.clone()))
                                } else {
                                    let (h, r) = parse_structure_cell(cell);
                                    (h, vec![r], None)
                                }
                            } else {
                                let (h, r) = parse_structure_cell(cell);
                                (h, vec![r], None)
                            }
                        } else {
                            let (h, r) = parse_structure_cell(cell);
                            (h, vec![r], None)
                        };

                        let parent_ctx = StructureParentContext {
                            parent_category: ev.parent_category.clone(),
                            parent_sheet: ev.parent_sheet.clone(),
                            parent_row: ev.row_index,
                            parent_col: ev.col_index,
                        };

                        // Build virtual sheet metadata
                        let depth = state.virtual_structure_stack.len();
                        let virtual_name = make_virtual_sheet_name(&parent_ctx, depth);
                        // Create columns from headers, copying ai_context from parent sheet columns when header matches
                        let columns: Vec<ColumnDefinition> = headers
                            .iter()
                            .enumerate()
                            .map(|(i, h)| {
                                // If schema defs exist, use them
                                if let Some(schema) = &schema_defs {
                                    if let Some(def) = schema.get(i) {
                                        return ColumnDefinition {
                                            header: def.header.clone(),
                                            display_header: None,
                                            data_type: def.data_type,
                                            validator: def.validator.clone(),
                                            filter: def.filter.clone(),
                                            width: None,
                                            ai_context: def.ai_context.clone(),
                                            ai_enable_row_generation: def.ai_enable_row_generation,
                                            ai_include_in_send: def.ai_include_in_send,
                                            deleted: false,
                                            hidden: false, // User-defined schema column
                                            // Preserve deeper-level nested schemas & key metadata so that deeper levels persist and render consistently
                                            structure_schema: def.structure_schema.clone(),
                                            structure_column_order: def
                                                .structure_column_order
                                                .clone(),
                                            structure_key_parent_column_index: def
                                                .structure_key_parent_column_index,
                                            structure_ancestor_key_parent_column_indices: def
                                                .structure_ancestor_key_parent_column_indices
                                                .clone(),
                                        };
                                    }
                                }
                                let ai_ctx = meta
                                    .columns
                                    .iter()
                                    .find(|c| c.header == *h)
                                    .and_then(|c| c.ai_context.clone());
                                ColumnDefinition {
                                    header: h.clone(),
                                    display_header: None,
                                    data_type: ColumnDataType::String,
                                    validator: None,
                                    filter: None,
                                    width: None,
                                    ai_context: ai_ctx,
                                    ai_enable_row_generation: None,
                                    ai_include_in_send: None,
                                    deleted: false,
                                    hidden: false, // User-defined data column
                                    structure_schema: None,
                                    structure_column_order: None,
                                    structure_key_parent_column_index: None,
                                    structure_ancestor_key_parent_column_indices: None,
                                }
                            })
                            .collect();
                        let mut metadata = SheetMetadata::create_generic(
                            virtual_name.clone(),
                            format!("{}.json", virtual_name),
                            columns.len(),
                            ev.parent_category.clone(),
                        );
                        metadata.structure_parent =
                            Some(crate::sheets::definitions::StructureParentLink {
                                parent_category: ev.parent_category.clone(),
                                parent_sheet: ev.parent_sheet.clone(),
                                parent_column_index: ev.col_index,
                            });
                        // Overwrite generated columns with detailed ones
                        metadata.columns = columns;
                        // Insert grid
                        let mut grid_data = SheetGridData::default();
                        grid_data.metadata = Some(metadata);
                        for rv in all_rows.into_iter() {
                            if !rv.is_empty() {
                                grid_data.grid.push(rv);
                            }
                        }
                        if grid_data.grid.is_empty() {
                            grid_data.grid.push(vec![String::new(); headers.len()]);
                        }
                        // Register or replace (always replace to refresh)
                        registry.add_or_replace_sheet(
                            ev.parent_category.clone(),
                            virtual_name.clone(),
                            grid_data,
                        );
                        // Clear any cached filtered indices for this virtual sheet (avoid stale row index cache leading to Row Idx Err)
                        state.filtered_row_indices_cache.retain(|(cat, name), _| {
                            !(cat == &ev.parent_category && name == &virtual_name)
                        });
                        // Push context
                        state.virtual_structure_stack.push(VirtualStructureContext {
                            virtual_sheet_name: virtual_name.clone(),
                            parent: parent_ctx,
                        });
                        // Do NOT change selected_sheet_name; we keep parent selected and render virtual via stack
                        // Trigger render cache rebuild
                        data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                            category: ev.parent_category.clone(),
                            sheet_name: virtual_name.clone(),
                        });
                        // Ensure render cache entry exists (will be filled by system)
                        let _ = render_cache.ensure_and_get_sheet_cache_mut(
                            &ev.parent_category,
                            &virtual_name,
                            1,
                            headers.len(),
                        );
                    }
                }
            }
        }
    }
}

pub fn handle_close_structure_view(
    mut events: EventReader<CloseStructureViewEvent>,
    mut state: ResMut<EditorWindowState>,
    mut registry: ResMut<SheetRegistry>,
    mut render_cache: ResMut<SheetRenderCache>,
) {
    if events.is_empty() {
        return;
    }
    events.clear();

    // Determine if user explicitly deselected sheet (selected_sheet_name is None). If so, pop all.
    let pop_all = state.selected_sheet_name.is_none();

    if pop_all {
        while let Some(popped) = state.virtual_structure_stack.pop() {
            if let Ok(_removed) =
                registry.delete_sheet(&state.selected_category, &popped.virtual_sheet_name)
            {
                render_cache
                    .clear_sheet_render_data(&state.selected_category, &popped.virtual_sheet_name);
            }
        }
    } else {
        // Single-step pop
        if let Some(popped) = state.virtual_structure_stack.pop() {
            if let Ok(_removed) =
                registry.delete_sheet(&state.selected_category, &popped.virtual_sheet_name)
            {
                render_cache
                    .clear_sheet_render_data(&state.selected_category, &popped.virtual_sheet_name);
            }
            // If now empty and we were viewing a virtual sheet, swap back to its parent sheet.
            if state.virtual_structure_stack.is_empty() {
                if let Some(sel) = &state.selected_sheet_name {
                    if sel.starts_with("__virtual__") {
                        state.selected_sheet_name = Some(popped.parent.parent_sheet.clone());
                    }
                }
            }
        }
    }
}



