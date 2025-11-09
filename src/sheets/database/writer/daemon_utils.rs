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
/// # Example
/// ```
/// exec_simple_statement(
///     "UPDATE table SET col = ? WHERE id = ?",
///     vec![value1, value2],
///     daemon_client
/// )?;
/// ```
pub fn exec_simple_statement(
    sql: String,
    params: Vec<serde_json::Value>,
    daemon_client: &super::super::daemon_client::DaemonClient,
) -> DbResult<usize> {
    use crate::sheets::database::daemon_client::Statement;
    
    let stmt = Statement { sql, params };
    let response = daemon_client.exec_batch(vec![stmt])
        .map_err(daemon_error_to_rusqlite)?;
    Ok(response.rows_affected.unwrap_or(0))
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
