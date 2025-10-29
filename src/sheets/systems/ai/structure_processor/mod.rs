// src/sheets/systems/ai/structure_processor/mod.rs
//! Structure AI processing system - orchestrates AI-powered structure data generation

mod existing_row_extractor;
mod new_row_extractor;
mod python_executor;
mod task_executor;

pub use task_executor::process_structure_ai_jobs;
