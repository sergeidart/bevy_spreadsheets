// src/sheets/systems/ai/processor/parser.rs
//! AI Response Parser
//!
//! This module parses AI JSON responses and categorizes rows.
//!
//! ## Responsibilities
//!
//! - Parse Gemini JSON responses
//! - Extract row data with column values
//! - Categorize rows by position: Original (first N rows), AiAdded (rest), Lost (missing rows)
//! - Report parsing errors
//!
//! ## Row Categorization Logic (Position-based)
//!
//! AI always returns original rows first (in order), followed by any AI-added rows.
//! - First `sent_count` rows → Original  
//! - Remaining rows → AI-added
//! - If fewer than `sent_count` rows returned → some are "lost"

use std::collections::HashMap;

/// Result of parsing an AI response
#[derive(Debug, Clone)]
pub struct ParseResult {
    /// Rows that match original data (found in sent values)
    pub original_rows: Vec<ParsedRow>,
    /// Rows that are new (AI-generated, not in sent values)
    pub ai_added_rows: Vec<ParsedRow>,
    /// Display values of rows that were sent but not returned
    pub lost_display_values: Vec<String>,
    /// Fatal parsing error (if any)
    pub error: Option<String>,
}

/// Result of parsing a multi-parent response
#[derive(Debug, Clone)]
pub struct MultiParentParseResult {
    /// Parse results keyed by parent prefix value
    pub by_parent: HashMap<String, ParseResult>,
    /// Rows with unmatched/unknown parent prefix (orphaned rows)
    pub orphaned_rows: Vec<ParsedRow>,
}

impl ParseResult {
    /// Create an error result
    pub fn error(message: String) -> Self {
        Self {
            original_rows: Vec::new(),
            ai_added_rows: Vec::new(),
            lost_display_values: Vec::new(),
            error: Some(message),
        }
    }

    /// Check if parsing was successful
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

/// A single parsed row from the AI response
#[derive(Debug, Clone)]
pub struct ParsedRow {
    /// Display value from the first/key column (for matching)
    pub display_value: String,
    /// Prefix column values (parent/ancestor values) - for ancestor display in review UI
    pub prefix_columns: Vec<String>,
    /// Column values: (column_name, value) - excludes prefix columns
    pub columns: Vec<(String, String)>,
}

impl ParsedRow {
    /// Get value for a column by index (excludes prefix columns)
    pub fn get_column_by_index(&self, index: usize) -> Option<&str> {
        self.columns.get(index).map(|(_, v)| v.as_str())
    }
}

/// AI Response Parser
#[derive(Debug)]
pub struct ResponseParser {
    /// Column names expected in the response (excluding prefix columns)
    expected_columns: Vec<String>,
    /// Index of the key/display column within expected_columns (usually 0 or 1)
    key_column_index: usize,
    /// Name of the key column
    key_column_name: String,
    /// Number of prefix columns (parent/ancestor values) at the start of each row
    prefix_count: usize,
    /// Names of prefix columns (ancestor table names) - for object format parsing
    prefix_column_names: Vec<String>,
}

impl ResponseParser {
    /// Create a new parser with expected columns
    ///
    /// # Arguments
    /// * `expected_columns` - Column names expected in the response (excluding prefix columns)
    /// * `key_column_index` - Index of the column used for display/matching (within expected_columns)
    /// * `prefix_count` - Number of prefix columns (parent values) to skip at the start of each row
    /// * `prefix_column_names` - Names of prefix columns (ancestor table names) for object format parsing
    pub fn new(
        expected_columns: Vec<String>,
        key_column_index: usize,
        prefix_count: usize,
        prefix_column_names: Vec<String>,
    ) -> Self {
        let key_column_name = expected_columns
            .get(key_column_index)
            .cloned()
            .unwrap_or_else(|| "Name".to_string());

        Self {
            expected_columns,
            key_column_index,
            key_column_name,
            prefix_count,
            prefix_column_names,
        }
    }

