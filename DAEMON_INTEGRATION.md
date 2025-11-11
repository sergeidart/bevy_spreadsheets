# SQLite Daemon Integration - Implementation Guide

## Overview
We've successfully integrated the SQLite daemon architecture to solve single-writer concurrency issues. The daemon serializes all write operations while allowing direct read access for maximum performance.

## What's Been Implemented

### 1. Core Daemon Infrastructure

#### `daemon_client.rs` - Protocol Implementation
- **Length-prefixed JSON protocol** over Windows Named Pipes (`\\.\pipe\SkylineDBd-v1`)
- **Request/Response types** for ExecBatch, Ping, Shutdown
- **Auto-retry logic** with daemon auto-start on connection failure
- **Platform-specific** pipe handling (Windows/Unix)

Key functions:
```rust
pub fn exec(&self, sql: String, params: Vec<serde_json::Value>) -> Result<DaemonResponse, String>
pub fn exec_batch(&self, statements: Vec<Statement>) -> Result<DaemonResponse, String>
pub fn ping(&self) -> bool
```

#### `daemon_manager.rs` - Lifecycle Management  
- **Auto-download** from GitHub releases: https://github.com/sergeidart/sqlite_daemon/releases/tag/V1.2
- **Installation** to `Documents\SkylineDB\skylinedb-daemon.exe`
- **Process spawning** with CREATE_NO_WINDOW flag
- **Health checks** to verify daemon status

Key functions:
```rust
pub async fn ensure_daemon_installed() -> Result<PathBuf, String>
pub fn start_daemon(db_path: &Path) -> Result<std::process::Child, String>
pub fn is_daemon_running() -> bool
```

#### `daemon_resource.rs` - Bevy Resource
- **Shared DaemonClient** instance available across all systems
- Initialized as `SharedDaemonClient` resource in plugin

#### `daemon_init.rs` - Startup Systems
- **`ensure_daemon_ready`** - Checks daemon status on startup
- **`initiate_daemon_download_if_needed`** - Downloads in background if missing
- **`check_daemon_health`** - Periodic warnings if daemon is down (every 30s)
- **`DaemonState`** resource tracks installation/running status

### 2. Integration Points

#### Plugin Registration
- Daemon client resource initialized in `SheetsPlugin`
- Startup chain ensures daemon readiness before database operations:
  1. `ensure_daemon_ready` - Check/prepare daemon
  2. `initiate_daemon_download_if_needed` - Download if needed
  3. Regular startup sequence (scan DBs, load sheets, etc.)
- Health check system runs in `FileOperations` system set

#### Dependencies Added
```toml
base64 = "0.22"
reqwest = { version = "0.12", features = ["blocking"] }

[target.'cfg(windows)'.dependencies]
winapi = { version = "0.3", features = ["winbase", "fileapi", "handleapi", "namedpipeapi"] }
```

## Migration Pattern: Converting Write Operations

### Current Pattern (Direct SQLite)
```rust
// OLD: Direct connection write
let conn = DbConnection::open_existing(&db_path)?;
conn.execute(
    &format!("UPDATE \"{}\" SET \"{}\" = ? WHERE id = ?", table, column),
    rusqlite::params![value, row_id],
)?;
```

### New Pattern (Daemon-Mediated)
```rust
// NEW: Write through daemon
use crate::sheets::database::daemon_client::Statement;

let client = daemon_client.client(); // From SharedDaemonClient resource

let stmt = Statement {
    sql: format!("UPDATE \"{}\" SET \"{}\" = ? WHERE id = ?", table, column),
    params: vec![
        serde_json::Value::String(value.to_string()),
        serde_json::Value::Number(row_id.into()),
    ],
};

client.exec_batch(vec![stmt])?;
```

### Read Operations (UNCHANGED)
```rust
// Reads stay direct for performance
let conn = DbConnection::open_existing(&db_path)?;
let row_id: i64 = conn.query_row(
    &format!("SELECT id FROM \"{}\" WHERE ...", table),
    [],
    |row| row.get(0),
)?;
```

## Files to Migrate

All write operations need conversion. Here's the complete list:

### High Priority (Core Data Modifications)
1. **src/sheets/systems/logic/update_cell/db_persistence.rs**
   - `persist_structure_cell_update()` - Line ~25: `DbWriter::update_structure_cell_by_id`
   - `persist_regular_cell_update()` - Line ~63: `conn.execute(UPDATE ...)`

2. **src/sheets/systems/logic/add_row/db_persistence.rs**
   - `persist_row_to_db()` - Multiple `conn.execute(INSERT ...)`
   - `persist_rows_batch_to_db()` - Batch inserts
   - `update_toggle_ai_generation_db()` - AI settings updates

3. **src/sheets/systems/logic/delete_rows.rs**
   - `system_delete_rows()` - DELETE operations

4. **src/sheets/systems/logic/add_column.rs**
   - `handle_add_column_request()` - ALTER TABLE ADD COLUMN

5. **src/sheets/systems/logic/delete_columns.rs**
   - `handle_delete_columns_request()` - Multiple ALTER TABLE operations

6. **src/sheets/systems/logic/update_column_name.rs**
   - Column rename operations (2 locations)

### Medium Priority (Metadata Operations)
7. **src/sheets/database/writer/mod.rs**
   - All `DbWriter` methods that perform writes
   - `update_structure_cell_by_id()`
   - `update_metadata_column_name()`
   - `rename_structure_table()`

