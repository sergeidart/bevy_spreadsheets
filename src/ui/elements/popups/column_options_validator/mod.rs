// src/ui/elements/popups/column_options_validator/mod.rs
// Module organization for column validator options UI and logic

mod filter_widgets;
mod schema_helpers;
mod state_sync;
mod validator_apply;
mod validator_ui;
mod validator_validation;

// Re-export public API
pub use validator_apply::apply_validator_update;
pub use validator_ui::show_validator_section;
pub use validator_validation::is_validator_config_valid;
