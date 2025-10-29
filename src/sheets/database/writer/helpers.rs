// src/sheets/database/writer/helpers.rs
// Helper functions for SQL generation and parameter preparation

use rusqlite::ToSql;

/// Quote a SQL identifier by wrapping it in double quotes.
/// This protects against SQL injection and allows special characters in names.
/// 
/// # Example
/// ```
/// let quoted = quote_identifier("User Name");
/// assert_eq!(quoted, "\"User Name\"");
/// ```
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name)
}

/// Build a comma-separated list of quoted column names.
/// 
/// # Example
/// ```
/// let cols = vec!["Name".to_string(), "Age".to_string()];
/// let quoted = quote_column_list(&cols);
/// assert_eq!(quoted, "\"Name\", \"Age\"");
/// ```
pub fn quote_column_list(columns: &[String]) -> String {
    columns
        .iter()
        .map(|name| quote_identifier(name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build a string of SQL placeholders (?, ?, ?, ...).
/// 
/// # Example
/// ```
/// let placeholders = build_placeholders(3);
/// assert_eq!(placeholders, "?, ?, ?");
/// ```
pub fn build_placeholders(count: usize) -> String {
    (0..count)
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build an INSERT SQL statement.
/// 
/// # Example
/// ```
/// let cols = vec!["Name".to_string(), "Age".to_string()];
/// let sql = build_insert_sql("Users", &cols);
/// assert_eq!(sql, "INSERT INTO \"Users\" (row_index, \"Name\", \"Age\") VALUES (?, ?, ?)");
/// ```
pub fn build_insert_sql(table_name: &str, columns: &[String]) -> String {
    let cols_str = quote_column_list(columns);
    let placeholders = build_placeholders(columns.len());
    
    format!(
        "INSERT INTO {} (row_index, {}) VALUES (?, {})",
        quote_identifier(table_name),
        cols_str,
        placeholders
    )
}

/// Build an UPDATE SQL statement for a single column.
/// 
/// # Example
/// ```
/// let sql = build_update_sql("Users", "Name", "row_index = ?");
/// assert_eq!(sql, "UPDATE \"Users\" SET \"Name\" = ? WHERE row_index = ?");
/// ```
pub fn build_update_sql(table_name: &str, column_name: &str, where_clause: &str) -> String {
    format!(
        "UPDATE {} SET {} = ? WHERE {}",
        quote_identifier(table_name),
        quote_identifier(column_name),
        where_clause
    )
}

/// Build a DELETE SQL statement.
/// 
/// # Example
/// ```
/// let sql = build_delete_sql("Users", "row_index = ?");
/// assert_eq!(sql, "DELETE FROM \"Users\" WHERE row_index = ?");
/// ```
pub fn build_delete_sql(table_name: &str, where_clause: &str) -> String {
    format!(
        "DELETE FROM {} WHERE {}",
        quote_identifier(table_name),
        where_clause
    )
}

/// Get the metadata table name for a given table.
/// 
/// # Example
/// ```
/// let meta = metadata_table_name("Users");
/// assert_eq!(meta, "Users_Metadata");
/// ```
pub fn metadata_table_name(table_name: &str) -> String {
    format!("{}_Metadata", table_name)
}

/// Prepare parameters for SQL execution by boxing values.
/// Adds row_data strings to the params vector.
pub fn append_string_params(
    params: &mut Vec<Box<dyn ToSql>>,
    row_data: &[String],
) {
    for cell in row_data {
        params.push(Box::new(cell.clone()));
    }
}

/// Prepare parameters for SQL execution by boxing values.
/// Takes initial parameters and row data, returns a complete parameter vector.
pub fn prepare_params_with_row_data(
    initial: Vec<Box<dyn ToSql>>,
    row_data: &[String],
) -> Vec<Box<dyn ToSql>> {
    let mut params = initial;
    append_string_params(&mut params, row_data);
    params
}

/// Pad parameters vector with empty strings up to a target length.
/// Useful for ensuring all columns have values even if data is sparse.
pub fn pad_params_with_empty_strings(
    params: &mut Vec<Box<dyn ToSql>>,
    target_len: usize,
) {
    while params.len() < target_len {
        params.push(Box::new(String::new()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_identifier() {
        assert_eq!(quote_identifier("Name"), "\"Name\"");
        assert_eq!(quote_identifier("User Name"), "\"User Name\"");
    }

    #[test]
    fn test_quote_column_list() {
        let cols = vec!["Name".to_string(), "Age".to_string()];
        assert_eq!(quote_column_list(&cols), "\"Name\", \"Age\"");
    }

    #[test]
    fn test_build_placeholders() {
        assert_eq!(build_placeholders(0), "");
        assert_eq!(build_placeholders(1), "?");
        assert_eq!(build_placeholders(3), "?, ?, ?");
    }

    #[test]
    fn test_build_insert_sql() {
        let cols = vec!["Name".to_string(), "Age".to_string()];
        let sql = build_insert_sql("Users", &cols);
        assert_eq!(
            sql,
            "INSERT INTO \"Users\" (row_index, \"Name\", \"Age\") VALUES (?, ?, ?)"
        );
    }

    #[test]
    fn test_build_update_sql() {
        let sql = build_update_sql("Users", "Name", "row_index = ?");
        assert_eq!(sql, "UPDATE \"Users\" SET \"Name\" = ? WHERE row_index = ?");
    }

    #[test]
    fn test_build_delete_sql() {
        let sql = build_delete_sql("Users", "row_index = ?");
        assert_eq!(sql, "DELETE FROM \"Users\" WHERE row_index = ?");
    }

    #[test]
    fn test_metadata_table_name() {
        assert_eq!(metadata_table_name("Users"), "Users_Metadata");
        assert_eq!(metadata_table_name("Games_Items"), "Games_Items_Metadata");
    }

    #[test]
    fn test_prepare_params_with_row_data() {
        let initial: Vec<Box<dyn ToSql>> = vec![Box::new(42i32)];
        let row_data = vec!["Alice".to_string(), "Bob".to_string()];
        let params = prepare_params_with_row_data(initial, &row_data);
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_pad_params_with_empty_strings() {
        let mut params: Vec<Box<dyn ToSql>> = vec![Box::new("test".to_string())];
        pad_params_with_empty_strings(&mut params, 4);
        assert_eq!(params.len(), 4);
    }
}
