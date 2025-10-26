DB Rename Flow — Implementation Plan

Goals
- Perform DB-first operations; only update in-memory if DB succeeds.
- On DB failure, leave in-memory unchanged and emit clear feedback.
- When renaming a Structure validator column, also rename the structure table.
- Support safe metadata-only renames when physical columns don’t exist.

Scope (files/functions)
- `src/sheets/systems/logic/update_column_name.rs` — main handler to reorder operations to DB-first and control feedback/events.
- `src/sheets/database/writer/renames.rs` — ensure renames handle metadata-only cases and structure table rename safety.
- `src/sheets/database/writer/mod.rs` — optionally expose transactional variants for grouped operations.

Behavior Changes
1) Column rename handler (DB-first)
   - Validate input early (empty/invalid/duplicate).
   - Resolve DB mode: if `metadata.category.is_some()`, treat as DB-backed; otherwise JSON mode unchanged.
   - Compute context flags without mutating state:
     - `was_structure_before` = column validator was `Structure`.
     - `is_structure_sheet_now` = registry `is_structure_table(category, sheet_name)`.
   - DB-first apply (DB mode):
     - Open SQLite connection for category DB file. On failure: feedback error, return.
     - Execute DB rename path (see 2). If error: feedback error, return; do NOT mutate in-memory.
     - On success: mutate in-memory `metadata.columns[col_index].header = new_name` and proceed with save/event.
   - Emit `SheetOperationFeedback` only after DB success; otherwise emit failure with clear reason.
   - Emit `SheetDataModifiedInRegistryEvent` only on success.
   - Only enqueue save (`save_single_sheet`) when success.

2) DB rename path selection
   - If `is_structure_sheet_now`:
     - Rename data column in the structure table: `DbWriter::rename_data_column(conn, sheet_name, old_name, new_name)`.
       • This function already performs metadata-only update when the physical column is absent.
   - Else (renaming a column in a parent/main table):
     - If `was_structure_before` (Structure validator):
       • First rename the structure table: `DbWriter::rename_structure_table(conn, sheet_name, old_name, new_name)`.
         – Plan change to make this a no-op (Ok) if the old structure table doesn’t exist.
       • Then update the parent table’s metadata column name: `DbWriter::update_metadata_column_name(conn, sheet_name, col_index, new_name)`.
     - Else (regular data column):
       • Rename data column in the main table: `DbWriter::rename_data_column(conn, sheet_name, old_name, new_name)`.
         – This already falls back to metadata-only if the physical column is missing.

3) Transactional grouping (optional but recommended)
   - Group multi-step structure renames into a single transaction to avoid partial success:
     - Begin transaction; run `rename_structure_table` then `update_metadata_column_name`; commit on success, rollback on error.
   - Implementation approach:
     - Introduce a small trait (e.g., `SqlConn`) with `execute`, `prepare`, `query_row` used by renames.
     - Implement `SqlConn` for both `rusqlite::Connection` and `rusqlite::Transaction<'_>`.
     - Update `renames.rs` functions to be generic over `impl SqlConn`, enabling both connection and transaction usage without duplication.
     - Add `*_tx` convenience wrappers in `DbWriter` if keeping current signatures for external callers.
   - Phase plan: start with DB-first without transaction to unblock correctness; follow up with transactional refactor.

4) Safe metadata-only handling
   - `rename_data_column` already detects missing physical column and updates only metadata. Keep as-is.
   - Harden `rename_structure_table` to check if old structure table exists before ALTER:
     - If not found in `sqlite_master`, return `Ok(())` (no-op) and let caller continue with metadata-only rename.
     - If metadata table exists but data table doesn’t, still attempt to rename metadata table or treat as no-op. Log a warning.

5) Feedback and logging
   - Success: “Renamed column N ‘old’ -> ‘new’ in DB and memory.”
   - Failure (DB mode): “Rename failed in DB for ‘old’ -> ‘new’: <reason>. No changes applied in memory.”
   - Failure (open DB): “Cannot open database for category ‘X’: <io/sql error>.”
   - Structure rename missing tables: Log `info` that physical tables are absent and proceeding with metadata-only update.

6) Edge cases
   - Conflicting name in metadata at a different index: rely on existing checks in `renames.rs` that delete deleted-conflict rows or error on active conflict.
   - Case-insensitive conflicts: maintain current case-insensitive checks in both handler and DB writer.
   - No-op rename (same name) is blocked by validation.

Code-Level To-Dos
- `src/sheets/systems/logic/update_column_name.rs`
  - Move the in-memory header change after a successful DB call.
  - Gate `success = true`, event emission, and save behind DB success in DB mode.
  - Preserve current JSON mode behavior.
- `src/sheets/database/writer/renames.rs`
  - Add existence check in `rename_structure_table`:
    • Query `sqlite_master` for both data and metadata tables; no-op if absent, with logs.
  - (Optional) Extract shared helpers for `PRAGMA table_info` and conflict checks under a trait to enable transactional reuse.
- `src/sheets/database/writer/mod.rs`
  - (Optional) Add transactional wrappers: `rename_structure_table_tx`, `update_metadata_column_name_tx`, `rename_data_column_tx`.

Validation Plan
- Manual test matrix (DB mode):
  1) Regular column rename where column physically exists.
  2) Regular column rename where column is missing physically (metadata-only path).
  3) Structure validator column rename where structure tables exist.
  4) Structure validator column rename where structure tables do not exist.
  5) Attempt rename to a duplicate active name (expect failure; memory unchanged).
- Manual test (JSON mode): ensure current behavior unchanged.
- Logs should clearly show DB-first attempt and outcome.

Rollout
- Implement `update_column_name.rs` reordering first with clear feedback.
- Add `rename_structure_table` existence check.
- Optionally follow up with transactional refactor if needed.

Notes
- Current code already calls `rename_structure_table` for structure validator renames and falls back to metadata-only in `rename_data_column`; the primary change is enforcing DB-first and gating in-memory updates/events on DB success.
