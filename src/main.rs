// src/main.rs

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use bevy::{
    log::LogPlugin,
    prelude::*,
    window::{PrimaryWindow, WindowPlugin},
    winit::{UpdateMode, WinitSettings},
};
use std::time::Duration;

use image::ImageFormat as CrateImageFormat;
use winit::window::Icon as WinitIcon;

use bevy_egui::EguiPlugin;
use bevy_tokio_tasks::TokioTasksPlugin;

mod sheets;
mod ui;
mod example_definitions;

use sheets::SheetsPlugin;
use ui::EditorUiPlugin;

// KEYRING constants are no longer strictly needed here if we fully remove keyring usage
// const KEYRING_SERVICE_NAME: &str = "bevy_spreadsheet_ai";
// const KEYRING_API_KEY_USERNAME: &str = "llm_api_key";

#[derive(Resource, Debug, Default)]
pub struct ApiKeyDisplayStatus {
    pub status: String,
}

// --- NEW: Resource to hold the API key for the current session ---
#[derive(Resource, Debug, Default)]
pub struct SessionApiKey(pub Option<String>);
// --- END NEW ---

fn main() {
    App::new()
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::Continuous,
            unfocused_mode: UpdateMode::reactive_low_power(Duration::from_secs_f32(1.0 / 5.0)),
        })
        .init_resource::<ApiKeyDisplayStatus>()
        // --- NEW: Initialize SessionApiKey resource ---
        .init_resource::<SessionApiKey>()
        // --- END NEW ---
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Standalone Sheet Editor".into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(LogPlugin {
                    level: bevy::log::Level::INFO,
                    filter: "wgpu=error,naga=warn,bevy_tokio_tasks=warn".to_string(),
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin {
            enable_multipass_for_primary_context: true,
        })
        .add_plugins(TokioTasksPlugin::default())
        .add_plugins(SheetsPlugin)
        .add_plugins(EditorUiPlugin)
        .add_systems(Startup, (
            initialize_api_key_status_startup, // Renamed and modified system
            set_window_icon,
        ))
        .run();
}

// --- MODIFIED: Simplified startup system ---
fn initialize_api_key_status_startup(mut api_key_status_res: ResMut<ApiKeyDisplayStatus>) {
    // Since the key is session-only, it's never set on startup from persistent storage.
    info!("API Key is session-only. Initial status: No Key Set (Session).");
    api_key_status_res.status = "No Key Set (Session)".to_string();
}
// --- END MODIFIED ---

fn set_window_icon(
     primary_window_query: Query<Entity, With<PrimaryWindow>>,
     windows: NonSend<bevy::winit::WinitWindows>,
 ) {
     let Ok(primary_entity) = primary_window_query.get_single() else {
         warn!("Could not find single primary window to set icon.");
         return;
     };

     let Some(primary_winit_window) = windows.get_window(primary_entity) else {
         warn!("Could not get winit window for primary window entity.");
         return;
     };

     let icon_path = "assets/icon.png";
     match std::fs::read(icon_path) {
         Ok(icon_bytes) => {
             match image::load_from_memory_with_format(&icon_bytes, CrateImageFormat::Png) {
                 Ok(image_data) => {
                     let image_buffer = image_data.into_rgba8();
                     let (width, height) = image_buffer.dimensions();
                     let rgba_data = image_buffer.into_raw();

                     match WinitIcon::from_rgba(rgba_data, width, height) {
                         Ok(winit_icon) => {
                             primary_winit_window.set_window_icon(Some(winit_icon));
                             info!("Successfully set window icon using 'image' crate and 'winit::window::Icon' from: {}", icon_path);
                         }
                         Err(e) => {
                             warn!("Failed to create winit::window::Icon: {:?}", e);
                         }
                     }
                 }
                 Err(e) => {
                     warn!("'image' crate: Failed to load image data from '{}': {}", icon_path, e);
                 }
             }
         }
         Err(e) => {
             warn!("Failed to read icon file '{}': {}", icon_path, e);
         }
     }
 }