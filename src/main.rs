// src/main.rs

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use bevy::{
    log::LogPlugin,
    prelude::*,
    window::{PrimaryWindow, WindowPlugin}, // Keep PrimaryWindow and WindowPlugin
    winit::{WinitSettings, UpdateMode},
};
use std::time::Duration;

// For loading the icon image from disk using the image crate
use image::ImageFormat as CrateImageFormat; // Alias to avoid conflict

// For the winit window icon type
use winit::window::Icon as WinitIcon; // Import winit's Icon type

use bevy_egui::EguiPlugin;
use bevy_tokio_tasks::TokioTasksPlugin;

mod sheets;
mod ui;
mod example_definitions;

use sheets::SheetsPlugin;
use ui::EditorUiPlugin;

const KEYRING_SERVICE_NAME: &str = "bevy_spreadsheet_ai";
const KEYRING_API_KEY_USERNAME: &str = "llm_api_key";

fn main() {
    App::new()
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::Continuous,
            unfocused_mode: UpdateMode::reactive_low_power(Duration::from_secs_f32(1.0 / 5.0)),
        })
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
            check_api_key_startup,
            set_window_icon,
        ))
        .run();
}

fn check_api_key_startup(mut state: Local<ui::elements::editor::EditorWindowState>) {
     match keyring::Entry::new(KEYRING_SERVICE_NAME, KEYRING_API_KEY_USERNAME) {
        Ok(entry) => match entry.get_password() {
             Ok(_) => {
                  info!("API Key found in keyring on startup.");
                  state.settings_api_key_status = "Key Set".to_string();
             },
             Err(keyring::Error::NoEntry) => {
                  info!("No API Key found in keyring on startup.");
                  state.settings_api_key_status = "No Key Set".to_string();
             }
             Err(e) => {
                  error!("Error accessing keyring on startup: {}", e);
                  state.settings_api_key_status = "Keyring Error".to_string();
             }
        },
        Err(e) => {
             error!("Error creating keyring entry on startup: {}", e);
             state.settings_api_key_status = "Keyring Error".to_string();
        }
   }
}


fn set_window_icon(
     primary_window_query: Query<Entity, With<PrimaryWindow>>,
     windows: NonSend<bevy::winit::WinitWindows>,
     // AssetServer is removed as we are not using Bevy's asset system for this
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
                     let image_buffer = image_data.into_rgba8(); // image crate's RgbaImage
                     let (width, height) = image_buffer.dimensions();
                     let rgba_data = image_buffer.into_raw(); // This is Vec<u8>
                     
                     // Use winit::window::Icon::from_rgba
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
 