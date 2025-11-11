// src/cli/mod.rs
// CLI tools for database maintenance and diagnostics

pub mod repair_metadata;
pub mod diagnose_metadata;
pub mod add_display_name;
pub mod list_columns;
pub mod sync_column_names;
pub mod restore_columns;
pub mod check_structure_columns;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "skylinedb")]
#[command(about = "SkylineDB - Spreadsheet database application with maintenance tools", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Repair corrupted metadata tables (fixes column_index type issues)
    RepairMetadata {
        /// Path to the SkylineDB data directory
        path: PathBuf,
    },
    
    /// Diagnose metadata table issues
    DiagnoseMetadata {
        /// Path to the database file
        path: PathBuf,
    },
    
    /// Add missing display_name column to metadata tables
    AddDisplayName {
        /// Path to the SkylineDB data directory
        path: PathBuf,
    },
    
    /// List all columns in a table's metadata and physical schema
    ListColumns {
        /// Path to the database file
        path: PathBuf,
    },
    
    /// Sync column names between metadata and physical table
    SyncColumnNames {
        /// Path to the database file
        path: PathBuf,
    },
    
    /// Restore missing columns to physical table from metadata
    RestoreColumns {
        /// Path to the database file
        path: PathBuf,
    },
    
    /// Check which columns are Structure validators
    CheckStructureColumns {
        /// Path to the database file (optional, defaults to SkylineDB/Tactical Frontlines.db)
        path: Option<PathBuf>,
    },
}
