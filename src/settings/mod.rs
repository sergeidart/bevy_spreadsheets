pub mod io;

use crate::ui::elements::editor::state::FpsSetting;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub fps_setting: FpsSetting,
    /// Whether to show hidden sheets (override metadata.hidden) in lists
    /// Default: false (respect hidden flags)
    pub show_hidden_sheets: bool,
    /// AI depth limit: how many levels of structure tables to process
    /// Default: 2
    #[serde(default = "default_ai_depth_limit")]
    pub ai_depth_limit: usize,
    /// AI width limit: how many rows to send in one batch
    /// Default: 32
    #[serde(default = "default_ai_width_limit")]
    pub ai_width_limit: usize,
}

fn default_ai_depth_limit() -> usize {
    2
}

fn default_ai_width_limit() -> usize {
    32
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            fps_setting: FpsSetting::default(),
            show_hidden_sheets: false,
            ai_depth_limit: default_ai_depth_limit(),
            ai_width_limit: default_ai_width_limit(),
        }
    }
}
