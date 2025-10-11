// src/sheets/definitions/sheet_grid_data.rs
use serde::{Deserialize, Serialize};

use super::sheet_metadata::SheetMetadata;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SheetGridData {
    #[serde(skip)]
    pub metadata: Option<SheetMetadata>,
    pub grid: Vec<Vec<String>>,
}
