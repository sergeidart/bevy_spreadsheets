// src/sheets/database/validation.rs
// Validation utilities for database integrity checks

use bevy::prelude::*;
use rusqlite::Connection;
use std::collections::HashMap;

use super::error::DbResult;

#[derive(Debug, Clone)]
pub struct RowIndexValidationResult {
    pub table_name: String,
    pub total_rows: i64,
    pub unique_indices: i64,
    pub min_index: Option<i32>,
    pub max_index: Option<i32>,
    pub has_nulls: bool,
    pub null_count: i64,
    pub duplicates: Vec<DuplicateInfo>,
    pub has_issues: bool,
}

#[derive(Debug, Clone)]
pub struct DuplicateInfo {
    pub row_index: i32,
    pub count: i64,
    pub row_ids: Vec<i64>,
}

impl RowIndexValidationResult {
    pub fn is_valid(&self) -> bool {
        !self.has_issues
    }

    pub fn summary(&self) -> String {
        if self.is_valid() {
            format!(
                "✓ '{}': {} rows, sequential indices {}-{}",
                self.table_name,
                self.total_rows,
                self.min_index.unwrap_or(0),
                self.max_index.unwrap_or(0)
            )
        } else {
            let mut issues = Vec::new();
            if self.has_nulls {
                issues.push(format!("{} NULL indices", self.null_count));
            }
            if !self.duplicates.is_empty() {
                issues.push(format!("{} duplicate values", self.duplicates.len()));
            }
            if self.unique_indices < self.total_rows {
                let missing = self.total_rows - self.unique_indices;
                issues.push(format!("{} non-unique", missing));
            }
            format!(
                "⚠ '{}': {} rows, {} unique indices, issues: {}",
                self.table_name,
                self.total_rows,
                self.unique_indices,
                issues.join(", ")
            )
        }
    }
}

/// Validate row_index integrity for a single table
pub fn validate_table_row_index(
    conn: &Connection,
    table_name: &str,
) -> DbResult<RowIndexValidationResult> {
    // Check if table has row_index column
    let has_row_index: bool = conn
        .prepare(&format!(
            "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = 'row_index'",
            table_name
        ))?
        .query_row([], |row| {
            let count: i32 = row.get(0)?;
            Ok(count > 0)
        })?;

    if !has_row_index {
        return Ok(RowIndexValidationResult {
            table_name: table_name.to_string(),
            total_rows: 0,
            unique_indices: 0,
            min_index: None,
            max_index: None,
            has_nulls: false,
            null_count: 0,
            duplicates: Vec::new(),
            has_issues: false,
        });
    }

    // Get basic stats
    let (total_rows, unique_indices, min_index, max_index, null_count): (i64, i64, Option<i32>, Option<i32>, i64) = 
        conn.query_row(
            &format!(
                "SELECT 
                    COUNT(*) as total,
                    COUNT(DISTINCT row_index) as unique_count,
                    MIN(row_index) as min_idx,
                    MAX(row_index) as max_idx,
                    SUM(CASE WHEN row_index IS NULL THEN 1 ELSE 0 END) as null_count
                FROM \"{}\"",
                table_name
            ),
            [],
            |row| Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2).ok(),
                row.get(3).ok(),
                row.get(4)?
            ))
        )?;

    let has_nulls = null_count > 0;

    // Find duplicates
    let mut duplicates = Vec::new();
    let mut stmt = conn.prepare(&format!(
        "SELECT row_index, COUNT(*) as cnt, GROUP_CONCAT(id) as ids
         FROM \"{}\"
         WHERE row_index IS NOT NULL
         GROUP BY row_index
         HAVING cnt > 1
         ORDER BY cnt DESC
         LIMIT 100",
        table_name
    ))?;

    let dup_rows = stmt.query_map([], |row| {
        let row_index: i32 = row.get(0)?;
        let count: i64 = row.get(1)?;
        let ids_str: String = row.get(2)?;
        let row_ids: Vec<i64> = ids_str
            .split(',')
            .filter_map(|s| s.parse::<i64>().ok())
            .collect();
        Ok(DuplicateInfo {
            row_index,
            count,
            row_ids,
        })
    })?;

    for dup in dup_rows {
        duplicates.push(dup?);
    }

    let has_issues = has_nulls || !duplicates.is_empty() || (unique_indices < total_rows);

    Ok(RowIndexValidationResult {
        table_name: table_name.to_string(),
        total_rows,
        unique_indices,
        min_index,
        max_index,
        has_nulls,
        null_count,
        duplicates,
        has_issues,
    })
}

