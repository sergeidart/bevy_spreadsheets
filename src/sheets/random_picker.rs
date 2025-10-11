// src/sheets/definitions/random_picker.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RandomPickerMode {
    Simple,
    Complex,
}

impl Default for RandomPickerMode {
    fn default() -> Self {
        RandomPickerMode::Simple
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RandomPickerSettings {
    #[serde(default, skip_serializing_if = "is_simple_mode")]
    pub mode: RandomPickerMode,
    /// Used when mode == Simple
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub simple_result_col_index: usize,
    /// Used when mode == Complex
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub complex_result_col_index: usize,
    // Legacy single-weight fields retained for backward compatibility. Don't serialize when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight_col_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub second_weight_col_index: Option<usize>,
    /// New: support arbitrary weight columns (stored as indices)
    #[serde(default)]
    pub weight_columns: Vec<usize>,
    /// Per-weight-column exponent/power applied to that column's numeric value before combining.
    /// Default is 1.0 for each weight column (no change).
    #[serde(default)]
    pub weight_exponents: Vec<f64>,
    /// Per-weight-column linear multiplier applied before exponentiation. Default 1.0.
    #[serde(default)]
    pub weight_multipliers: Vec<f64>,
    /// New: support multiple summarizer columns
    #[serde(default)]
    pub summarizer_columns: Vec<usize>,
}

// Helper functions for skip_serializing_if
fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}

fn is_simple_mode(m: &RandomPickerMode) -> bool {
    matches!(m, RandomPickerMode::Simple)
}
