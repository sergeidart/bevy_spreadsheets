// src/sheets/database/migration/json_extractor.rs

use crate::sheets::definitions::StructureFieldDefinition;

/// Convert any JSON value to string losslessly enough for grid
pub fn json_value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        // For nested structures, serialize compactly
        _ => v.to_string(),
    }
}

/// Normalize a key string by lowercasing and stripping non-alphanumeric characters
/// This helps match headers with minor differences
pub fn normalize_key_str(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect::<String>()
}

/// Check if a row has any non-empty value
pub fn row_has_any_value(row: &[String]) -> bool {
    row.iter().any(|s| !s.trim().is_empty())
}

/// Parse JSON cell value, supporting double-encoded JSON strings
pub fn parse_cell_json(cell: &str) -> serde_json::Value {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(cell) {
        // If it's a string that looks like JSON, try parsing once more
        if let serde_json::Value::String(s) = &v {
            let st = s.trim();
            if (st.starts_with('[') && st.ends_with(']'))
                || (st.starts_with('{') && st.ends_with('}'))
            {
                if let Ok(v2) = serde_json::from_str::<serde_json::Value>(st) {
                    return v2;
                }
            }
        }
        v
    } else {
        // Try wrap single object as array
        let ts = cell.trim();
        if ts.starts_with('{') && ts.ends_with('}') {
            let wrapped = format!("[{}]", ts);
            serde_json::from_str(&wrapped).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        }
    }
}

/// Expand a JSON value into rows honoring schema ordering; handle common wrappers
pub fn expand_value_to_rows(
    val: serde_json::Value,
    schema_fields: &[StructureFieldDefinition],
    structure_header: &str,
) -> Vec<Vec<String>> {
    let header_norm = normalize_key_str(structure_header);
    match val {
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                return Vec::new();
            }
            if arr.iter().all(|v| v.is_array()) {
                // [[...], [...]]: map by position
                let mut out: Vec<Vec<String>> = Vec::with_capacity(arr.len());
                for inner in arr.into_iter() {
                    if let Some(ia) = inner.as_array() {
                        let cols = schema_fields.len();
                        let mut row_vec: Vec<String> = ia
                            .iter()
                            .take(cols)
                            .map(json_value_to_string)
                            .collect();
                        if row_vec.len() < cols {
                            row_vec.resize(cols, String::new());
                        }
                        if row_has_any_value(&row_vec) {
                            out.push(row_vec);
                        }
                    }
                }
                return out;
            }
            if arr.iter().all(|v| v.is_object()) {
                // [ { field: val }, ... ] map by schema order with normalized key matching
                let mut out: Vec<Vec<String>> = Vec::with_capacity(arr.len());
                for obj in arr.into_iter() {
                    let map = obj.as_object().cloned().unwrap_or_default();
                    let mut norm_map: std::collections::HashMap<
                        String,
                        &serde_json::Value,
                    > = std::collections::HashMap::new();
                    for (k, v) in &map {
                        norm_map.insert(normalize_key_str(k), v);
                    }
                    let mut row_vec = Vec::with_capacity(schema_fields.len());
                    for f in schema_fields {
                        let key_norm = normalize_key_str(&f.header);
                        let val = map
                            .get(&f.header)
                            .or_else(|| norm_map.get(&key_norm).copied());
                        row_vec.push(
                            val.map(json_value_to_string).unwrap_or_default(),
                        );
                    }
                    if row_has_any_value(&row_vec) {
                        out.push(row_vec);
                    }
                }
                return out;
            }
            // Array of primitives or mixed -> map by position when possible
            if !schema_fields.is_empty() {
                // Convert all items to strings (objects/arrays become compact JSON)
                let values: Vec<String> =
                    arr.iter().map(json_value_to_string).collect();
                let cols = schema_fields.len();

                if cols == 1 {
                    // Single-field schema: N rows, one per value
                    let mut out: Vec<Vec<String>> =
                        Vec::with_capacity(values.len());
                    for v in values {
                        if !v.trim().is_empty() {
                            out.push(vec![v]);
                        }
                    }
                    return out;
                }

                // Multi-field schema
                if values.len() == cols {
                    // Exact fit -> single row by position
                    let mut row_vec =
                        values.into_iter().take(cols).collect::<Vec<_>>();
                    if row_vec.len() < cols {
                        row_vec.resize(cols, String::new());
                    }
                    if row_has_any_value(&row_vec) {
                        return vec![row_vec];
                    }
                    return Vec::new();
                }
                if values.len() % cols == 0 {
                    // Chunk into groups of cols
                    let mut out: Vec<Vec<String>> =
                        Vec::with_capacity(values.len() / cols);
                    for chunk in values.chunks(cols) {
                        let mut row_vec =
                            chunk.iter().cloned().collect::<Vec<_>>();
                        if row_vec.len() < cols {
                            row_vec.resize(cols, String::new());
                        }
                        if row_has_any_value(&row_vec) {
                            out.push(row_vec);
                        }
                    }
                    return out;
                }
                // Fallback: map first N values into one row
                let mut row_vec =
                    values.into_iter().take(cols).collect::<Vec<_>>();
                if row_vec.len() < cols {
                    row_vec.resize(cols, String::new());
                }
                if row_has_any_value(&row_vec) {
                    return vec![row_vec];
                }
                return Vec::new();
            }
            Vec::new()
        }
        serde_json::Value::Object(map) => {
            // If object contains an array under a key matching the structure header, prefer that
            let mut norm_map: std::collections::HashMap<
                String,
                &serde_json::Value,
            > = std::collections::HashMap::new();
            for (k, v) in &map {
                norm_map.insert(normalize_key_str(k), v);
            }

            if let Some(arr_val) = norm_map.get(&header_norm).and_then(|v| {
                if v.is_array() {
                    Some((*v).clone())
                } else {
                    None
                }
            }) {
                return expand_value_to_rows(
                    arr_val,
                    schema_fields,
                    structure_header,
                );
            }

            // Then look for common wrapper keys
            let candidate_keys =
                ["Rows", "rows", "items", "Items", "data", "Data"];
            if let Some((_, arr_val)) = map
                .iter()
                .find(|(k, v)| {
                    v.is_array() && candidate_keys.contains(&k.as_str())
                })
                .or_else(|| map.iter().find(|(_, v)| v.is_array()))
            {
                return expand_value_to_rows(
                    arr_val.clone(),
                    schema_fields,
                    structure_header,
                );
            }
            // Otherwise, map this object as a single row with normalized key matching
            if schema_fields.is_empty() {
                return Vec::new();
            }
            let mut norm_map: std::collections::HashMap<
                String,
                &serde_json::Value,
            > = std::collections::HashMap::new();
            for (k, v) in &map {
                norm_map.insert(normalize_key_str(k), v);
            }
            let mut row_vec = Vec::with_capacity(schema_fields.len());
            for f in schema_fields {
                let key_norm = normalize_key_str(&f.header);
                let val = map
                    .get(&f.header)
                    .or_else(|| norm_map.get(&key_norm).copied());
                row_vec.push(val.map(json_value_to_string).unwrap_or_default());
            }
            if row_has_any_value(&row_vec) {
                vec![row_vec]
            } else {
                Vec::new()
            }
        }
        serde_json::Value::String(s) => {
            // Try parse inner JSON
            let inner = parse_cell_json(&s);
            expand_value_to_rows(inner, schema_fields, structure_header)
        }
        _ => Vec::new(),
    }
}