/// Validate row_index integrity for all tables in a database
pub fn validate_all_tables(conn: &Connection) -> DbResult<Vec<RowIndexValidationResult>> {
    // Get all table names from GlobalMetadata
    let mut stmt = conn.prepare(
        "SELECT table_name FROM GlobalMetadata ORDER BY display_order"
    )?;
    
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut results = Vec::new();
    for table_name in tables {
        let result = validate_table_row_index(conn, &table_name)?;
        results.push(result);
    }

    Ok(results)
}

/// Print validation report to log
pub fn log_validation_report(results: &[RowIndexValidationResult]) {
    info!("========================================");
    info!("row_index Validation Report");
    info!("========================================");
    
    let mut total_tables = 0;
    let mut valid_tables = 0;
    let mut tables_with_issues = 0;
    let mut total_duplicates = 0;
    let mut total_nulls = 0;

    for result in results {
        if result.total_rows == 0 {
            continue; // Skip empty tables
        }
        
        total_tables += 1;
        
        if result.is_valid() {
            valid_tables += 1;
            debug!("{}", result.summary());
        } else {
            tables_with_issues += 1;
            warn!("{}", result.summary());
            
            total_nulls += result.null_count;
            total_duplicates += result.duplicates.len() as i64;
            
            // Log first few duplicates
            for dup in result.duplicates.iter().take(5) {
                warn!(
                    "  Duplicate: row_index={} appears {} times (row IDs: {:?})",
                    dup.row_index,
                    dup.count,
                    &dup.row_ids[..dup.row_ids.len().min(10)]
                );
            }
            if result.duplicates.len() > 5 {
                warn!("  ... and {} more duplicates", result.duplicates.len() - 5);
            }
        }
    }

    info!("========================================");
    info!("Summary:");
    info!("  Total tables: {}", total_tables);
    info!("  Valid tables: {}", valid_tables);
    info!("  Tables with issues: {}", tables_with_issues);
    if tables_with_issues > 0 {
        info!("  Total NULL row_index values: {}", total_nulls);
        info!("  Total duplicate row_index values: {}", total_duplicates);
    }
    info!("========================================");

    if tables_with_issues == 0 {
        info!("✓ All tables have valid row_index values!");
    } else {
        warn!("⚠ {} tables have row_index issues that need fixing!", tables_with_issues);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation() {
        let conn = Connection::open_in_memory().unwrap();
        
        // Create test table
        conn.execute(
            "CREATE TABLE test_table (
                id INTEGER PRIMARY KEY,
                row_index INTEGER,
                data TEXT
            )",
            [],
        ).unwrap();

        // Insert test data with duplicates
        conn.execute("INSERT INTO test_table (id, row_index, data) VALUES (1, 0, 'a')", []).unwrap();
        conn.execute("INSERT INTO test_table (id, row_index, data) VALUES (2, 1, 'b')", []).unwrap();
        conn.execute("INSERT INTO test_table (id, row_index, data) VALUES (3, 1, 'c')", []).unwrap();
        conn.execute("INSERT INTO test_table (id, row_index, data) VALUES (4, 2, 'd')", []).unwrap();
        conn.execute("INSERT INTO test_table (id, row_index, data) VALUES (5, NULL, 'e')", []).unwrap();

        let result = validate_table_row_index(&conn, "test_table").unwrap();
        
        assert_eq!(result.total_rows, 5);
        assert_eq!(result.unique_indices, 3); // 0, 1, 2
        assert_eq!(result.null_count, 1);
        assert_eq!(result.duplicates.len(), 1); // row_index=1 appears twice
        assert!(result.has_issues);
        
        assert_eq!(result.duplicates[0].row_index, 1);
        assert_eq!(result.duplicates[0].count, 2);
    }
}
