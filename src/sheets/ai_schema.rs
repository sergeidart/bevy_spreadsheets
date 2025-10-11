// src/sheets/definitions/ai_schema.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiSchemaGroup {
    pub name: String,
    #[serde(default)]
    pub included_columns: Vec<usize>,
    #[serde(default)]
    pub allow_add_rows: bool,
    #[serde(default)]
    pub structure_row_generation_overrides: Vec<AiSchemaGroupStructureOverride>,
    #[serde(default)]
    pub included_structures: Vec<Vec<usize>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiSchemaGroupStructureOverride {
    pub path: Vec<usize>,
    pub allow_add_rows: bool,
}
