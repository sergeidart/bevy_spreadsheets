// src/visual_copier/resources.rs

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Represents a single copy task configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
#[reflect(Default, Serialize, Deserialize)]
pub struct CopyTask {
    pub id: usize,
    pub start_folder: Option<PathBuf>,
    pub end_folder: Option<PathBuf>,
    pub status: String,
}

impl Default for CopyTask {
    fn default() -> Self {
        Self {
            id: 0, // Default ID, should be set properly on creation
            start_folder: None,
            end_folder: None,
            status: "Idle".to_string(),
        }
    }
}

impl CopyTask {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            start_folder: None,
            end_folder: None,
            status: "Idle".to_string(),
        }
    }
}

/// Error types for copy operations.
#[derive(Error, Debug, Clone, Reflect)]
pub enum CopyError {
    #[error("I/O error: {0}")]
    Io(String), // Store String to be Reflect-friendly
    #[error("File system operation error: {0}")]
    FsExtra(String), // Store String
    #[error("Start folder is not set.")]
    StartFolderMissing,
    #[error("End folder is not set.")]
    EndFolderMissing,
    #[error("Start path is not a directory: {0}")]
    StartNotADirectory(PathBuf),
    #[error("End path could not be created or is invalid: {0}")]
    EndPathInvalid(PathBuf),
    #[error("Source path does not exist: {0}")]
    SourceDoesNotExist(PathBuf),
    #[error("Task with ID {0} not found.")]
    TaskNotFound(usize),
}

// Convert std::io::Error to CopyError::Io
impl From<std::io::Error> for CopyError {
    fn from(err: std::io::Error) -> Self {
        CopyError::Io(err.to_string())
    }
}

// Convert fs_extra::error::Error to CopyError::FsExtra
impl From<fs_extra::error::Error> for CopyError {
    fn from(err: fs_extra::error::Error) -> Self {
        CopyError::FsExtra(err.to_string())
    }
}


/// Main resource holding the state of the Visual Copier.
#[derive(Resource, Debug, Default, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Serialize, Deserialize)]
pub struct VisualCopierManager {
    pub copy_tasks: Vec<CopyTask>,
    pub next_id: usize,
    pub top_panel_from_folder: Option<PathBuf>,
    pub top_panel_to_folder: Option<PathBuf>,
    // --- NEW FIELD ---
    #[serde(default)] // Default to false if missing during load
    pub copy_top_panel_on_exit: bool, // Save this preference
    // --- END NEW FIELD ---
    #[serde(skip, default = "default_status_string")]
    #[reflect(skip_serializing)]
    pub top_panel_copy_status: String,
    #[serde(skip)]
    #[reflect(skip_serializing)]
    pub is_saving_on_exit: bool,
}

fn default_status_string() -> String {
    "Idle".to_string()
}


impl VisualCopierManager {
    /// Gets the next available ID for a new copy task.
    pub fn get_next_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Recalculates the next_id based on the highest existing ID in copy_tasks.
    pub fn recalculate_next_id(&mut self) {
        self.next_id = self.copy_tasks.iter()
            .map(|task| task.id)
            .max()
            .map_or(0, |max_id| max_id + 1);
        debug!("VisualCopierManager: Recalculated next_id to {}", self.next_id);
    }

    /// Resets transient status fields after loading.
    pub fn reset_transient_state(&mut self) {
        self.top_panel_copy_status = default_status_string();
        for task in self.copy_tasks.iter_mut() {
            if task.status.starts_with("Copying...") || task.status.starts_with("Queued...") || task.status.starts_with("Error:") {
                 task.status = "Idle".to_string();
            }
        }
        self.is_saving_on_exit = false;
    }
}