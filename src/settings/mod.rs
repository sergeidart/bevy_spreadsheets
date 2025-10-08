pub mod io;

use crate::ui::elements::editor::state::FpsSetting;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub fps_setting: FpsSetting,
    /// Whether to show hidden sheets (override metadata.hidden) in lists
    /// Default: false (respect hidden flags)
    pub show_hidden_sheets: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            fps_setting: FpsSetting::default(),
            show_hidden_sheets: false,
        }
    }
}
