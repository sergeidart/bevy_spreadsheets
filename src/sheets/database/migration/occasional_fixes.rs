// src/sheets/database/migration/occasional_fixes.rs

use bevy::prelude::*;
use rusqlite::Connection;

use super::super::error::{DbError, DbResult};

/// Trait for implementing occasional migration fixes
/// 
/// Each fix should be a struct that implements this trait.
/// Fixes are intended for one-off data corrections or schema adjustments
/// that need to be applied to existing databases.
pub trait MigrationFix {
    /// A unique identifier for this fix
    fn id(&self) -> &str;

    /// A description of what this fix does
    fn description(&self) -> &str;

    /// Apply the fix to the database
    fn apply(&self, conn: &mut Connection, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<()>;

    /// Check if this fix has already been applied
    fn is_applied(&self, conn: &Connection) -> DbResult<bool> {
        // Default implementation checks a migration_fixes table
        let result: Result<i32, rusqlite::Error> = conn.query_row(
            "SELECT COUNT(*) FROM migration_fixes WHERE fix_id = ?1",
            [self.id()],
            |row| row.get(0),
        );

        match result {
            Ok(count) => Ok(count > 0),
            Err(rusqlite::Error::SqliteFailure(_, _)) => {
                // Table might not exist yet
                Ok(false)
            }
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    /// Mark this fix as applied
    fn mark_applied(&self, _conn: &mut Connection, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<()> {
        use super::super::daemon_client::Statement;
        
        // Ensure the migration_fixes table exists
        let create_stmt = Statement {
            sql: "CREATE TABLE IF NOT EXISTS migration_fixes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                fix_id TEXT UNIQUE NOT NULL,
                description TEXT,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )".to_string(),
            params: vec![],
        };
        daemon_client.exec_batch(vec![create_stmt])
            .map_err(|e| DbError::MigrationFailed(format!("Failed to create migration_fixes table: {}", e)))?;

        // Write through daemon (no direct DB writes)
        let insert_stmt = Statement {
            sql: "INSERT OR IGNORE INTO migration_fixes (fix_id, description) VALUES (?, ?)".to_string(),
            params: vec![
                serde_json::Value::String(self.id().to_string()),
                serde_json::Value::String(self.description().to_string()),
            ],
        };
        daemon_client
            .exec_batch(vec![insert_stmt])
            .map_err(|e| DbError::MigrationFailed(format!("Failed to mark migration fix as applied: {}", e)))?;

        Ok(())
    }
}

/// Manager for applying occasional migration fixes
pub struct OccasionalFixManager {
    fixes: Vec<Box<dyn MigrationFix>>,
}

impl OccasionalFixManager {
    pub fn new() -> Self {
        Self { fixes: Vec::new() }
    }

    /// Register a fix to be managed
    pub fn register_fix(&mut self, fix: Box<dyn MigrationFix>) {
        self.fixes.push(fix);
    }

    /// Apply all unapplied fixes to the database
    pub fn apply_all_fixes(&self, conn: &mut Connection, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<Vec<String>> {
        let mut applied = Vec::new();

        info!("Checking {} registered migrations...", self.fixes.len());

        for fix in &self.fixes {
            if !fix.is_applied(conn)? {
                info!("Migration '{}' not yet applied, running: {}", fix.id(), fix.description());

                match fix.apply(conn, daemon_client) {
                    Ok(_) => {
                        fix.mark_applied(conn, daemon_client)?;
                        applied.push(fix.id().to_string());
                        info!("✓ Successfully applied migration: {}", fix.id());
                    }
                    Err(e) => {
                        error!("✗ Failed to apply migration {}: {}", fix.id(), e);
                        return Err(e);
                    }
                }
            } else {
                info!("Migration '{}' already applied, skipping", fix.id());
            }
        }

        if applied.is_empty() {
            info!("All migrations already applied, no action needed");
        }

        Ok(applied)
    }
}

impl Default for OccasionalFixManager {
    fn default() -> Self {
        Self::new()
    }
}

// Example fix implementation (removed to avoid direct write pattern in guards).
