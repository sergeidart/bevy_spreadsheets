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
use dotenvy::dotenv;

// --- ADD THIS IMPORT ---
use pyo3::prelude::*;

mod sheets;
mod ui;
mod example_definitions;
mod visual_copier;

use sheets::SheetsPlugin;
use ui::EditorUiPlugin;
use visual_copier::VisualCopierPlugin;

#[derive(Resource, Debug, Default)]
pub struct ApiKeyDisplayStatus {
    pub status: String,
}

#[derive(Resource, Debug, Default)]
pub struct SessionApiKey(pub Option<String>);

fn main() {
    // --- Always write the Python script at startup to ensure it's up to date ---
    const AI_PROCESSOR_PY: &str = include_str!("../script/ai_processor.py");
    let script_path = std::path::Path::new("script/ai_processor.py");
    if let Some(parent) = script_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("Failed to create script directory: {e}");
        }
    }
    if let Err(e) = std::fs::write(script_path, AI_PROCESSOR_PY) {
        eprintln!("Failed to write ai_processor.py: {e}");
    } else {
        println!("ai_processor.py written to script/ directory.");
    }

    // --- ADD THIS LINE ---
    // This initializes the Python interpreter for use in multiple threads,
    // which is necessary for the background tasks that call the Python script.
    pyo3::prepare_freethreaded_python();

    match dotenv() {
        Ok(path) => info!("Loaded .env file from: {:?}", path),
        Err(_) => info!(".env file not found or failed to load. API key must be set via UI or other means."),
    }

    App::new()
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::Continuous,
            unfocused_mode: UpdateMode::reactive_low_power(Duration::from_secs_f32(1.0 / 1.0)),
        })
        .init_resource::<ApiKeyDisplayStatus>()
        .init_resource::<SessionApiKey>()
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
                    filter: "wgpu=error,naga=warn,bevy_tokio_tasks=warn,hyper=warn,reqwest=warn,gemini_client_rs=info,visual_copier=info".to_string(),
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin {
            enable_multipass_for_primary_context: true,
        })
        .add_plugins(TokioTasksPlugin::default())
        .add_plugins(SheetsPlugin)
        .add_plugins(EditorUiPlugin)
        .add_plugins(VisualCopierPlugin)
        .add_systems(Startup, (
            initialize_api_key_status_startup,
            set_window_icon,
        ))
        .run();
}

// Functions initialize_api_key_status_startup and set_window_icon remain the same
fn initialize_api_key_status_startup(
    mut api_key_status_res: ResMut<ApiKeyDisplayStatus>,
    mut session_api_key: ResMut<SessionApiKey>,
) {
    // Only try to load from Windows Credential Manager
    if let Ok(keyring) = keyring::Entry::new("GoogleGeminiAPI", whoami::username().as_str()) {
        match keyring.get_password() {
            Ok(cred) if !cred.is_empty() => {
                info!("API Key loaded from Windows Credential Manager. Populating SessionApiKey.");
                session_api_key.0 = Some(cred);
                api_key_status_res.status = "Key Set (Session from Windows Credential Manager)".to_string();
                return;
            }
            _ => {}
        }
    }
    if session_api_key.0.is_some() {
        api_key_status_res.status = "Key Set (Session)".to_string();
    } else {
        api_key_status_res.status = "No Key Set (Session)".to_string();
        info!("API Key not set in SessionApiKey. User needs to set it via UI for AI features.");
    }
}

fn set_window_icon(
     primary_window_query: Query<Entity, With<PrimaryWindow>>,
     windows: NonSend<bevy::winit::WinitWindows>,
 ) {
     let Ok(primary_entity) = primary_window_query.single() else {
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
                             info!("Successfully set window icon from: {}", icon_path);
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