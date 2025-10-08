# SkylineDB Database Migration Implementation

## 📋 Summary

This implementation adds SQLite database support to SkylineDB (formerly bevy_spreadsheets) with:
- Per-topic database files with multiple sheets
- JSON to Database migration tools with dependency detection
- Native foreign key relationships for hierarchical structures
- JSON viewer/fallback mode support (to be activated via UI)
- Full metadata preservation including AI settings and column validators

## 🏗️ Architecture Overview

### Database Structure

**File Organization:**
```
Documents/SkylineDB/
├── TacticalFrontlines.db    # Topic-based database
├── GameLibrary.db
└── SharedReferences.db       # Shared tags/enums
```

**Per-Database Schema:**
- `_Metadata` - Global table with all sheet-level metadata
- `{TableName}` - Main data table with dynamic columns
- `{TableName}_Metadata` - Column definitions and validators
- `{TableName}_Metadata_Groups` - AI schema group membership
- `{TableName}_{StructureColumn}` - Nested structure tables (with FK constraints)

### Key Features

1. **Relational Structure Support**
   - Native parent_id foreign keys for hierarchical data
   - CASCADE deletion for data integrity
   - Separate tables for each structure level

2. **Metadata Preservation**
   - Column validators (Basic, Linked, Structure)
   - AI context and groups
   - Table-level settings (ai_allow_add_rows, etc.)
   - Filter expressions

3. **Migration with Dependency Resolution**
   - Automatic detection of linked sheets
   - Topological sorting to migrate dependencies first
   - Suggestion to include related sheets
   - Preserves all JSON metadata

## 📦 Modules Created

```
src/sheets/database/
├── mod.rs              # Public API and DbConfig
├── error.rs            # DbError types
├── schema.rs           # Table creation SQL
├── connection.rs       # DbConnection wrapper
├── reader.rs           # DbReader for queries
├── writer.rs           # DbWriter for updates
├── migration.rs        # MigrationTools for JSON↔DB
└── systems.rs          # Bevy ECS systems
```

## 🎨 UI Components

### Migration Popup
- **Location:** `src/ui/elements/popups/migration_popup.rs`
- **Features:**
  - Folder picker for JSON source
  - DB selection (existing or create new)
  - Automatic sheet scanning and dependency detection
  - Real-time migration progress
  - Error reporting

### Events Added
```rust
// In src/sheets/events.rs
RequestMigrateJsonToDb     // Trigger migration
MigrationCompleted         // Migration finished callback
RequestExportSheetToJson   // Export DB table to JSON
```

## 🔧 How to Use

### 1. Migration (JSON → Database)

**Via UI (To Be Implemented):**
1. Click "Database" menu → "Migrate JSON to DB"
2. Select folder containing .json + .meta.json pairs
3. Choose target database (existing or create new)
4. Review detected sheets and dependencies
5. Click "Start Migration"

**Programmatically:**
```rust
use crate::sheets::database::MigrationTools;

let report = MigrationTools::migrate_folder_to_db(
    Path::new("path/to/TacticalFrontlines.db"),
    Path::new("path/to/json_folder"),
    true, // create_new_db
)?;

println!("Migrated {} sheets", report.sheets_migrated);
```

### 2. Export (Database → JSON)

```rust
use crate::sheets::database::{MigrationTools, DbConnection};

let conn = DbConnection::create_new(Path::new("path/to/db.db"))?;
MigrationTools::export_sheet_to_json(
    conn.connection()?,
    "Aircraft",  // table name
    Path::new("path/to/output_folder"),
)?;
```

### 3. Reading from Database

```rust
use crate::sheets::database::DbReader;
use rusqlite::Connection;

let conn = Connection::open("TacticalFrontlines.db")?;

// List all sheets
let sheets = DbReader::list_sheets(&conn)?;

// Read specific sheet
let sheet_data = DbReader::read_sheet(&conn, "Aircraft")?;
```

## 🚀 Next Steps to Complete Integration

### 1. UI Integration Points

