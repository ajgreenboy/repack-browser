mod client_id;
mod config;
mod extractor;
mod server_client;
mod system_info;

use config::Config;
use eframe::egui;
use log::{error, info, warn};
use server_client::ServerClient;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use tokio::time;

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

// Shared app state
struct AppState {
    config: Arc<RwLock<Config>>,
    server_client: Arc<ServerClient>,
    runtime: Arc<Runtime>,
    status: Arc<RwLock<String>>,
    current_installation: Arc<RwLock<Option<InstallationInfo>>>,
    is_paused: Arc<RwLock<bool>>,
}

#[derive(Clone)]
struct InstallationInfo {
    game_title: String,
    installer_path: PathBuf,
    started_at: String,
    status: String,
}

struct SettingsWindow {
    state: Arc<AppState>,
    server_url: String,
    download_folder: String,
    run_on_startup: bool,
    show_window: Arc<RwLock<bool>>,
}

impl SettingsWindow {
    fn new(state: Arc<AppState>) -> Self {
        let config = state.runtime.block_on(async {
            state.config.read().await.clone()
        });

        Self {
            server_url: config.server.url.clone(),
            download_folder: config.extraction.output_dir.to_string_lossy().to_string(),
            run_on_startup: is_in_startup(),
            show_window: Arc::new(RwLock::new(true)),
            state,
        }
    }
}

impl eframe::App for SettingsWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Repack Auto-Installer Settings");
            ui.add_space(10.0);

            // Explanation
            ui.label(egui::RichText::new(
                "This app automatically installs downloaded game repacks. \
                It monitors your download folder and runs installers when downloads complete."
            ).italics().color(egui::Color32::GRAY));

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);

            // Current status
            let status = self.state.runtime.block_on(async {
                self.state.status.read().await.clone()
            });

            ui.horizontal(|ui| {
                ui.label("Status:");
                ui.label(egui::RichText::new(&status).strong().color(
                    if status.contains("Installing") {
                        egui::Color32::from_rgb(100, 200, 100)
                    } else if status.contains("Error") {
                        egui::Color32::from_rgb(200, 100, 100)
                    } else {
                        egui::Color32::GRAY
                    }
                ));
            });

            // Current installation
            if let Some(install) = self.state.runtime.block_on(async {
                self.state.current_installation.read().await.clone()
            }) {
                ui.add_space(5.0);
                ui.label(format!("📦 Installing: {}", install.game_title));
                ui.label(format!("   Started: {}", install.started_at));
            }

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);

            // Settings
            ui.heading("Configuration");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Server URL:");
                ui.text_edit_singleline(&mut self.server_url);
            });

            ui.horizontal(|ui| {
                ui.label("Download Folder:");
                ui.text_edit_singleline(&mut self.download_folder);
                if ui.button("📂").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.download_folder = path.to_string_lossy().to_string();
                    }
                }
            });

            ui.add_space(10.0);

            if ui.checkbox(&mut self.run_on_startup, "🚀 Run on Windows startup").changed() {
                if self.run_on_startup {
                    add_to_startup();
                } else {
                    remove_from_startup();
                }
            }

            ui.add_space(10.0);

            // Pause/Resume
            let is_paused = self.state.runtime.block_on(async {
                *self.state.is_paused.read().await
            });

            if ui.button(if is_paused { "▶ Resume Installations" } else { "⏸ Pause Installations" }).clicked() {
                self.state.runtime.spawn({
                    let is_paused_arc = self.state.is_paused.clone();
                    async move {
                        let mut paused = is_paused_arc.write().await;
                        *paused = !*paused;
                    }
                });
            }

            ui.add_space(20.0);

            // Save button
            if ui.button("💾 Save Settings").clicked() {
                self.save_settings();
            }

            ui.add_space(10.0);
            ui.separator();

            // Info
            ui.add_space(10.0);
            let config = self.state.runtime.block_on(async {
                self.state.config.read().await.clone()
            });
            ui.label(egui::RichText::new(format!("Client ID: {}", config.client.id)).small().weak());
            ui.label(egui::RichText::new(format!("Version: {}", env!("CARGO_PKG_VERSION"))).small().weak());
        });
    }
}

impl SettingsWindow {
    fn save_settings(&mut self) {
        let state = self.state.clone();
        let url = self.server_url.clone();
        let folder = self.download_folder.clone();

        self.state.runtime.spawn(async move {
            let mut config = state.config.write().await;
            config.server.url = url;
            config.extraction.output_dir = PathBuf::from(folder);

            if let Err(e) = config.save() {
                error!("Failed to save config: {}", e);
            } else {
                info!("Settings saved");
            }
        });
    }
}

// Windows startup functions
#[cfg(windows)]
fn add_to_startup() {
    let exe_path = std::env::current_exe().unwrap();
    let exe_path_str = format!("\"{}\" --minimized", exe_path.to_string_lossy());

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run_key) = hkcu.open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", KEY_SET_VALUE) {
        if let Err(e) = run_key.set_value("RepackAutoInstaller", &exe_path_str) {
            error!("Failed to add to startup: {}", e);
        } else {
            info!("Added to Windows startup");
        }
    }
}

