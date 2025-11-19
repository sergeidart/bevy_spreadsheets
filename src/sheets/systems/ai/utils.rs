// src/sheets/systems/ai/utils.rs
// Shared helpers for AI systems (non-UI)
use crate::sheets::definitions::StructureFieldDefinition;

/// Extract nested structure field by navigating through JSON using field headers
/// Returns the JSON string representing the nested structure data
///
/// # Arguments
/// * `cell_json` - The JSON string to navigate
/// * `field_path` - Path of field names to navigate through (e.g., ["Skills", "Proficiency"])
///
/// # Returns
/// `Some(json_string)` if the path exists, `None` otherwise
pub fn extract_nested_structure_json(cell_json: &str, field_path: &[String]) -> Option<String> {
    if field_path.is_empty() {
        return Some(cell_json.to_string());
    }

    let trimmed = cell_json.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut current_value = match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Navigate through each level of the field path
    for (depth, field_name) in field_path.iter().enumerate() {
        let is_last = depth == field_path.len() - 1;

        match current_value {
            serde_json::Value::Array(arr) => {
                // For arrays, we need to extract the field from each object in the array
                // and reconstruct as an array
                let mut extracted_values = Vec::new();

                for item in arr {
                    if let serde_json::Value::Object(map) = item {
                        if let Some(nested_value) = map.get(field_name) {
                            extracted_values.push(nested_value.clone());
                        }
                    }
                }

                if extracted_values.is_empty() {
                    return None;
                }

                if is_last {
                    // This is the target field - return all extracted values as array
                    return Some(serde_json::to_string(&extracted_values).unwrap_or_default());
                } else {
                    // Continue navigating - if there are multiple values, we take the first one
                    // (this is a simplification; in practice, nested arrays are complex)
                    current_value = extracted_values.into_iter().next()?;
                }
            }
            serde_json::Value::Object(map) => {
                // For a single object, extract the field
                current_value = map.get(field_name)?.clone();

                if is_last {
                    // This is the target field - return it
                    return Some(serde_json::to_string(&current_value).unwrap_or_default());
                }
            }
            _ => return None,
        }
    }

    Some(serde_json::to_string(&current_value).unwrap_or_default())
}

/// Parse structure rows from a serialized cell string produced previously by serialize_structure_rows_for_review.
/// Accepts either a JSON array of objects or legacy newline/pipe/tab separated formats.
pub fn parse_structure_rows_from_cell(
    cell: &str,
    schema: &[StructureFieldDefinition],
) -> Vec<Vec<String>> {
    let trimmed = cell.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    // Try JSON first
    if trimmed.starts_with('[') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let serde_json::Value::Array(arr) = val {
                // Check if this is a flat array of values (single row) or array of rows
                // Flat array: ["val1", "val2", "val3"] - all elements are primitives
                // Array of rows: [["val1", "val2"], ...] or [{...}, ...] - elements are arrays/objects
                let is_flat_array = arr.iter().all(|item| {
                    matches!(
                        item,
                        serde_json::Value::String(_)
                            | serde_json::Value::Number(_)
                            | serde_json::Value::Bool(_)
                            | serde_json::Value::Null
                    )
                });

                if is_flat_array && !arr.is_empty() {
                    // Handle flat array as a single row: ["val1", "val2", "val3"]
                    let mut row: Vec<String> = Vec::with_capacity(schema.len());
                    for (idx, val) in arr.iter().enumerate() {
                        if idx < schema.len() {
                            let cell_val = match val {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Number(n) => n.to_string(),
                                serde_json::Value::Bool(b) => b.to_string(),
                                serde_json::Value::Null => String::new(),
                                _ => String::new(),
                            };
                            row.push(cell_val);
                        }
                    }
                    // Pad row if needed
                    while row.len() < schema.len() {
                        row.push(String::new());
                    }
                    return vec![row];
                }

                // Handle array of rows
                let mut out = Vec::with_capacity(arr.len());
                for item in arr {
                    match item {
                        // Handle array-of-objects format: [{"header1": "val1", "header2": "val2"}, ...]
                        serde_json::Value::Object(map) => {
                            let mut row: Vec<String> = Vec::with_capacity(schema.len());
                            for field in schema {
                                let v = map
                                    .get(&field.header)
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                row.push(v.to_string());
                            }
                            out.push(row);
                        }
                        // Handle array-of-arrays format: [["val1", "val2"], ["val3", "val4"], ...]
                        serde_json::Value::Array(inner_arr) => {
                            let mut row: Vec<String> = Vec::with_capacity(schema.len());
                            for (idx, val) in inner_arr.iter().enumerate() {
                                if idx < schema.len() {
                                    let cell_val = match val {
                                        serde_json::Value::String(s) => s.clone(),
                                        serde_json::Value::Number(n) => n.to_string(),
                                        serde_json::Value::Bool(b) => b.to_string(),
                                        serde_json::Value::Null => String::new(),
                                        _ => String::new(),
                                    };
                                    row.push(cell_val);
                                }
                            }
                            // Pad row if needed
                            while row.len() < schema.len() {
                                row.push(String::new());
                            }
                            out.push(row);
                        }
                        _ => {} // Skip other types
                    }
                }
                return out;
            }
        }
    }
    // Fallback: split lines, then split by tab or pipe
    let mut rows = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<String> = if line.contains('\t') {
            line.split('\t').map(|s| s.trim().to_string()).collect()
        } else if line.contains('|') {
            line.split('|').map(|s| s.trim().to_string()).collect()
        } else {
            vec![line.to_string()]
        };
        if !parts.is_empty() {
            let mut row = parts;
            if row.len() < schema.len() {
                row.resize(schema.len(), String::new());
            }
            rows.push(row);
        }
    }
    rows
}

