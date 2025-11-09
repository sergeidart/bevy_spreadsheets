// src/sheets/database/writer/test_helpers.rs
// Test utilities for database writer tests

#![cfg(test)]

use rusqlite::{params, Connection};
use crate::sheets::database::daemon_client::DaemonClient;

/// Create a mock daemon client for testing
/// 
/// Returns a DaemonClient that simulates successful operations without
/// requiring an actual daemon process. All operations return success.
/// 
/// # Example
/// ```
/// let mock_daemon = create_mock_daemon_client();
/// DbWriter::prepend_row(&conn, table, &data, &cols, &mock_daemon).unwrap();
/// ```
pub fn create_mock_daemon_client() -> DaemonClient {
    // Create a mock client that bypasses actual daemon communication
    // The internal structure will handle mock responses
    DaemonClient::new_mock()
}

/// Set up a simple test table with standard columns for testing.
/// 
/// Creates a table with:
/// - `id`: Primary key (auto-increment)
/// - `row_index`: Unique row identifier
/// - `Name`: Text column for test data
/// - `created_at` and `updated_at`: Timestamp columns
/// 
/// Also creates an index on `row_index` for performance.
/// 
/// # Example
/// ```
/// let conn = Connection::open_in_memory().unwrap();
/// setup_simple_table(&conn, "TestTable");
/// ```
pub fn setup_simple_table(conn: &Connection, table: &str) {
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            row_index INTEGER NOT NULL UNIQUE,
            \"Name\" TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        table
    );
    conn.execute(&sql, []).unwrap();
    
    // Create index similar to production
    let index_name = table.replace(" ", "_");
    conn.execute(
        &format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_row_index ON \"{}\"(row_index)",
            index_name, table
        ),
        [],
    )
    .unwrap();
}

/// Set up a metadata table for testing with the given columns.
/// 
/// Creates a `{table}_Metadata` table with all standard metadata fields
/// and inserts rows for each specified column name.
/// 
/// # Arguments
/// * `conn` - Database connection
/// * `table` - Base table name (metadata table will be `{table}_Metadata`)
/// * `cols` - Array of column names to insert
/// 
/// # Example
/// ```
/// let conn = Connection::open_in_memory().unwrap();
/// setup_metadata_table(&conn, "Main", &["Name", "Age", "Email"]);
/// ```
pub fn setup_metadata_table(conn: &Connection, table: &str, cols: &[&str]) {
    let meta = format!("{}_Metadata", table);
    conn.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                column_index INTEGER UNIQUE NOT NULL,
                column_name TEXT UNIQUE NOT NULL,
                data_type TEXT,
                validator_type TEXT,
                validator_config TEXT,
                ai_context TEXT,
                filter_expr TEXT,
                ai_enable_row_generation INTEGER DEFAULT 0,
                ai_include_in_send INTEGER DEFAULT 1,
                deleted INTEGER DEFAULT 0,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            meta
        ),
        [],
    )
    .unwrap();
    
    // Insert metadata rows for each column
    let mut idx = 0i32;
    for c in cols {
        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (column_index, column_name) VALUES (?, ?)",
                meta
            ),
            params![idx, *c],
        )
        .unwrap();
        idx += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_simple_table_creates_table() {
        let conn = Connection::open_in_memory().unwrap();
        setup_simple_table(&conn, "TestTable");
        
        // Verify table exists with expected columns
        let mut stmt = conn.prepare("PRAGMA table_info(\"TestTable\")").unwrap();
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        
        assert!(columns.contains(&"id".to_string()));
        assert!(columns.contains(&"row_index".to_string()));
        assert!(columns.contains(&"Name".to_string()));
    }

    #[test]
    fn test_setup_metadata_table_creates_metadata() {
        let conn = Connection::open_in_memory().unwrap();
        setup_metadata_table(&conn, "Main", &["A", "B", "C"]);
        
        // Verify metadata table exists
        let mut stmt = conn
            .prepare("SELECT column_name FROM \"Main_Metadata\" ORDER BY column_index")
            .unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        
        assert_eq!(cols, vec!["A", "B", "C"]);
    }
}