#[cfg(windows)]
fn remove_from_startup() {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run_key) = hkcu.open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", KEY_SET_VALUE) {
        let _ = run_key.delete_value("RepackAutoInstaller");
        info!("Removed from Windows startup");
    }
}

#[cfg(windows)]
fn is_in_startup() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run_key) = hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") {
        run_key.get_value::<String, _>("RepackAutoInstaller").is_ok()
    } else {
        false
    }
}

#[cfg(not(windows))]
fn add_to_startup() {}
#[cfg(not(windows))]
fn remove_from_startup() {}
#[cfg(not(windows))]
fn is_in_startup() -> bool { false }

// Background tasks
async fn monitor_downloads(state: Arc<AppState>) {
    let mut interval = time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        // Skip if paused
        if *state.is_paused.read().await {
            continue;
        }

        // Check if already installing
        if state.current_installation.read().await.is_some() {
            continue;
        }

        // Get download folder
        let config = state.config.read().await;
        let download_folder = config.extraction.output_dir.clone();
        drop(config);

        // Scan for installers
        if let Ok(entries) = std::fs::read_dir(&download_folder) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Look for repack installers (setup.exe, install.exe, etc.)
                if path.is_file() {
                    let filename = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_lowercase();

                    if filename.contains("setup") || filename.contains("install") {
                        if filename.ends_with(".exe") {
                            info!("Found installer: {:?}", path);

                            // Get game title from parent folder name
                            let game_title = path.parent()
                                .and_then(|p| p.file_name())
                                .and_then(|n| n.to_str())
                                .unwrap_or("Unknown")
                                .to_string();

                            // Start installation
                            start_installation(state.clone(), path, game_title).await;
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn start_installation(state: Arc<AppState>, installer_path: PathBuf, game_title: String) {
    info!("Starting installation: {}", game_title);

    let install_info = InstallationInfo {
        game_title: game_title.clone(),
        installer_path: installer_path.clone(),
        started_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        status: "Installing".to_string(),
    };

    {
        let mut current = state.current_installation.write().await;
        *current = Some(install_info);
    }

    {
        let mut status = state.status.write().await;
        *status = format!("Installing {}", game_title);
    }

    // Run the installer
    let result = tokio::process::Command::new(&installer_path)
        .spawn();

    match result {
        Ok(mut child) => {
            info!("Installer process started: {:?}", installer_path);

            // Wait for installer to complete
            match child.wait().await {
                Ok(exit_status) => {
                    if exit_status.success() {
                        info!("Installation completed: {}", game_title);
                        let mut status = state.status.write().await;
                        *status = format!("✅ Installed {}", game_title);
                    } else {
                        error!("Installation failed with code: {:?}", exit_status.code());
                        let mut status = state.status.write().await;
                        *status = format!("❌ Installation failed: {}", game_title);
                    }
                }
                Err(e) => {
                    error!("Failed to wait for installer: {}", e);
                }
            }
        }
        Err(e) => {
            error!("Failed to start installer: {}", e);
            let mut status = state.status.write().await;
            *status = format!("❌ Error starting installer: {}", e);
        }
    }

    // Clear current installation
    {
        let mut current = state.current_installation.write().await;
        *current = None;
    }
}

async fn register_with_server(state: Arc<AppState>) {
    let config = state.config.read().await;

    if !config.server.enabled {
        return;
    }

    let sys_info = system_info::gather_system_info(
        &config.client.id,
        &config.client.name,
    );

    match state.server_client.register(
        &config.client.id,
        &config.client.name,
        &sys_info.os_version,
    ).await {
        Ok(_) => info!("Registered with server"),
        Err(e) => warn!("Failed to register: {}", e),
    }
}

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("Repack Auto-Installer starting...");

    // Load config
    let mut config = Config::load().unwrap_or_else(|e| {
        error!("Failed to load config: {}", e);
        Config::default()
    });

    // Generate client ID
    let client_id = client_id::generate_or_load_client_id(&mut config);
    info!("Client ID: {}", client_id);

    // Create runtime
    let runtime = Arc::new(Runtime::new().expect("Failed to create runtime"));

    // Create server client
    let server_client = Arc::new(ServerClient::new(config.server.url.clone()));

    // Create app state
    let state = Arc::new(AppState {
        config: Arc::new(RwLock::new(config)),
        server_client,
        runtime: runtime.clone(),
        status: Arc::new(RwLock::new("Idle - waiting for downloads".to_string())),
        current_installation: Arc::new(RwLock::new(None)),
        is_paused: Arc::new(RwLock::new(false)),
    });

    // Register with server
    runtime.spawn({
        let state = state.clone();
        async move {
            register_with_server(state).await;
        }
    });

    // Start background monitor
    runtime.spawn({
        let state = state.clone();
        async move {
            monitor_downloads(state).await;
        }
    });

    // Create GUI
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 400.0])
            .with_min_inner_size([450.0, 350.0])
            .with_title("Repack Auto-Installer"),
        ..Default::default()
    };

    eframe::run_native(
        "Repack Auto-Installer",
        options,
        Box::new(move |_cc| Ok(Box::new(SettingsWindow::new(state)))),
    )
}
