// src/sheets/database/checkpoint.rs
//! WAL checkpoint management to prevent data loss
//! 
//! ## The Problem
//! In WAL (Write-Ahead Logging) mode, SQLite writes changes to a separate WAL file
//! and not immediately to the main database. When using `PRAGMA synchronous=NORMAL`,
//! the WAL is not always flushed to the main database file immediately.
//! 
//! This can cause data loss in several scenarios:
//! 1. Application crashes before WAL is checkpointed
//! 2. Improper shutdown (e.g., Windows doesn't flush properly)
//! 3. Power loss before checkpoint happens
//! 4. Force-closing the app during active use
//! 
//! Users experience this as: "I added data a few minutes ago, but after restarting
//! the app, the changes are gone."
//! 
//! ## The Solution
//! This module provides utilities to force WAL checkpoints at critical moments:
//! - On app exit (most critical)
//! - Periodically every 30 seconds (additional safety)
//! - After write-heavy operations (rows, batch operations)
//! 
//! This ensures data is durably written to the main database file.

use super::error::DbResult;
use bevy::prelude::*;
use rusqlite::Connection;
use std::path::Path;

/// Force a WAL checkpoint on a database connection
/// This ensures all pending changes in the WAL file are written to the main database
/// 
/// Note: Only performs checkpoint if database is actually in WAL mode
pub fn checkpoint_database(conn: &Connection) -> DbResult<()> {
    // Check if database is in WAL mode first
    let journal_mode: String = conn.query_row(
        "PRAGMA journal_mode",
        [],
        |row| row.get(0)
    )?;
    
    if journal_mode.to_uppercase() != "WAL" {
        trace!("Database is not in WAL mode (mode: {}), skipping checkpoint", journal_mode);
        return Ok(());
    }
    
    // RESTART mode: Checkpoint and restart the WAL file
    // This ensures maximum durability
    conn.execute_batch("PRAGMA wal_checkpoint(RESTART);")?;
    info!("WAL checkpoint completed");
    Ok(())
}

/// Checkpoint a database file by path
/// Returns Ok(true) if checkpoint was performed, Ok(false) if skipped (no work needed)
pub fn checkpoint_database_file(db_path: &Path) -> DbResult<bool> {
    if !db_path.exists() {
        return Ok(false); // Nothing to checkpoint
    }
    
    // Check if WAL file exists before opening the database
    // WAL file is named as: database.db-wal (appended, not replaced)
    let mut wal_path = db_path.as_os_str().to_os_string();
    wal_path.push("-wal");
    let wal_path = Path::new(&wal_path);
    
    if !wal_path.exists() {
        trace!("No WAL file found for {:?}, skipping checkpoint", db_path.file_name());
        return Ok(false); // No WAL file, nothing to checkpoint
    }
    
    // Check if WAL file has data (size > 0)
    // Empty or very small WAL files don't need checkpointing
    if let Ok(metadata) = std::fs::metadata(&wal_path) {
        let size = metadata.len();
        if size == 0 {
            trace!("WAL file for {:?} is empty (0 bytes), skipping checkpoint", db_path.file_name());
            return Ok(false); // Empty WAL, nothing to checkpoint
        }
        // WAL files smaller than typical header (32 bytes) are likely empty/invalid
        if size < 32 {
            trace!("WAL file for {:?} is too small ({} bytes), skipping checkpoint", db_path.file_name(), size);
            return Ok(false); // Too small, nothing meaningful to checkpoint
        }
    }
    
    let conn = super::connection::DbConnection::open_existing(db_path)?;
    checkpoint_database(&conn)?;
    Ok(true) // Successfully checkpointed
}

/// Checkpoint all database files in the SkylineDB directory
/// Returns the number of databases actually checkpointed (where work was done)
pub fn checkpoint_all_databases() -> DbResult<usize> {
    let base_path = crate::sheets::systems::io::get_default_data_base_path();
    
    if !base_path.exists() {
        return Ok(0);
    }
    
    let entries = std::fs::read_dir(&base_path)?;
    let mut checkpointed_count = 0;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() && path.extension().map_or(false, |ext| ext == "db") {
            match checkpoint_database_file(&path) {
                Ok(true) => {
                    // Checkpoint was actually performed
                    checkpointed_count += 1;
                    info!("Checkpointed database: {:?}", path.file_name());
                }
                Ok(false) => {
                    // Skipped - no WAL file or empty WAL file (normal when idle)
                    trace!("Skipped checkpoint for {:?} (no pending WAL data)", path.file_name());
                }
                Err(e) => {
                    error!("Failed to checkpoint {:?}: {}", path, e);
                }
            }
        }
    }
    
    if checkpointed_count > 0 {
        info!("Successfully checkpointed {} database(s)", checkpointed_count);
    }
    
    Ok(checkpointed_count)
}

/// Bevy system to checkpoint all databases on app exit
pub fn checkpoint_on_exit(app_exit: EventReader<bevy::app::AppExit>) {
    if app_exit.is_empty() {
        return;
    }
    
    info!("App exit detected, checkpointing all databases...");
    
    match checkpoint_all_databases() {
        Ok(count) => {
            if count > 0 {
                info!("Checkpointed {} database(s) successfully before exit", count);
            } else {
                info!("All databases already synchronized before exit (no pending WAL data)");
            }
        }
        Err(e) => error!("Failed to checkpoint databases on exit: {}", e),
    }
}

/// Periodic checkpoint system (runs every 30 seconds)
/// This provides additional safety by periodically flushing WAL to disk
#[derive(Resource)]
pub struct CheckpointTimer {
    pub timer: Timer,
}

impl Default for CheckpointTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(30.0, TimerMode::Repeating),
        }
    }
}

pub fn periodic_checkpoint(
    time: Res<Time>,
    mut timer: ResMut<CheckpointTimer>,
) {
    if timer.timer.tick(time.delta()).just_finished() {
        trace!("Running periodic WAL checkpoint check...");
        
        match checkpoint_all_databases() {
            Ok(count) if count > 0 => {
                info!("Periodic checkpoint: flushed {} database(s)", count);
            }
            Ok(_) => {
                trace!("Periodic checkpoint: all databases already synchronized (no pending WAL data)");
            }
            Err(e) => warn!("Periodic checkpoint failed: {}", e),
        }
    }
}
