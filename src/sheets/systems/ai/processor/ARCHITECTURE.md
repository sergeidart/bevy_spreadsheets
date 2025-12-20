# Complete Unified Architecture Plan

## Location

`src/sheets/systems/ai/processor/`

---

## System Components

### 1. Navigator (navigator.rs) ~180 lines

**Purpose:** Index management - the CRITICAL piece for fixing the current bug

**Responsibilities:**
- Track original row indexes (from grid/DB)
- Assign MAX+1 indexes to AI-added rows
- Persist index mappings across steps
- Must be called BEFORE Storager to feed indexes

**Key Structures:**
```rust
struct StableRowId {
    table_name: String,
    category: Option<String>,
    stable_index: usize,        // DB row_index OR assigned AI index
    display_value: String,       // Human-readable (e.g., "MiG-25PD")
    origin: RowOrigin,          // Original | AiAdded
    parent_stable_index: Option<usize>,
}

struct IndexMapper {
    rows: HashMap<TableRowKey, StableRowId>,
    table_trackers: HashMap<(String, Option<String>), TableIndexTracker>,
    display_value_index: HashMap<(String, Option<String>, String), usize>,
}
```

**Flow:**
1. Pre-processor calls `register_original_rows()` → assigns stable IDs
2. After AI response, Navigator calls `register_ai_added_rows()` → assigns MAX+1 indexes
3. Storager receives these indexes and uses them for storage
4. Next step reads from Navigator to resolve display values

---

### 2. Pre-Processor (pre_processor.rs) ~250 lines

**Purpose:** Prepare data BEFORE AI call

**Responsibilities:**
- Read original rows from grid/DB
- Extract display values (human-readable names) for key columns
- Register indexes with Navigator
- Build parent lineage context
- Handle structure vs root table differences

**Key Functions:**
```rust
fn prepare_batch(
    config: PreProcessConfig,
    grid: &[Vec<String>],
    row_indices: &[i64],
    selected_rows: &[usize],
    navigator: &mut IndexMapper,
) -> PreparedBatch
```

**Display Resolution Logic:**
- Check if row is AI-added → get from Navigator
- Otherwise → read from grid at `key_col_index`
- `key_col_index`: column 2 for structure tables, column 1 for root tables

---

### 3. Storager (storager.rs) ~200 lines

**Purpose:** Persist AI results with proper index mapping

**Responsibilities:**
- Store parsed AI responses
- Link to StableRowIds from Navigator
- Support multi-step accumulation
- Clear on cancel/complete

**Key Structures:**
```rust
struct StoredRowResult {
    stable_id: StableRowId,     // From Navigator
    columns: HashMap<usize, ColumnResult>,
    category: RowCategory,      // Original | AiAdded | Lost
    structure_path: Vec<usize>,
    parent_valid: bool,
}

struct ResultStorage {
    results: HashMap<TableKey, Vec<StoredRowResult>>,
    current_step: usize,
    generation_id: u64,
}
```

---

### 4. Parser (parser.rs) ~150 lines

**Purpose:** Parse AI JSON responses

**Responsibilities:**
- Parse Gemini JSON response
- Extract row data with column values
- Identify which rows are AI-added vs original
- Report parsing errors

**Key Function:**
```rust
fn parse(
    raw_json: &str,
    sent_display_values: &HashSet<String>,
) -> ParseResult
```

**Row Categorization:**
- **Original:** First column value found in `original_first_col_values`
- **AI Added:** First column value NOT found OR explicitly marked
- **Lost:** Was in original set, not in response → skip, no change

---

### 5. Genealogist (genealogist.rs) ~180 lines

**Purpose:** Build ancestry/lineage for structure tables

**Responsibilities:**
- Walk parent chain using existing `walk_parent_lineage()`
- Build context strings for AI prompts
- Use Navigator for display values (not raw indexes)

**Key Function:**
```rust
fn build_lineage_from_navigator(
    stable_id: &StableRowId,
    navigator: &IndexMapper,
) -> Lineage
```

---

### 6. Messenger (messenger.rs) ~180 lines

**Purpose:** Send requests to AI, receive responses

**Responsibilities:**
- Build AI prompt from prepared context
- Call Python/Gemini bridge
- Return raw response
- Handle timeouts/errors

**Key Function:**
```rust
fn build_payload(
    config: &RequestConfig,
    batch: &PreparedBatch,
    lineage: Option<&Lineage>,
) -> Result<String, String>
```

---

### 7. Director (director.rs) ~300 lines