/// Build nested field path for structure navigation
///
/// Traverses the structure schema to build a path of field headers
/// for navigating nested structures.
pub fn build_nested_field_path(
    structure_path: &[usize],
    root_meta: &crate::sheets::sheet_metadata::SheetMetadata,
) -> Vec<String> {
    if structure_path.len() <= 1 {
        return Vec::new();
    }

    let mut field_path = Vec::new();
    if let Some(&first_col_idx) = structure_path.first() {
        if let Some(first_col) = root_meta.columns.get(first_col_idx) {
            let mut current_schema = first_col.structure_schema.as_ref();
            for &nested_idx in structure_path.iter().skip(1) {
                if let Some(schema) = current_schema {
                    if let Some(field) = schema.get(nested_idx) {
                        field_path.push(field.header.clone());
                        current_schema = field.structure_schema.as_ref();
                    }
                }
            }
        }
    }
    field_path
}

/// Extract key column index and row generation settings from structure path
///
/// Navigates through the structure schema to find the key parent column
/// and whether row generation is allowed.
///
/// Returns (key_column_index, allow_row_additions)
pub fn extract_structure_settings(
    structure_path: &[usize],
    root_meta: &crate::sheets::sheet_metadata::SheetMetadata,
    registry: Option<&crate::sheets::resources::SheetRegistry>,
    category: &Option<String>,
) -> (Option<usize>, bool) {
    // Default fallback: use parent sheet's setting
    let mut sheet_allow_add_rows = root_meta.ai_enable_row_generation;

    if let Some(&first_path_idx) = structure_path.first() {
        if let Some(first_col) = root_meta.columns.get(first_path_idx) {
            // Get child table's sheet-level setting (this is the authoritative source for structure calls)
            if let Some(reg) = registry {
                // Build child table name: ParentTable_ColumnName
                let parent_table_name = &root_meta.sheet_name;
                let child_table_name = format!("{}_{}", parent_table_name, first_col.header);
                
                if let Some(child_sheet) = reg.get_sheet(category, &child_table_name) {
                    if let Some(child_meta) = &child_sheet.metadata {
                        sheet_allow_add_rows = child_meta.ai_enable_row_generation;
                        bevy::log::info!(
                            "Using child table '{}' sheet-level ai_enable_row_generation={} for structure calls",
                            child_table_name, sheet_allow_add_rows
                        );
                    }
                }
            }
            
            if structure_path.len() == 1 {
                let key_idx = first_col.structure_key_parent_column_index;
                // Use child table's setting directly (already set in sheet_allow_add_rows above)
                let allow_add = sheet_allow_add_rows;
                
                bevy::log::info!(
                    "extract_structure_settings for column {}: using allow_row_additions={}",
                    first_col.header, allow_add
                );
                
                return (key_idx, allow_add);
            } else {
                // Nested structures: start with child table's setting
                let mut current_schema = first_col.structure_schema.as_ref();
                let mut key_idx = first_col.structure_key_parent_column_index;
                let mut allow_add = sheet_allow_add_rows; // Start with child table's setting

                for &nested_idx in structure_path.iter().skip(1) {
                    if let Some(schema) = current_schema {
                        if let Some(field) = schema.get(nested_idx) {
                            key_idx = field.structure_key_parent_column_index;
                            // For nested structures, use field's setting if explicitly set
                            if let Some(explicit_setting) = field.ai_enable_row_generation {
                                allow_add = explicit_setting;
                            }
                            current_schema = field.structure_schema.as_ref();
                        }
                    }
                }
                return (key_idx, allow_add);
            }
        }
    }
    (None, sheet_allow_add_rows)
}
