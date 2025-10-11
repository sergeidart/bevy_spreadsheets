// src/sheets/systems/ai/structure_results/schema.rs
// Functions for extracting schema and nested structure rows

use bevy::prelude::*;
use serde_json::Value as JsonValue;

use crate::sheets::definitions::StructureFieldDefinition;

/// Extract original nested structure rows for a structure path with depth > 1.
/// Walks the JSON in the parent cell and collects rows from the nested array at the target depth.
/// Falls back to a single empty row if traversal fails.
///
/// NOTE: Current implementation supports exactly one additional nested level (root -> nested field).
/// Deeper paths will return a single empty row so the UI can still render a consistent preview.
/// This is sufficient for restoring original previews where previously nested levels showed as (empty).
pub fn extract_original_nested_structure_rows(
    cell_value: &str,
    structure_path: &[usize],
    sheet: &crate::sheets::definitions::SheetGridData,
    target_schema_fields: &[StructureFieldDefinition],
    schema_len: usize,
) -> Vec<Vec<String>> {
    // If cell empty, return one blank row
    let trimmed = cell_value.trim();
    if trimmed.is_empty() {
        return vec![vec![String::new(); schema_len]];
    }

    let Some(meta) = sheet.metadata.as_ref() else {
        return vec![vec![String::new(); schema_len]];
    };

    // Parse the root JSON (array of objects expected for structure column)
    let Ok(root_json) = serde_json::from_str::<JsonValue>(trimmed) else {
        return vec![vec![String::new(); schema_len]];
    };

    // Helper to get schema vector for current level (after first column)
    // We navigate using the indices in structure_path beyond root (which references sheet column itself)
    let first_col_def = meta.columns.get(structure_path[0]);
    let mut current_schema_opt = first_col_def.and_then(|c| c.structure_schema.as_ref());

    // Collect arrays representing the current level's rows (start with the root array)
    let mut current_level_arrays: Vec<&[JsonValue]> = Vec::new();
    if let JsonValue::Array(arr) = &root_json {
        current_level_arrays.push(arr.as_slice());
    } else {
        return vec![vec![String::new(); schema_len]];
    }

    // Traverse each nested index except the final one: we need to arrive at the parent whose field holds the target array
    for (depth, &nested_idx) in structure_path.iter().enumerate().skip(1) {
        let Some(schema) = current_schema_opt else {
            return vec![vec![String::new(); schema_len]];
        };
        let Some(field_def) = schema.get(nested_idx) else {
            return vec![vec![String::new(); schema_len]];
        };
        let field_header = &field_def.header;

        // If this is the last index in the path, we extract the array(s) at this field as final rows
        let is_last = depth == structure_path.len() - 1;
        let mut next_level_row_objects: Vec<&serde_json::Map<String, JsonValue>> = Vec::new();
        let mut final_row_objects: Vec<&serde_json::Map<String, JsonValue>> = Vec::new();

        for arr in &current_level_arrays {
            for item in *arr {
                if let JsonValue::Object(obj) = item {
                    if let Some(field_val) = obj.get(field_header) {
                        if is_last {
                            if let JsonValue::Array(nested_rows) = field_val {
                                for row_item in nested_rows {
                                    if let JsonValue::Object(row_obj) = row_item {
                                        final_row_objects.push(row_obj);
                                    }
                                }
                            }
                        } else {
                            if let JsonValue::Array(nested_arr) = field_val {
                                // Collect all objects at this intermediate level to traverse further
                                for row_item in nested_arr {
                                    if let JsonValue::Object(row_obj) = row_item {
                                        next_level_row_objects.push(row_obj);
                                    }
                                }
                                // We'll rebuild current_level_arrays from the nested arrays for deeper traversal
                            }
                        }
                    }
                }
            }
        }

        if is_last {
            if final_row_objects.is_empty() {
                return vec![vec![String::new(); schema_len]];
            }
            // Map final objects into rows using target schema fields
            let mut out_rows = Vec::with_capacity(final_row_objects.len());
            for obj in final_row_objects {
                let mut row = Vec::with_capacity(schema_len);
                for field in target_schema_fields {
                    row.push(
                        obj.get(&field.header)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                }
                if row.len() < schema_len {
                    row.resize(schema_len, String::new());
                }
                out_rows.push(row);
            }
            return out_rows;
        } else {
            // Prepare for next loop: build arrays from next_level_row_objects by collecting nested arrays again.
            // But we actually need the nested arrays, not the objects; since we already iterated arrays we can rebuild from objects containing further nested structure later.
            // For simplicity, rebuild current_level_arrays from any nested structure arrays contained in the gathered objects for the next nested_idx.
            // To do that we peek at the next schema after current field.
            let next_schema_opt = field_def.structure_schema.as_ref();
            if next_schema_opt.is_none() {
                return vec![vec![String::new(); schema_len]];
            }
            current_schema_opt = next_schema_opt;

            // Rebuild current_level_arrays by pulling arrays from the collected objects (for the next nested iteration)
            let rebuilt: Vec<&[JsonValue]> = Vec::new();
            for obj in next_level_row_objects {
                // The next iteration will look up a field by header, but we don't yet know which header; that's fine because we rebuild when we know nested_idx.
                // So we keep the entire object array contents by finding all child arrays in this object whose key matches the upcoming field when iterating.
                // Instead of guessing now, we'll postpone and reconstruct from the original traversal logic; thus we leave current_level_arrays empty forcing early return if deeper than one level.
                // NOTE: For now we only support a single nested level reliably. If deeper nesting is required, a more elaborate traversal should be implemented.
                let _ = obj; // silence unused warning if feature not extended
            }
            if structure_path.len() > 2 {
                // deeper than one nested level not yet fully supported
                debug!("Nested structure original extraction: deeper than 2 levels not fully supported (path {:?})", structure_path);
                return vec![vec![String::new(); schema_len]];
            }
            current_level_arrays = rebuilt; // likely empty -> triggers fallback
        }
    }

    vec![vec![String::new(); schema_len]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheets::definitions::{
        ColumnDataType, ColumnDefinition, ColumnValidator, SheetMetadata, StructureFieldDefinition,
    };

    #[test]
    fn test_extract_original_nested_structure_rows_one_level() {
        // Simulate a structure cell with nested structure field "items"
        // Root schema: [{ name: String, items: Structure[...] }]
        let json = r#"[
            {"name":"A", "items":[{"val":"1"},{"val":"2"}]},
            {"name":"B", "items":[{"val":"3"}]}
        ]"#;

        // Build fake sheet metadata with one structure column at index 0
        let nested_leaf_field = StructureFieldDefinition {
            header: "val".to_string(),
            validator: Some(ColumnValidator::Basic(ColumnDataType::String)),
            data_type: ColumnDataType::String,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        };
        let nested_field = StructureFieldDefinition {
            header: "items".to_string(),
            validator: Some(ColumnValidator::Structure),
            data_type: ColumnDataType::String,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            width: None,
            structure_schema: Some(vec![nested_leaf_field.clone()]),
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        };
        let name_field = StructureFieldDefinition {
            header: "name".to_string(),
            validator: Some(ColumnValidator::Basic(ColumnDataType::String)),
            data_type: ColumnDataType::String,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        };
        let root_col = ColumnDefinition {
            header: "root".to_string(),
            validator: Some(ColumnValidator::Structure),
            data_type: ColumnDataType::String,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            deleted: false,
            hidden: false, // Test column, not hidden
            width: None,
            structure_schema: Some(vec![name_field.clone(), nested_field.clone()]),
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        };
        let meta = SheetMetadata {
            sheet_name: "test".to_string(),
            category: None,
            data_filename: "test.json".to_string(),
            columns: vec![root_col],
            ai_general_rule: None,
            ai_model_id: crate::sheets::definitions::default_ai_model_id(),
            ai_temperature: None,
            requested_grounding_with_google_search:
                crate::sheets::definitions::default_grounding_with_google_search(),
            ai_enable_row_generation: true,
            ai_schema_groups: Vec::new(),
            ai_active_schema_group: None,
            random_picker: None,
            structure_parent: None,
            hidden: false,
        };
        let sheet = crate::sheets::definitions::SheetGridData {
            grid: vec![vec![json.to_string()]],
            metadata: Some(meta),
        };

        // Target schema is the nested field schema (val)
        let target_schema = nested_field.structure_schema.as_ref().unwrap();
        let rows = extract_original_nested_structure_rows(
            &sheet.grid[0][0],
            &[0, 1],
            &sheet,
            target_schema,
            target_schema.len(),
        );
        // NOTE: Current implementation returns a flattened sequence of nested rows per parent object.
        assert_eq!(rows.len(), 3, "Expected 3 flattened nested rows");
        assert_eq!(rows[0][0], "1");
        assert_eq!(rows[1][0], "2");
        assert_eq!(rows[2][0], "3");
    }
}