    /// Parse a response containing mixed rows from multiple parents
    /// 
    /// # Arguments
    /// * `raw_json` - Raw JSON response
    /// * `parent_map` - Map of Parent Prefix -> Sent Count
    /// 
    /// # Returns
    /// MultiParentParseResult with per-parent results and orphaned rows
    pub fn parse_multi_parent_response(
        &self,
        raw_json: &str,
        parent_map: &HashMap<String, usize>,
    ) -> Result<MultiParentParseResult, String> {
        let trimmed = raw_json.trim();
        if trimmed.is_empty() {
            return Err("Empty response from AI".to_string());
        }

        // Try to parse as JSON
        let json_value: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                if let Some(extracted) = extract_json_from_markdown(trimmed) {
                    match serde_json::from_str(&extracted) {
                        Ok(v) => v,
                        Err(e2) => return Err(format!("Failed to parse extracted JSON: {}", e2)),
                    }
                } else {
                    return Err(format!("Failed to parse JSON: {}", e));
                }
            }
        };

        // Extract rows array
        let rows_array = match self.extract_rows_array(&json_value) {
            Ok(arr) => arr,
            Err(_) => return Err("Could not find rows array in response".to_string()),
        };

        // Parse all rows and group by full prefix path
        let mut parent_rows: HashMap<String, Vec<ParsedRow>> = HashMap::new();
        let mut rows_without_prefix: Vec<ParsedRow> = Vec::new();
        
        for row_value in rows_array {
            if let Ok(parsed_row) = self.parse_row(row_value) {
                // Join ALL prefix columns to create unique ancestry key
                // This matches how parent_map keys are built in director.rs
                if !parsed_row.prefix_columns.is_empty() {
                    let prefix_key = parsed_row.prefix_columns.join("|");
                    if !prefix_key.is_empty() {
                        parent_rows.entry(prefix_key).or_default().push(parsed_row);
                    } else {
                        // Empty prefix - treat as orphaned
                        rows_without_prefix.push(parsed_row);
                    }
                } else {
                    // No prefix columns at all - treat as orphaned
                    rows_without_prefix.push(parsed_row);
                }
            }
        }
        
        // Categorize for each known parent
        let mut by_parent = HashMap::new();
        for (prefix, sent_count) in parent_map {
            let rows = parent_rows.remove(prefix).unwrap_or_default();
            let result = self.categorize_parsed_rows(rows, *sent_count);
            by_parent.insert(prefix.clone(), result);
        }
        
        // Remaining rows in parent_rows have prefixes that don't match known parents - orphaned
        let mut orphaned_rows = rows_without_prefix;
        for (_unknown_prefix, rows) in parent_rows {
            orphaned_rows.extend(rows);
        }
        
