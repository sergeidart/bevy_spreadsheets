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
pub fn checkpoint_database(conn: &Connection) -> DbResult<()> {
    // RESTART mode: Checkpoint and restart the WAL file
    // This ensures maximum durability
    conn.execute_batch("PRAGMA wal_checkpoint(RESTART);")?;
    info!("WAL checkpoint completed");
    Ok(())
}

/// Checkpoint a database file by path
pub fn checkpoint_database_file(db_path: &Path) -> DbResult<()> {
    if !db_path.exists() {
        return Ok(()); // Nothing to checkpoint
    }
    
    let conn = Connection::open(db_path)?;
    checkpoint_database(&conn)?;
    Ok(())
}

/// Checkpoint all database files in the SkylineDB directory
pub fn checkpoint_all_databases() -> DbResult<()> {
    let base_path = crate::sheets::systems::io::get_default_data_base_path();
    
    if !base_path.exists() {
        return Ok(());
    }
    
    let entries = std::fs::read_dir(&base_path)?;
    let mut checkpointed_count = 0;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() && path.extension().map_or(false, |ext| ext == "db") {
            match checkpoint_database_file(&path) {
                Ok(()) => {
                    info!("Checkpointed database: {:?}", path.file_name());
                    checkpointed_count += 1;
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
    
    Ok(())
}

/// Bevy system to checkpoint all databases on app exit
pub fn checkpoint_on_exit(app_exit: EventReader<bevy::app::AppExit>) {
    if app_exit.is_empty() {
        return;
    }
    
    info!("App exit detected, checkpointing all databases...");
    
    match checkpoint_all_databases() {
        Ok(()) => info!("All databases checkpointed successfully before exit"),
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
        trace!("Running periodic WAL checkpoint...");
        
        match checkpoint_all_databases() {
            Ok(()) => trace!("Periodic checkpoint completed"),
            Err(e) => warn!("Periodic checkpoint failed: {}", e),
        }
    }
}