**Add Migration Button:**
```rust
// In src/ui/elements/top_panel/mod.rs or similar
if ui.button("📁 Migrate JSON").clicked() {
    migration_popup_state.show = true;
}
```

**Add JSON Viewer Mode Toggle:**
```rust
// In settings or top panel
ui.checkbox(&mut editor_state.use_json_fallback_mode, "📄 JSON Viewer Mode");
```

### 2. Storage Mode Resource

Add to `src/sheets/resources.rs`:
```rust
#[derive(Resource, Default)]
pub struct StorageMode {
    pub use_database: bool,  // true = DB mode, false = JSON mode
    pub active_db_path: Option<PathBuf>,
}
```

### 3. Update SheetRegistry

Modify `SheetRegistry` to support both modes:
```rust
impl SheetRegistry {
    pub fn load_from_database(&mut self, db_path: &Path) -> Result<()> {
        let conn = Connection::open(db_path)?;
        let sheets = DbReader::list_sheets(&conn)?;
        
        for sheet_name in sheets {
            let sheet_data = DbReader::read_sheet(&conn, &sheet_name)?;
            self.add_or_replace_sheet(None, sheet_name, sheet_data);
        }
        Ok(())
    }
}
```

### 4. Startup System Modification

```rust
fn detect_and_load_storage(
    mut commands: Commands,
    mut registry: ResMut<SheetRegistry>,
) {
    let skyline_path = DbConfig::default_path();
    
    // Check for existing databases
    if let Ok(entries) = std::fs::read_dir(&skyline_path) {
        let db_files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "db"))
            .collect();
        
        if !db_files.is_empty() {
            // Use database mode
            commands.insert_resource(StorageMode {
                use_database: true,
                active_db_path: Some(db_files[0].path()),
            });
            
            // Load sheets from database
            registry.load_from_database(&db_files[0].path()).ok();
            return;
        }
    }
    
    // Fallback to JSON mode (existing behavior)
    commands.insert_resource(StorageMode {
        use_database: false,
        active_db_path: None,
    });
}
```

### 5. Structure Cell Expansion (Future)

When implementing the "spawn cells button" for structures:

```rust
// In table cell rendering
if matches!(col.validator, Some(ColumnValidator::Structure)) {
    let child_count = get_structure_child_count(conn, parent_id, structure_table);
    
    if ui.button(format!("[{} items] ▼", child_count)).clicked() {
        events.send(OpenStructureView {
            parent_table: "Aircraft",
            parent_id: row_id,
            structure_table: "Aircraft_Pylons",
        });
    }
}
```

## ⚠️ Important Notes

1. **Backward Compatibility:**
   - JSON mode remains fully functional
   - No breaking changes to existing file-based workflows
   - Users can switch between modes

2. **Thread Safety:**
   - SQLite with WAL mode supports multiple readers
   - Single writer per database (enforced by SQLite)
   - Per-topic databases minimize lock contention

3. **Data Integrity:**
   - Foreign key constraints enabled
   - Transactions for atomic operations
   - Cascade deletion for parent-child relationships

4. **Performance:**
   - Lazy loading of grid data
   - Indexed queries on row_index and parent_id
   - In-memory caching via existing SheetRegistry

## 📚 Dependencies Added

```toml
rusqlite = { version = "0.32", features = ["bundled", "serde_json"] }
```

## 🔍 Testing Checklist

- [ ] Create new database from empty folder
- [ ] Migrate existing JSON sheets to database
- [ ] Export database table back to JSON
- [ ] Verify metadata preservation (AI settings, validators, groups)
- [ ] Test linked column dependencies
- [ ] Test structure table creation
- [ ] Verify foreign key constraints work
- [ ] Test concurrent read access
- [ ] Verify fallback to JSON mode works
- [ ] Test migration error handling

## 📝 Documentation TODO

- [ ] Add user guide for migration workflow
- [ ] Document database schema in detail
- [ ] Create troubleshooting guide
- [ ] Add examples for Optima compatibility
- [ ] Document structure table query patterns

---

**Status:** Core implementation complete. UI integration and testing pending.
**Next Priority:** Add migration button to UI and implement storage mode toggle.