        Ok(MultiParentParseResult {
            by_parent,
            orphaned_rows,
        })
    }

    /// Categorize parsed rows by position: first sent_count = Original, rest = AI-added
    fn categorize_parsed_rows(&self, rows: Vec<ParsedRow>, sent_count: usize) -> ParseResult {
        let mut original_rows = Vec::new();
        let mut ai_added_rows = Vec::new();
        let total_rows = rows.len();

        for (idx, row) in rows.into_iter().enumerate() {
            if idx < sent_count {
                original_rows.push(row);
            } else {
                ai_added_rows.push(row);
            }
        }

        let lost_count = if total_rows < sent_count {
            sent_count - total_rows
        } else {
            0
        };

        let lost_display_values: Vec<String> = (0..lost_count)
            .map(|i| format!("Row {}", sent_count - lost_count + i))
            .collect();

        ParseResult {
            original_rows,
            ai_added_rows,
            lost_display_values,
            error: None,
        }
    }

    /// Parse an AI response and categorize rows
    ///
    /// # Arguments
    /// * `raw_json` - The raw JSON response from the AI
    /// * `sent_count` - Number of rows that were sent to AI (first N rows = Original, rest = AI-added)
    ///
    /// # Returns
    /// ParseResult with categorized rows
    /// 
    /// # Row Categorization Logic (Position-based)
    /// 
    /// AI always returns original rows first, in order, followed by any AI-added rows.
    /// - First `sent_count` rows → Original
    /// - Remaining rows → AI-added
    /// - If fewer than `sent_count` rows returned → some are "lost"
    pub fn parse(
        &self,
        raw_json: &str,
        sent_count: usize,
    ) -> ParseResult {
        let trimmed = raw_json.trim();

        // Handle empty response
        if trimmed.is_empty() {
            return ParseResult::error("Empty response from AI".to_string());
        }

        // Try to parse as JSON
        let json_value: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                // Try to extract JSON from markdown code blocks
                if let Some(extracted) = extract_json_from_markdown(trimmed) {
                    match serde_json::from_str(&extracted) {
                        Ok(v) => v,
                        Err(e2) => {
                            return ParseResult::error(format!(
                                "Failed to parse extracted JSON: {}",
                                e2
                            ))
                        }
                    }
                } else {
                    return ParseResult::error(format!("Failed to parse JSON: {}", e));
                }
            }
        };

        // Extract rows array
        let rows_array = self.extract_rows_array(&json_value);

        match rows_array {
            Ok(rows) => self.categorize_rows_by_position(rows, sent_count),
            Err(e) => ParseResult::error(e),
        }
    }

    /// Extract the rows array from the JSON value
    fn extract_rows_array<'a>(
        &self,
        value: &'a serde_json::Value,
    ) -> Result<&'a Vec<serde_json::Value>, String> {
        // Direct array at root
        if let Some(arr) = value.as_array() {
            return Ok(arr);
        }

        // Try common wrapper keys
        let wrapper_keys = ["rows", "data", "results", "items"];
        for key in wrapper_keys {
            if let Some(arr) = value.get(key).and_then(|v| v.as_array()) {
                return Ok(arr);
            }
        }

        // If it's an object, maybe it's a single row - wrap it
        if value.is_object() {
            return Err("Response is a single object, expected array of rows".to_string());
        }

        Err("Could not find rows array in response".to_string())
    }

    /// Categorize parsed rows by position: first sent_count = Original, rest = AI-added
    /// 
    /// AI always returns original rows first (in order), then any AI-added rows.
    /// Lost rows = sent_count - number of successfully parsed rows (up to sent_count)
    fn categorize_rows_by_position(
        &self,
        rows: &[serde_json::Value],
        sent_count: usize,
    ) -> ParseResult {
        let mut original_rows = Vec::new();
        let mut ai_added_rows = Vec::new();

        for (idx, row_value) in rows.iter().enumerate() {
            match self.parse_row(row_value) {
                Ok(parsed_row) => {
                    // First sent_count rows are Original, rest are AI-added
                    if idx < sent_count {
                        original_rows.push(parsed_row);
                    } else {
                        ai_added_rows.push(parsed_row);
                    }
                }
                Err(_e) => {
                    // Row-level parse errors are ignored; such rows are skipped.
                    // This could result in fewer original rows than expected.
                }
            }
        }

        // Lost count = how many original row slots were not returned
        // If AI returned fewer rows than sent_count, the missing ones are "lost"
        let lost_count = if rows.len() < sent_count {
            sent_count - rows.len()
        } else {
            0
        };

        // We don't track display_values for lost rows in position-based mode
        // Just report the count
        let lost_display_values: Vec<String> = (0..lost_count)
            .map(|i| format!("Row {}", sent_count - lost_count + i))
            .collect();

        ParseResult {
            original_rows,
            ai_added_rows,
            lost_display_values,
            error: None,
        }
    }

    /// Parse a single row from JSON
    fn parse_row(
        &self,
        row_value: &serde_json::Value,
    ) -> Result<ParsedRow, String> {
        // Handle array format: ["value1", "value2", ...]
        if let Some(arr) = row_value.as_array() {
            return self.parse_array_row(arr);
        }

        // Handle object format: {"column1": "value1", "column2": "value2", ...}
        if let Some(obj) = row_value.as_object() {
            return self.parse_object_row(obj);
        }

        Err(format!(
            "Row is neither array nor object: {:?}",
            row_value
        ))
    }

    /// Parse a row in array format
    fn parse_array_row(
        &self,
        arr: &[serde_json::Value],
    ) -> Result<ParsedRow, String> {
        // Split array into prefix columns and data columns
        let prefix_columns: Vec<String> = arr.iter()
            .take(self.prefix_count)
            .map(value_to_string)
            .collect();
        
        let mut columns = Vec::new();
        for (idx, value) in arr.iter().skip(self.prefix_count).enumerate() {
            let column_name = self
                .expected_columns
                .get(idx)
                .cloned()
                .unwrap_or_else(|| format!("Column{}", idx));

            let string_value = value_to_string(value);
            columns.push((column_name, string_value));
        }

        // Extract display value from key column (within data columns, not prefix)
        let display_value = columns
            .get(self.key_column_index)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();

        Ok(ParsedRow {
            display_value,
            prefix_columns,
            columns,
        })
    }

    /// Parse a row in object format
    /// 
    /// Extracts prefix columns by looking up prefix_column_names in the object keys.
    fn parse_object_row(
        &self,
        obj: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<ParsedRow, String> {
        // Extract prefix columns using known prefix column names
        let prefix_columns: Vec<String> = self.prefix_column_names
            .iter()
            .map(|prefix_name| {
                obj.get(prefix_name)
                    .or_else(|| {
                        // Try case-insensitive match
                        obj.iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(prefix_name))
                            .map(|(_, v)| v)
                    })
                    .map(value_to_string)
                    .unwrap_or_default()
            })
            .collect();

        let mut columns = Vec::new();

        // Match by expected column names (data columns, excluding prefix)
        for col_name in &self.expected_columns {
            let value = obj
                .get(col_name)
                .or_else(|| {
                    // Try case-insensitive match
                    obj.iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(col_name))
                        .map(|(_, v)| v)
                })
                .map(value_to_string)
                .unwrap_or_default();

            columns.push((col_name.clone(), value));
        }

        // If no expected columns matched, use all object keys (excluding prefix column names)
        if columns.iter().all(|(_, v)| v.is_empty()) && !obj.is_empty() {
            columns.clear();
            for (key, value) in obj {
                // Skip prefix column names when falling back
                let is_prefix_col = self.prefix_column_names
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case(key));
                if !is_prefix_col {
                    columns.push((key.clone(), value_to_string(value)));
                }
            }
        }

        // Extract display value
        let display_value = if let Some(val) = obj.get(&self.key_column_name) {
            value_to_string(val)
        } else {
            // Try case-insensitive
            obj.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&self.key_column_name))
                .map(|(_, v)| value_to_string(v))
                .or_else(|| {
                    // Fall back to first column
                    columns.get(self.key_column_index).map(|(_, v)| v.clone())
                })
                .unwrap_or_default()
        };

        Ok(ParsedRow {
            display_value,
            prefix_columns,
            columns,
        })
    }
}

