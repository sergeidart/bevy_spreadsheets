// src/sheets/database/writer/daemon_utils.rs
// Utility functions for working with the daemon client

use super::super::error::DbResult;

/// Convert a daemon client error (String) to a rusqlite error.
/// This is a convenience function to reduce boilerplate error conversion.
/// 
/// # Example
/// ```
/// daemon_client.exec_batch(vec![stmt])
///     .map_err(daemon_error_to_rusqlite)?;
/// ```
pub fn daemon_error_to_rusqlite(e: String) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        e
    )))
}

/// Execute a simple SQL statement with parameters through the daemon client.
/// This is a convenience wrapper to reduce Statement boilerplate.
/// 
/// # Arguments
/// * `sql` - The SQL statement to execute
/// * `params` - Parameters for the SQL statement
/// * `daemon_client` - The daemon client
/// * `conn` - The read-only connection (used to extract database name)
/// 
/// # Example
/// ```
/// exec_simple_statement(
///     "UPDATE table SET col = ? WHERE id = ?",
///     vec![value1, value2],
///     daemon_client,
///     &conn
/// )?;
/// ```
pub fn exec_simple_statement(
    sql: String,
    params: Vec<serde_json::Value>,
    daemon_client: &super::super::daemon_client::DaemonClient,
    conn: &rusqlite::Connection,
) -> DbResult<usize> {
    use crate::sheets::database::daemon_client::Statement;
    
    let db_path = get_db_name_from_connection(conn);
    let stmt = Statement { sql, params };
    let response = daemon_client.exec_batch(vec![stmt], db_path.as_deref())
        .map_err(daemon_error_to_rusqlite)?;
    Ok(response.rows_affected.unwrap_or(0))
}

/// Extract database filename from a rusqlite Connection
/// Returns the filename (e.g., "galaxy.db") from the connection path
pub fn get_db_name_from_connection(conn: &rusqlite::Connection) -> Option<String> {
    // Get the database path from the connection
    conn.path()
        .and_then(|path| {
            std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_error_conversion() {
        let error_string = "test error".to_string();
        let rusqlite_err = daemon_error_to_rusqlite(error_string);
        assert!(matches!(rusqlite_err, rusqlite::Error::ToSqlConversionFailure(_)));
    }
}
