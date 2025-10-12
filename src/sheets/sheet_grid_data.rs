// src/sheets/definitions/sheet_grid_data.rs
use serde::{Deserialize, Serialize};

use super::sheet_metadata::SheetMetadata;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SheetGridData {
    #[serde(skip)]
    pub metadata: Option<SheetMetadata>,
    pub grid: Vec<Vec<String>>,
    /// Maps grid row index to database row_index (for DB-backed tables only)
    /// This allows us to directly identify which DB row to delete/update
    #[serde(skip)]
    pub row_indices: Vec<i64>,
}
