// src/sheets/database/daemon_protocol.rs
//! Protocol types for SkylineDB daemon communication
//!
//! This module defines the wire protocol for IPC between clients and the daemon.
//! All types use serde for JSON serialization over the pipe/socket connection.

use serde::{Deserialize, Serialize};

/// Protocol version for daemon communication
/// Note: Currently unused as the daemon uses a request counter in the 'rev' field instead
#[allow(dead_code)]
pub const PROTOCOL_VERSION: u64 = 1;

/// Request sent to daemon
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum DaemonRequest {
    /// Execute a batch of SQL statements atomically
    ExecBatch {
        db: String,
        stmts: Vec<Statement>,
        tx: TransactionMode,
    },
    /// Prepare database for maintenance (checkpoint WAL)
    PrepareForMaintenance {
        db: String,
    },
    /// Close database (release file locks for replacement)
    CloseDatabase {
        db: String,
    },
    /// Reopen database after maintenance
    ReopenDatabase {
        db: String,
    },
    /// Check if daemon is alive
    Ping {
        #[serde(skip_serializing_if = "Option::is_none")]
        db: Option<String>,
    },
    /// Gracefully shutdown the daemon process
    /// WARNING: This stops the daemon for ALL clients
    Shutdown,
    /// Disconnect this client (daemon continues running)
    Disconnect,
}

/// Single SQL statement with parameters
#[derive(Debug, Serialize)]
pub struct Statement {
    pub sql: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<serde_json::Value>,
}

/// Transaction mode for batch execution
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionMode {
    /// All statements in one atomic transaction (recommended)
    Atomic,
}

/// Response from daemon
#[derive(Debug, Deserialize)]
pub struct DaemonResponse {
    pub status: String,
    /// Request counter (increments with each request), not a protocol version
    #[serde(default)]
    #[allow(dead_code)]
    pub rev: Option<u64>,
    #[serde(default)]
    pub rows_affected: Option<usize>,
    /// Error message (standard field name per README)
    #[serde(default)]
    pub error: Option<String>,
    /// Error message (alternative field name used by actual daemon)
    #[serde(default)]
    pub message: Option<String>,
    /// Error code (used by actual daemon for SQL errors)
    #[serde(default)]
    #[allow(dead_code)]
    pub code: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // Used by daemon protocol, may be checked in future
    pub checkpointed: Option<bool>,
    #[serde(default)]
    #[allow(dead_code)] // Used by daemon protocol, may be checked in future
    pub closed: Option<bool>,
    #[serde(default)]
    #[allow(dead_code)] // Used by daemon protocol, may be checked in future
    pub reopened: Option<bool>,
}
