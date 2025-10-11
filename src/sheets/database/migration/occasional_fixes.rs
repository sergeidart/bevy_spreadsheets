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
    fn apply(&self, conn: &mut Connection) -> DbResult<()>;

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
    fn mark_applied(&self, conn: &mut Connection) -> DbResult<()> {
        // Ensure the migration_fixes table exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS migration_fixes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                fix_id TEXT UNIQUE NOT NULL,
                description TEXT,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        conn.execute(
            "INSERT OR IGNORE INTO migration_fixes (fix_id, description) VALUES (?1, ?2)",
            [self.id(), self.description()],
        )?;

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
    pub fn apply_all_fixes(&self, conn: &mut Connection) -> DbResult<Vec<String>> {
        let mut applied = Vec::new();

        for fix in &self.fixes {
            if !fix.is_applied(conn)? {
                info!("Applying migration fix: {} - {}", fix.id(), fix.description());
                
                match fix.apply(conn) {
                    Ok(_) => {
                        fix.mark_applied(conn)?;
                        applied.push(fix.id().to_string());
                        info!("Successfully applied fix: {}", fix.id());
                    }
                    Err(e) => {
                        error!("Failed to apply fix {}: {}", fix.id(), e);
                        return Err(e);
                    }
                }
            }
        }

        Ok(applied)
    }

    /// Apply a specific fix by ID
    pub fn apply_fix_by_id(&self, conn: &mut Connection, fix_id: &str) -> DbResult<bool> {
        for fix in &self.fixes {
            if fix.id() == fix_id {
                if fix.is_applied(conn)? {
                    info!("Fix {} already applied", fix_id);
                    return Ok(false);
                }

                info!("Applying migration fix: {} - {}", fix.id(), fix.description());
                fix.apply(conn)?;
                fix.mark_applied(conn)?;
                info!("Successfully applied fix: {}", fix.id());
                return Ok(true);
            }
        }

        Err(DbError::MigrationFailed(format!("Fix not found: {}", fix_id)))
    }

    /// List all registered fixes and their status
    pub fn list_fixes(&self, conn: &Connection) -> DbResult<Vec<(String, String, bool)>> {
        let mut result = Vec::new();

        for fix in &self.fixes {
            let applied = fix.is_applied(conn)?;
            result.push((
                fix.id().to_string(),
                fix.description().to_string(),
                applied,
            ));
        }

        Ok(result)
    }
}

impl Default for OccasionalFixManager {
    fn default() -> Self {
        Self::new()
    }
}

// Example fix implementation (commented out for reference)
/*
pub struct ExampleArrayOfPrimitivesFix;

impl MigrationFix for ExampleArrayOfPrimitivesFix {
    fn id(&self) -> &str {
        "fix_array_of_primitives_2024_10"
    }

    fn description(&self) -> &str {
        "Fix array-of-primitives handling in structure columns"
    }

    fn apply(&self, conn: &mut Connection) -> DbResult<()> {
        // Implementation of the fix
        info!("Applying array-of-primitives fix...");
        
        // Example: update specific tables or data
        conn.execute(
            "UPDATE some_table SET some_column = ? WHERE condition",
            [],
        )?;

        Ok(())
    }
}
*/
