// src/main.rs
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use crate::ui::elements::editor::state::{EditorWindowState, FpsSetting};
use bevy::{
    log::LogPlugin,
    prelude::*,
    window::{PrimaryWindow, WindowPlugin},
    winit::{UpdateMode, WinitSettings},
};
use bevy_framepace::Limiter;
use std::time::Duration;
use winit::window::UserAttentionType;

use image::ImageFormat as CrateImageFormat;
use winit::window::Icon as WinitIcon;

use bevy_egui::EguiPlugin;
use bevy_tokio_tasks::TokioTasksPlugin;
use dotenvy::dotenv;
mod settings;

mod example_definitions;
mod sheets;
mod single_instance;
mod ui;
mod visual_copier;
mod cli;

use sheets::SheetsPlugin;
use ui::EditorUiPlugin;
use visual_copier::VisualCopierPlugin;
use clap::Parser;

#[derive(Resource, Debug, Default)]
pub struct ApiKeyDisplayStatus {
    pub status: String,
}

#[derive(Resource, Debug, Default)]
pub struct SessionApiKey(pub Option<String>);

#[derive(Resource)]
pub struct IpcReceiver(std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>);

fn main() {
    // Parse CLI arguments first
    let cli = cli::Cli::parse();
    
    // If a subcommand was provided, run it and exit (don't start GUI)
    if let Some(command) = cli.command {
        if let Err(e) = run_cli_command(command) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        return;
    }
    
    // No subcommand - launch GUI app
    run_gui_app();
}

fn run_cli_command(command: cli::Commands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        cli::Commands::RepairMetadata { path } => {
            cli::repair_metadata::run(path)?;
        }
        cli::Commands::DiagnoseMetadata { path } => {
            cli::diagnose_metadata::run(path)?;
        }
        cli::Commands::AddDisplayName { path } => {
            cli::add_display_name::run(path)?;
        }
        cli::Commands::ListColumns { path } => {
            cli::list_columns::run(path)?;
        }
        cli::Commands::SyncColumnNames { path } => {
            cli::sync_column_names::run(path)?;
        }
        cli::Commands::RestoreColumns { path } => {
            cli::restore_columns::run(path)?;
        }
        cli::Commands::CheckStructureColumns { path } => {
            cli::check_structure_columns::run(path)?;
        }
    }
    Ok(())
}

fn run_gui_app() {
    // Check for single instance before doing anything else
    let _instance_guard = match single_instance::SingleInstanceGuard::try_acquire() {
        Some(guard) => guard,
        None => {
            // Another instance is already running
            println!("Another instance of SkylineDB is already running. Bringing it to foreground...");
            single_instance::SingleInstanceGuard::signal_existing_instance();
            std::thread::sleep(std::time::Duration::from_millis(500)); // Give time for the signal
            return; // Exit this instance
        }
    };

    // Start IPC listener to receive signals from other instances
    let ipc_receiver = single_instance::start_ipc_listener();

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
        Err(_) => info!(
            ".env file not found or failed to load. API key must be set via UI or other means."
        ),
    }

    App::new()
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::Continuous,
            unfocused_mode: UpdateMode::reactive_low_power(Duration::from_secs_f32(1.0 / 1.0)),
        })
        .insert_resource(IpcReceiver(std::sync::Arc::new(std::sync::Mutex::new(ipc_receiver))))
        .init_resource::<ApiKeyDisplayStatus>()
        .init_resource::<SessionApiKey>()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "SkylineDB - Spreadsheet Editor".into(),
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
        .add_plugins(bevy_framepace::FramepacePlugin)
        .add_plugins(SheetsPlugin)
        .add_plugins(EditorUiPlugin)
        .add_plugins(VisualCopierPlugin)
        .add_systems(Startup, (
            initialize_api_key_status_startup,
            set_window_icon,
            load_app_settings_startup,
        ))
        .add_systems(Update, fps_limit)
        .add_systems(Update, handle_ipc_focus_request)
        .run();
    
    // Keep the instance guard alive until the app exits
    drop(_instance_guard);
}

fn fps_limit(
    mut settings: ResMut<bevy_framepace::FramepaceSettings>,
    state: Res<EditorWindowState>,
    primary_window_query: Query<Entity, With<PrimaryWindow>>,
    windows: NonSend<bevy::winit::WinitWindows>,
) {
    match state.fps_setting {
        FpsSetting::Thirty => settings.limiter = Limiter::from_framerate(30.0),
        FpsSetting::Sixty => settings.limiter = Limiter::from_framerate(60.0),
        FpsSetting::ScreenHz => {
            // Try to read monitor refresh rate from the primary winit window; fall back to no limiter (0.0) which lets the OS/compositor decide.
            let mut applied = false;
            if let Ok(primary_entity) = primary_window_query.single() {
                if let Some(winit_window) = windows.get_window(primary_entity) {
                    if let Some(monitor) = winit_window.current_monitor() {
                        // Try to get the refresh rate from the monitor's video modes (best-effort)
                        if let Some(video_mode) = monitor.video_modes().next() {
                            // video_mode.refresh_rate_millihertz() returns millihertz (1/1000 Hz)
                            let refresh_mhz = video_mode.refresh_rate_millihertz();
                            if refresh_mhz > 0 {
                                let refresh_hz = (refresh_mhz as f64) / 1000.0;
                                settings.limiter = Limiter::from_framerate(refresh_hz);
                                applied = true;
                            }
                        }
                    }
                }
            }
            if !applied {
                // No reliable screen Hz found; disable explicit limiting (let run at max / V-sync)
                settings.limiter = Limiter::from_framerate(0.0);
            }
        }
    }
}

