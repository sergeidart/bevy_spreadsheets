# SkylineDB

A desktop spreadsheet/database editor built with [Bevy](https://bevyengine.org/) and [egui](https://github.com/emilk/egui). SkylineDB provides a spreadsheet-like interface backed by SQLite databases with AI-powered features using Google Gemini.

## Features

### Core Spreadsheet Functionality
- **SQLite-backed storage** - Each category is a separate `.db` file with full relational database capabilities
- **Multiple sheets per database** - Organize data into tables within categories
- **Column types** - Support for String, Bool, I64 (integer), and F64 (float) data types
- **Column validators**:
  - **Basic** - Standard data type validation
  - **Linked** - Reference values from another sheet/column (foreign key relationships)
  - **Structure** - Nested spreadsheets within cells (hierarchical data)

### AI Integration (Google Gemini)
- **AI Autofill** - Generate row content using AI with context from existing data
- **Google Search Grounding** - Enhanced AI responses using real-time web search
- **Per-column AI settings** - Control which columns are included in AI context
- **AI Schema Groups** - Create multiple AI prompt configurations per sheet
- **Structure-aware AI** - AI understands and can generate nested data
- **Custom AI rules** - Define per-table context and generation rules

### Database Architecture
- **SQLite Daemon** - Write operations serialized through a daemon process for concurrency safety
- **Direct reads** - Fast read access directly to SQLite for performance
- **Auto-migration** - Automatic schema migrations and column management
- **JSON legacy support** - Can migrate from older JSON-based storage

## CLI Tools

SkylineDB includes several command-line utilities for database maintenance:

```bash
# Repair corrupted metadata tables
skylinedb repair-metadata <path>

# Diagnose metadata issues
skylinedb diagnose-metadata <path>

# Add missing display_name column to metadata
skylinedb add-display-name <path>

# List columns in a table
skylinedb list-columns <path>

# Sync column names between metadata and physical table
skylinedb sync-column-names <path>

# Restore missing columns from metadata
skylinedb restore-columns <path>

# Check which columns have Structure validators
skylinedb check-structure-columns [path]
```

## Technology Stack

- **Bevy 0.16** - Game engine used as application framework
- **egui / bevy_egui** - Immediate mode GUI
- **rusqlite** - SQLite database access
- **tokio** - Async runtime for background operations
- **Google Gemini API** - AI text generation (via `gemini_client_rs`)
- **pyo3** - Python integration for AI processing scripts
- **Windows Named Pipes** - IPC with SQLite daemon

## Building

```bash
cargo build --release
```

The release binary will be at `target/release/skylinedb.exe`.

## Configuration

- **API Key**: Set your Google Gemini API key through the Settings popup (stored securely in Windows Credential Manager)
- **Data Location**: Databases are stored in `Documents/SkylineDB/` by default

## Requirements

- Windows (primary platform, uses Windows-specific features for IPC and credential storage)
- Google Gemini API key for AI features

## License

This project is partially AI-assisted in development.
