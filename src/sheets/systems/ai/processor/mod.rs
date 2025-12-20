// src/sheets/systems/ai/processor/mod.rs
//! Unified AI Processing System
//!
//! This module provides a clean, single-path AI processing flow that handles
//! both single-step (root tables) and multi-step (structure tables) processing.
//!
//! ## Architecture Overview
//!
//! The processor is organized into these key components:
//!
//! - **Navigator**: Index mapping and stable row ID management (CRITICAL for fixing index mismatch)
//! - **PreProcessor**: Data preparation before AI calls, display value resolution
//! - **Parser**: AI response parsing and row categorization
//! - **Storager**: Result persistence across processing steps
//! - **Genealogist**: Parent lineage/ancestry building for structure tables
//! - **Messenger**: AI communication (Python/Gemini bridge)
//! - **Director**: Flow orchestration and step management
//! - **Integration**: Wiring between Director and EditorWindowState for AI Review
//!
//! ## Key Design Principles
//!
//! 1. Single processing path for all table types (no parallel systems)
//! 2. Stable row IDs that persist across steps
//! 3. Human-readable display values (not raw indexes) in AI context
//! 4. Clear separation of concerns between components

pub mod navigator;
pub mod storager;
pub mod parser;
pub mod pre_processor;
pub mod genealogist;
pub mod messenger;
pub mod director;
pub mod integration;

// Re-exports for external access (only what's actually used outside processor module)
pub use integration::{
    DirectorSession,
    start_director_session_v2, poll_director_results,
    cancel_director_session,
};
