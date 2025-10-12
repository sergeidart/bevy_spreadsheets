// src/sheets/database/systems/background_state.rs

use crate::sheets::events::MigrationProgress;
use bevy::prelude::*;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

#[derive(Resource, Default)]
pub struct MigrationBackgroundState {
    pub progress_rx: Option<Arc<Mutex<Receiver<MigrationProgress>>>>, // progress updates
    pub completion_rx:
        Option<Arc<Mutex<Receiver<Result<(super::super::migration::MigrationReport, PathBuf), String>>>>>, // final result with db path
    /// Optional target to auto-select after completion: (category/db name, table name)
    pub post_select: Option<(String, String)>,
}
