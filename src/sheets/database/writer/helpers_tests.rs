#[cfg(test)]
mod tests {
    use super::super::helpers::*;

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
    fn test_metadata_table_name() {
        assert_eq!(metadata_table_name("Users"), "Users_Metadata");
        assert_eq!(metadata_table_name("Games_Items"), "Games_Items_Metadata");
    }
}