/// Extract JSON from markdown code blocks
fn extract_json_from_markdown(text: &str) -> Option<String> {
    // Try to find ```json ... ``` blocks
    if let Some(start) = text.find("```json") {
        let content_start = start + 7;
        if let Some(end) = text[content_start..].find("```") {
            return Some(text[content_start..content_start + end].trim().to_string());
        }
    }

    // Try to find ``` ... ``` blocks
    if let Some(start) = text.find("```") {
        let content_start = start + 3;
        // Skip language identifier if present
        let content_start = text[content_start..]
            .find('\n')
            .map(|i| content_start + i + 1)
            .unwrap_or(content_start);

        if let Some(end) = text[content_start..].find("```") {
            return Some(text[content_start..content_start + end].trim().to_string());
        }
    }

    // Try to find [ ... ] or { ... }
    if let Some(start) = text.find('[') {
        if let Some(end) = text.rfind(']') {
            if end > start {
                return Some(text[start..=end].to_string());
            }
        }
    }

    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                return Some(text[start..=end].to_string());
            }
        }
    }

    None
}

/// Convert a JSON value to a string representation
fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            // For nested structures, return as JSON string
            serde_json::to_string(value).unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_array_format() {
        let parser = ResponseParser::new(vec!["Name".to_string(), "Speed".to_string()], 0, 0, Vec::new());

        let json = r#"[
            ["MiG-25PD", "3000"],
            ["Su-27", "2500"]
        ]"#;

        // Sent 1 row, so first row = Original, second = AI-added
        let result = parser.parse(json, 1);

        assert!(result.is_success());
        assert_eq!(result.original_rows.len(), 1);
        assert_eq!(result.ai_added_rows.len(), 1);
        assert_eq!(result.original_rows[0].display_value, "MiG-25PD");
        assert_eq!(result.ai_added_rows[0].display_value, "Su-27");
    }

    #[test]
    fn test_parse_object_format() {
        let parser = ResponseParser::new(vec!["Name".to_string(), "Speed".to_string()], 0, 0, Vec::new());

        let json = r#"[
            {"Name": "MiG-25PD", "Speed": "3000"},
            {"Name": "F-16C", "Speed": "2100"}
        ]"#;

        // Sent 2 rows, received 2 rows - both are Original
        let result = parser.parse(json, 2);

        assert!(result.is_success());
        assert_eq!(result.original_rows.len(), 2);
        assert_eq!(result.ai_added_rows.len(), 0);
        assert_eq!(result.lost_display_values.len(), 0);
    }

    #[test]
    fn test_parse_wrapped_response() {
        let parser = ResponseParser::new(vec!["Name".to_string()], 0, 0, Vec::new());

        let json = r#"{"rows": [["Aircraft1"], ["Aircraft2"]]}"#;

        // Sent 0 rows, so all are AI-added
        let result = parser.parse(json, 0);

        assert!(result.is_success());
        assert_eq!(result.ai_added_rows.len(), 2);
    }

    #[test]
    fn test_extract_from_markdown() {
        let parser = ResponseParser::new(vec!["Name".to_string()], 0, 0, Vec::new());

        let markdown = r#"Here's the data:
```json
[["Test1"], ["Test2"]]
```
"#;

        // Sent 0 rows, so all are AI-added
        let result = parser.parse(markdown, 0);

        assert!(result.is_success());
        assert_eq!(result.ai_added_rows.len(), 2);
    }

    #[test]
    fn test_categorization() {
        let parser = ResponseParser::new(vec!["Name".to_string(), "Value".to_string()], 0, 0, Vec::new());

        let json = r#"[
            ["Original1", "100"],
            ["Original2", "200"],
            ["NewRow", "300"]
        ]"#;

        // Sent 2 rows: first 2 = Original, third = AI-added
        let result = parser.parse(json, 2);

        assert!(result.is_success());
        assert_eq!(result.original_rows.len(), 2);
        assert_eq!(result.ai_added_rows.len(), 1);
        assert_eq!(result.lost_display_values.len(), 0); // All 2 originals returned
    }

    #[test]
    fn test_lost_rows() {
        let parser = ResponseParser::new(vec!["Name".to_string(), "Value".to_string()], 0, 0, Vec::new());

        let json = r#"[
            ["Original1", "100"]
        ]"#;

        // Sent 3 rows but only 1 returned - 2 are lost
        let result = parser.parse(json, 3);

        assert!(result.is_success());
        assert_eq!(result.original_rows.len(), 1);
        assert_eq!(result.ai_added_rows.len(), 0);
        assert_eq!(result.lost_display_values.len(), 2); // 3 sent - 1 returned = 2 lost
    }

    #[test]
    fn test_empty_response() {
        let parser = ResponseParser::new(vec!["Name".to_string()], 0, 0, Vec::new());
        let result = parser.parse("", 0);

        assert!(!result.is_success());
        assert!(result.error.is_some());
    }

    #[test]
    fn test_nested_json_values() {
        let parser = ResponseParser::new(
            vec!["Name".to_string(), "Skills".to_string()],
            0,
            0,
            Vec::new(),
        );

        let json = r#"[
            {"Name": "Pilot1", "Skills": ["Flying", "Navigation"]}
        ]"#;

        // Sent 0, so it's AI-added
        let result = parser.parse(json, 0);

        assert!(result.is_success());
        assert_eq!(result.ai_added_rows.len(), 1);

        let skills = result.ai_added_rows[0].get_column_by_index(1);
        assert!(skills.is_some());
        // Nested arrays are stored as JSON strings
        assert!(skills.unwrap().contains("Flying"));
    }

    #[test]
    fn test_parse_multi_parent_response_uses_full_ancestry() {
        // Test that multi-parent parsing uses the FULL ancestry path as key
        // to uniquely identify parents across different branches
        let parser = ResponseParser::new(
            vec!["Weapon".to_string(), "Damage".to_string()],
            0,
            2, // 2 prefix columns: grandparent, parent
            vec!["Aircraft".to_string(), "Pylon".to_string()],
        );

        // AI returns rows with prefix [grandparent, parent]
        // The full path "LaGG-3-66|Internal Gun 2" is the key
        let json = r#"[
            ["LaGG-3-66", "Internal Gun 2", "Cannon", "150"],
            ["LaGG-3-66", "Internal Gun 2", "MG", "50"]
        ]"#;

        // parent_map uses full ancestry path as key (joined with |)
        let mut parent_map = std::collections::HashMap::new();
        parent_map.insert("LaGG-3-66|Internal Gun 2".to_string(), 1);

        let result = parser.parse_multi_parent_response(json, &parent_map).unwrap();

        // Rows should be matched to full ancestry key, not orphaned
        assert_eq!(result.orphaned_rows.len(), 0, "Rows should not be orphaned when full ancestry matches");
        
        let parent_result = result.by_parent.get("LaGG-3-66|Internal Gun 2").unwrap();
        assert_eq!(parent_result.original_rows.len(), 1, "First row should be original");
        assert_eq!(parent_result.ai_added_rows.len(), 1, "Second row should be AI-added");
    }

    #[test]
    fn test_parse_multi_parent_response_distinguishes_same_name_parents() {
        // Test that parents with same name but different ancestry are distinguished
        let parser = ResponseParser::new(
            vec!["Weapon".to_string()],
            0,
            2, // 2 prefix columns
            vec!["Aircraft".to_string(), "Pylon".to_string()],
        );

        // Two different "Pylon 1" under different aircraft
        let json = r#"[
            ["F-16", "Pylon 1", "AIM-9"],
            ["MiG-25", "Pylon 1", "R-40"]
        ]"#;

        let mut parent_map = std::collections::HashMap::new();
        parent_map.insert("F-16|Pylon 1".to_string(), 1);
        parent_map.insert("MiG-25|Pylon 1".to_string(), 1);

        let result = parser.parse_multi_parent_response(json, &parent_map).unwrap();

        assert_eq!(result.orphaned_rows.len(), 0);
        
        let f16_result = result.by_parent.get("F-16|Pylon 1").unwrap();
        assert_eq!(f16_result.original_rows.len(), 1);
        assert_eq!(f16_result.original_rows[0].display_value, "AIM-9");
        
        let mig_result = result.by_parent.get("MiG-25|Pylon 1").unwrap();
        assert_eq!(mig_result.original_rows.len(), 1);
        assert_eq!(mig_result.original_rows[0].display_value, "R-40");
    }

    #[test]
    fn test_parse_multi_parent_response_orphans_unmatched() {
        // Test that rows with unmatched ancestry are correctly marked as orphaned
        let parser = ResponseParser::new(
            vec!["Weapon".to_string(), "Damage".to_string()],
            0,
            1, // 1 prefix column: parent
            vec!["Pylon".to_string()],
        );

        let json = r#"[
            ["Known Parent", "Cannon", "150"],
            ["Unknown Parent", "MG", "50"]
        ]"#;

        let mut parent_map = std::collections::HashMap::new();
        parent_map.insert("Known Parent".to_string(), 1);

        let result = parser.parse_multi_parent_response(json, &parent_map).unwrap();

        // Row with "Known Parent" should be matched
        let parent_result = result.by_parent.get("Known Parent").unwrap();
        assert_eq!(parent_result.original_rows.len(), 1);
        
        // Row with "Unknown Parent" should be orphaned
        assert_eq!(result.orphaned_rows.len(), 1, "Unmatched row should be orphaned");
        assert_eq!(result.orphaned_rows[0].display_value, "MG");
    }
}