fn load_app_settings_startup(mut state: ResMut<EditorWindowState>) {
    // Best-effort: Load persisted AppSettings and populate UI state
    if let Ok(loaded) = settings::io::load_settings_from_file::<settings::AppSettings>() {
        state.fps_setting = loaded.fps_setting;
        state.show_hidden_sheets = loaded.show_hidden_sheets;
        info!(
            "Loaded app settings: fps_setting={:?}, show_hidden_sheets={}",
            state.fps_setting, state.show_hidden_sheets
        );
    } else {
        info!("No persisted app settings found; using defaults.");
    }
}

fn handle_ipc_focus_request(
    ipc_receiver: Res<IpcReceiver>,
    primary_window_query: Query<Entity, With<PrimaryWindow>>,
    windows: NonSend<bevy::winit::WinitWindows>,
) {
    // Check if there's a request to bring the window to foreground
    if let Ok(receiver) = ipc_receiver.0.lock() {
        if receiver.try_recv().is_ok() {
            if let Ok(primary_entity) = primary_window_query.single() {
                if let Some(winit_window) = windows.get_window(primary_entity) {
                    winit_window.focus_window();
                    winit_window.request_user_attention(Some(UserAttentionType::Critical));
                    info!("Window brought to foreground due to IPC request");
                }
            }
        }
    }
}

// Functions initialize_api_key_status_startup and set_window_icon remain the same
fn initialize_api_key_status_startup(
    mut api_key_status_res: ResMut<ApiKeyDisplayStatus>,
    mut session_api_key: ResMut<SessionApiKey>,
) {
    // Only try to load from Windows Credential Manager
    // Use the same keyring service as Settings & Python (consistency avoids needing to open Settings once)
    if let Ok(keyring) = keyring::Entry::new("GoogleGeminiAPI", whoami::username().as_str()) {
        match keyring.get_password() {
            Ok(cred) if !cred.is_empty() => {
                info!("API Key loaded from Windows Credential Manager. Populating SessionApiKey.");
                session_api_key.0 = Some(cred);
                api_key_status_res.status =
                    "Key Set (Session from Windows Credential Manager)".to_string();
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

    // Prefer embedded icon bytes to avoid path/cwd issues at runtime
    // Fallback to reading from filesystem if the include fails during dev
    const ICON_BYTES: &[u8] = include_bytes!("../assets/icon.png");
    match image::load_from_memory_with_format(ICON_BYTES, CrateImageFormat::Png) {
        Ok(image_data) => {
            let image_buffer = image_data.into_rgba8();
            let (width, height) = image_buffer.dimensions();
            let rgba_data = image_buffer.into_raw();

            match WinitIcon::from_rgba(rgba_data, width, height) {
                Ok(winit_icon) => {
                    primary_winit_window.set_window_icon(Some(winit_icon));
                    info!("Successfully set window icon (embedded bytes)");
                    primary_winit_window
                        .request_user_attention(Some(UserAttentionType::Critical));
                }
                Err(e) => {
                    warn!("Failed to create winit::window::Icon: {:?}", e);
                }
            }
        }
        Err(e) => {
            warn!("Failed to decode embedded icon.png: {} — will try filesystem path", e);
            let icon_path = "assets/icon.png";
            match std::fs::read(icon_path) {
                Ok(icon_bytes) => match image::load_from_memory_with_format(&icon_bytes, CrateImageFormat::Png) {
                    Ok(image_data) => {
                        let image_buffer = image_data.into_rgba8();
                        let (width, height) = image_buffer.dimensions();
                        let rgba_data = image_buffer.into_raw();
                        match WinitIcon::from_rgba(rgba_data, width, height) {
                            Ok(winit_icon) => {
                                primary_winit_window.set_window_icon(Some(winit_icon));
                                info!("Successfully set window icon from: {}", icon_path);
                                primary_winit_window
                                    .request_user_attention(Some(UserAttentionType::Critical));
                            }
                            Err(e) => warn!("Failed to create winit::window::Icon: {:?}", e),
                        }
                    }
                    Err(e) => warn!("Failed to decode icon file '{}': {}", icon_path, e),
                },
                Err(e) => warn!("Failed to read icon file '{}': {}", icon_path, e),
            }
        }
    }
}
