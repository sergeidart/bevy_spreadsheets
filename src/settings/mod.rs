pub mod io;

use crate::ui::elements::editor::state::FpsSetting;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub fps_setting: FpsSetting,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            fps_setting: FpsSetting::default(),
        }
    }
}