8. **src/sheets/database/schema/mod.rs**
   - Schema migration operations
   - `ensure_metadata_schema()`
   - Table creation/modification

9. **src/sheets/database/migration/**
   - All migration write operations
   - JSON-to-DB import
   - Structure migrations

### Low Priority (Infrequent Operations)
10. **src/sheets/systems/io/**
    - Sheet creation/deletion
    - Category management
    - File operations

11. **src/sheets/database/validation.rs**
    - Validator updates

## Helper Functions

### Parameter Conversion
```rust
use crate::sheets::database::daemon_client::param_to_json;

// Convert rusqlite::types::Value to JSON
let json_param = param_to_json(&rusqlite_value);

// Convert strings
let params = vec![
    serde_json::Value::String(some_string),
    serde_json::Value::Number(some_i64.into()),
    serde_json::Value::Null,
];
```

### Batch Operations (Recommended)
```rust
// Group multiple statements into one atomic transaction
let statements = vec![
    Statement {
        sql: "UPDATE table1 SET col = ? WHERE id = ?".to_string(),
        params: vec![json!(value1), json!(id1)],
    },
    Statement {
        sql: "INSERT INTO table2 (col) VALUES (?)".to_string(),
        params: vec![json!(value2)],
    },
];

client.exec_batch(statements)?; // All or nothing
```

## Error Handling

### Daemon Not Running
The system will:
1. Log warnings every 30 seconds
2. Attempt auto-start on first write
3. Return error with clear message if daemon unavailable

User impact:
```
⚠️  SQLite daemon is not running. Database writes may fail!
   This means your changes will NOT be saved until the daemon starts.
   Please check that skylinedb-daemon.exe is present in your SkylineDB folder.
```

### Download Failure
If auto-download fails:
- Background task logs error
- System continues with direct writes (may hit SQLITE_BUSY)
- User can manually place daemon executable

## Testing Strategy

### Phase 1: Verify Daemon Works
1. Run app, check logs for "SQLite daemon is already running" or auto-start
2. Manually test: `skylinedb-cli.exe ping` should return "pong"
3. Verify `skylinedb-daemon.exe` exists in `Documents\SkylineDB`

### Phase 2: Migrate One Operation
Start with **cell updates** (most common):
1. Convert `persist_regular_cell_update()` to use daemon
2. Test: Edit cells, verify saves persist across app restarts
3. Monitor logs for any connection errors

### Phase 3: Systematic Migration
1. Convert all operations in one file at a time
2. Test each file's operations individually
3. Run full regression tests

### Phase 4: Remove Direct Writes
1. Search for remaining `conn.execute(` patterns that modify data
2. Convert any missed operations
3. Consider adding a compile-time check or wrapper

## Performance Considerations

- **Reads**: No change - stay direct (1-10ms)
- **Single writes**: Add 5-20ms IPC overhead
- **Batch writes**: Amortize overhead (200-500 batches/sec possible)
- **Recommendation**: Batch operations when possible

## Next Steps

1. **Start Migration**: Begin with `update_cell/db_persistence.rs`
2. **Test Incrementally**: Verify each file before moving to next
3. **Monitor Logs**: Watch for daemon health warnings
4. **Optimize Batching**: Group related operations where possible
5. **Document Issues**: Track any SQLITE_BUSY errors (should be zero with daemon)

## Architecture Diagram

```
┌─────────────────┐
│  Bevy Systems   │
│  (Your Code)    │
└────────┬────────┘
         │
         │ Reads (Direct, Fast)
         ├──────────────────────────────┐
         │                              │
         │ Writes (Daemon)              │
         ▼                              ▼
┌──────────────────┐            ┌─────────────┐
│  Daemon Client   │            │  rusqlite   │
│   (IPC Layer)    │            │   (READ)    │
└────────┬─────────┘            └──────┬──────┘
         │                             │
         │ Named Pipe                  │
         │ \\.\pipe\SkylineDBd-v1      │
         ▼                             │
┌──────────────────┐                  │
│ skylinedb-daemon │                  │
│  (Actor/Queue)   │                  │
└────────┬─────────┘                  │
         │                            │
         │ Serialized Writes          │
         └──────────┬─────────────────┘
                    ▼
            ┌──────────────┐
            │   SQLite DB  │
            │  (WAL Mode)  │
            └──────────────┘
```

## FAQ

**Q: What if I forget to convert a write operation?**
A: It will use direct SQLite write, potentially causing SQLITE_BUSY errors under concurrent access. Test thoroughly.

**Q: Can I mix direct and daemon writes?**
A: Technically yes, but defeats the purpose. ALL writes should go through daemon for proper serialization.

**Q: What about transactions?**
A: Use `exec_batch` with `TransactionMode::Atomic` - the daemon handles BEGIN/COMMIT automatically.

**Q: Does the daemon auto-start?**
A: Yes, on first write attempt. But startup systems check and can pre-start it.

**Q: What if daemon crashes?**
A: App will attempt to restart it. If that fails, warnings appear and writes fail with clear errors.

---

## Current Status: ✅ Infrastructure Complete, Ready for Migration

All foundational code is in place. The app compiles successfully. Next step is systematic conversion of write operations starting with the most common ones (cell updates).