**Purpose:** Orchestrate the complete flow

**Responsibilities:**
- Manage step progression
- Coordinate all components
- Track progress (current step, remaining)
- Emit events for UI updates
- Handle cancel/complete

**Flow Per Step:**
```
1. Pre-Processor: Prepare data, register original indexes
2. Genealogist: Build ancestry (structure tables only)
3. Messenger: Send AI request
4. Parser: Parse response, categorize rows
5. Navigator: Assign indexes to AI-added rows
6. Storager: Persist results with stable IDs
7. If more children → queue next step, goto 1
8. If done → emit ReviewReady
```

**Key Structures:**
```rust
struct Director {
    job_queue: VecDeque<PendingJob>,
    state: ProcessingState,
    navigator: IndexMapper,
    storage: ResultStorage,
    pre_processor: PreProcessor,
    genealogist: Genealogist,
    messenger: Messenger,
}

struct ProcessingState {
    current_step: usize,
    total_steps: usize,
    status: ProcessingStatus,
    generation_id: u64,
}
```

---

### 8. Reviewer (reviewer.rs) ~350 lines

**Purpose:** Bridge to existing UI (Legacy Support)

**Responsibilities:**
- The backend (Director/Storager) produces rich data including parent validation.
- The frontend (UI) is kept separate and visualizes the data.
- This module (or `integration.rs`) acts as a bridge, mapping `StoredRowResult` to the existing `NewRowReview` structures used by the UI.
- **Design Choice:** Backend prepares cached info; UI visualizes it. UI sends events to modify state, but logic remains in backend.

**Parent Validation Mapping:**
- `StoredRowResult.parent_valid` → Mapped to UI state (e.g. `ancestor_dropdown_cache` population)
- `StoredRowResult.parent_suggestions` → Populates dropdowns for invalid parents
- `StoredRowResult.columns[1]` (Parent Key) → `NewRowReview.ancestor_key_values`

**Invalid Parent Handling (UI Side):**
- The existing UI detects populated dropdown cache or invalid values.
- Users use the existing dropdown picker to fix parents.

---

### 9. mod.rs (mod.rs) ~50 lines

**Purpose:** Module exports and re-exports

```rust
pub mod navigator;
pub mod storager;
pub mod parser;
pub mod pre_processor;
pub mod genealogist;
pub mod messenger;
pub mod director;
pub mod reviewer;
pub mod integration;

// Re-exports for external access
pub use integration::{
    DirectorSession,
    start_director_session_v2,
    poll_director_results,
    cancel_director_session,
};
```

---

## File Structure Summary

```
src/sheets/systems/ai/processor/
├── ARCHITECTURE.md     # This document
├── mod.rs              # Module exports (~50 lines)
├── navigator.rs        # Index management (~180 lines)
├── pre_processor.rs    # Data preparation (~250 lines)
├── storager.rs         # Result persistence (~200 lines)
├── parser.rs           # Response parsing (~150 lines)
├── genealogist.rs      # Lineage building (~180 lines)
├── messenger.rs        # AI communication (~180 lines)
├── director.rs         # Flow orchestration (~300 lines)
├── reviewer.rs         # Review UI data (~350 lines)
└── integration.rs      # Bevy ECS bridge (~250 lines)
```

---

## Key Data Flows

### Index Mapping Flow (The Bug Fix)

```
BEFORE (Broken):
  Original rows: [0, 1, 2]
  AI adds row: gets index 3
  Next step: AI sees "3" instead of display value
  Result: Wrong row referenced

AFTER (Fixed):
  Original rows: Navigator registers [0, 1, 2] with display values
  AI adds row: Navigator assigns index 3, stores display value
  Next step: PreProcessor reads display values from Navigator
  Result: Correct human-readable names used
```

### Row Categorization Flow

```
Original Data:   ["MiG-25PD", "F-15C", "Su-27"]
AI Response:     ["MiG-25PD", "F-15C", "Su-35"]  ← Su-35 is new

Parser Logic:
  1. "MiG-25PD" in original? YES → Original
  2. "F-15C" in original? YES → Original
  3. "Su-35" in original? NO → AiAdded
  4. "Su-27" in response? NO → Lost (skip)
```

### Parent Validation Flow

```
Structure table: Aircraft_Engines
AI adds row: { name: "F404", parent_key: "F/A-18C" }

Validation:
  1. Look up "F/A-18C" in Navigator for parent table
  2. If found → Valid, proceed
  3. If NOT found → Invalid, show picker with valid options
```

---
