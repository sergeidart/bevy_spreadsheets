// src/visual_copier/events.rs

use bevy::prelude::*;
use std::path::PathBuf;
use super::resources::CopyError; // Import CopyError

/// Event to add a new, empty copy task.
#[derive(Event, Debug)]
pub struct AddNewCopyTaskEvent;

/// Event to remove a copy task by its ID.
#[derive(Event, Debug)]
pub struct RemoveCopyTaskEvent(pub usize);

/// Event to request picking a folder.
#[derive(Event, Debug, Clone)]
pub struct PickFolderRequest {
    /// If Some, this ID is for a specific CopyTask. If None, it's for the top panel.
    pub for_task_id: Option<usize>,
    /// True if picking for the 'start_folder' or 'top_panel_from_folder'. False for 'end_folder' or 'top_panel_to_folder'.
    pub is_start_folder: bool,
}

/// Event sent after a folder has been picked (or selection cancelled).
#[derive(Event, Debug, Clone)]
pub struct FolderPickedEvent {
    pub for_task_id: Option<usize>,
    pub is_start_folder: bool,
    pub path: Option<PathBuf>, // None if selection was cancelled
}

// --- NEW Specific Folder Update Events ---
#[derive(Event, Debug, Clone)]
pub struct UpdateTaskStartFolderEvent {
    pub task_id: usize,
    pub path: Option<PathBuf>, // Option to handle cancellation or clearing
}

#[derive(Event, Debug, Clone)]
pub struct UpdateTaskEndFolderEvent {
    pub task_id: usize,
    pub path: Option<PathBuf>, // Option to handle cancellation or clearing
}

#[derive(Event, Debug, Clone)]
pub struct UpdateTopPanelFromFolderEvent {
    pub path: Option<PathBuf>, // Option to handle cancellation or clearing
}

#[derive(Event, Debug, Clone)]
pub struct UpdateTopPanelToFolderEvent {
    pub path: Option<PathBuf>, // Option to handle cancellation or clearing
}
// --- END NEW Specific Folder Update Events ---

/// Event to initiate a copy operation for a specific task ID.
#[derive(Event, Debug)]
pub struct QueueCopyTaskEvent(pub usize);

/// Event to initiate the copy operation for the top panel folders.
#[derive(Event, Debug)]
pub struct QueueTopPanelCopyEvent;

/// Event to initiate copy operation for all configured tasks.
#[derive(Event, Debug)]
pub struct QueueAllCopyTasksEvent;

/// Event to swap the 'from' and 'to' folders in the top panel.
#[derive(Event, Debug)]
pub struct ReverseTopPanelFoldersEvent;

/// Event to report the result of a copy operation.
#[derive(Event, Debug, Clone)]
pub struct CopyOperationResultEvent {
    /// If Some, this ID is for a specific CopyTask. If None, it's for the top panel.
    pub task_id: Option<usize>, // None for top panel copy
    pub result: Result<String, CopyError>, // Ok contains success message (e.g., with timestamp), Err contains CopyError
}