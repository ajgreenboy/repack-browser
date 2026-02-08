mod client_id;
mod config;
mod downloader;
mod download_processor;  // New download processor for full workflow
mod extractor;
mod local_server;
mod realdebrid;
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
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

// Shared app state
struct AppState {
    config: Arc<RwLock<Config>>,
    server_client: Arc<ServerClient>,
    local_server_state: Arc<local_server::LocalServerState>,
    runtime: Arc<Runtime>,
    status: Arc<RwLock<String>>,
    current_installation: Arc<RwLock<Option<InstallationInfo>>>,
    is_paused: Arc<RwLock<bool>>,
}

#[derive(Clone)]
struct InstallationInfo {
    game_title: String,
    #[allow(dead_code)]
    installer_path: PathBuf,
    started_at: String,
    #[allow(dead_code)]
    status: String,
}

struct SettingsWindow {
    state: Arc<AppState>,
    server_url: String,
    download_folder: String,
    run_on_startup: bool,
    #[allow(dead_code)]
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
/// Show a Windows notification (using message box for now)
#[cfg(windows)]
fn show_notification(title: &str, message: &str) {
    use winapi::um::winuser::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

    let title_wide: Vec<u16> = OsStr::new(title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let message_wide: Vec<u16> = OsStr::new(message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message_wide.as_ptr(),
            title_wide.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(not(windows))]
fn show_notification(title: &str, message: &str) {
    println!("[NOTIFICATION] {}: {}", title, message);
}

/// Extract a ZIP file
async fn extract_zip(file_path: &std::path::Path, output_dir: &std::path::Path) -> Result<(), String> {

    let file = std::fs::File::open(file_path)
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read ZIP: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| format!("Failed to read file {}: {}", i, e))?;

        let outpath = match file.enclosed_name() {
            Some(path) => output_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            std::fs::create_dir_all(&outpath)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(p)
                        .map_err(|e| format!("Failed to create parent directory: {}", e))?;
                }
            }
            let mut outfile = std::fs::File::create(&outpath)
                .map_err(|e| format!("Failed to create output file: {}", e))?;
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("Failed to extract file: {}", e))?;
        }
    }

    Ok(())
}

/// Extract a 7Z file
async fn extract_7z(file_path: &std::path::Path, output_dir: &std::path::Path) -> Result<(), String> {
    sevenz_rust::decompress_file(file_path, output_dir)
        .map_err(|e| format!("Failed to extract 7z: {}", e))
}

/// Process downloads queued from the browser
async fn process_download_queue(state: Arc<AppState>) {
    let mut interval = time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        // Check if there are pending downloads from the browser
        let pending = {
            let mut queue = state.local_server_state.pending_downloads.write().await;
            if queue.is_empty() {
                continue;
            }
            queue.drain(..).collect::<Vec<_>>()
        };

        for download_req in pending {
            info!("Processing download request for game_id: {}", download_req.game_id);

            // Clone state for async move
            let state_clone = state.clone();
            let game_id = download_req.game_id;

            // Spawn download task
            tokio::spawn(async move {
                if let Err(e) = process_single_download(state_clone, game_id).await {
                    error!("Download failed: {}", e);
                    show_notification(
                        "Download Failed",
                        &format!("Failed to download game: {}", e)
                    );
                }
            });
        }
    }
}

/// Process a single download from start to finish
async fn process_single_download(state: Arc<AppState>, game_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Step 1: Fetch game info from server
    info!("Fetching game info for ID: {}", game_id);

    let config = state.config.read().await;
    let server_url = config.server.url.clone();
    let rd_api_key = config.realdebrid.api_key.clone();
    let rd_enabled = config.realdebrid.enabled;
    let output_dir = config.extraction.output_dir.clone();
    drop(config);

    // Update status
    {
        let mut status = state.status.write().await;
        *status = format!("Fetching game info...");
    }

    let client = reqwest::Client::new();
    let game_response = client
        .get(&format!("{}/api/games/{}", server_url, game_id))
        .send()
        .await?;

    if !game_response.status().is_success() {
        return Err(format!("Failed to fetch game info: {}", game_response.status()).into());
    }

    let game: serde_json::Value = game_response.json().await?;
    let game_title = game["title"].as_str().unwrap_or("Unknown").to_string();
    let magnet_link = game["magnet_link"].as_str().unwrap_or("").to_string();

    info!("Game: {}", game_title);
    info!("Magnet/Link: {}", magnet_link);

    // Step 2: Convert magnet link via Real-Debrid
    let download_urls = if rd_enabled && !rd_api_key.is_empty() {
        {
            let mut status = state.status.write().await;
            *status = format!("Converting link via Real-Debrid...");
        }

        show_notification(
            "Processing Download",
            &format!("Converting {} via Real-Debrid...", game_title)
        );

        let rd_client = realdebrid::RealDebridClient::new(rd_api_key);

        if magnet_link.starts_with("magnet:") {
            rd_client.convert_magnet(&magnet_link).await?
        } else {
            // It's a direct download link
            vec![rd_client.unrestrict_link(&magnet_link).await?]
        }
    } else {
        return Err("Real-Debrid is not configured. Please set your API key in the config.".into());
    };

    if download_urls.is_empty() {
        return Err("No download URLs available".into());
    }

    info!("Got {} download URLs", download_urls.len());

    // Step 3: Download files
    show_notification(
        "Download Started",
        &format!("Downloading {} ({} files)...", game_title, download_urls.len())
    );

    let downloader = downloader::Downloader::new();
    let game_folder = output_dir.join(&game_title);
    tokio::fs::create_dir_all(&game_folder).await?;

    for (i, url) in download_urls.iter().enumerate() {
        // Extract filename from URL
        let filename = url
            .split('/')
            .last()
            .and_then(|s| s.split('?').next())
            .unwrap_or(&format!("file_{}.bin", i))
            .to_string();

        let file_path = game_folder.join(&filename);

        info!("Downloading {}/{}: {}", i + 1, download_urls.len(), filename);

        {
            let mut status = state.status.write().await;
            *status = format!("Downloading {} ({}/{})", filename, i + 1, download_urls.len());
        }

        // Download with progress tracking
        downloader.download_file(url, &file_path).await?;
    }

    // Step 4: Show completion notification
    show_notification(
        "Download Complete",
        &format!("{} has been downloaded! Starting extraction...", game_title)
    );

    // Step 5: Extract archives
    {
        let mut status = state.status.write().await;
        *status = format!("Extracting {}...", game_title);
    }

    // Find archives in the downloaded folder
    let entries = tokio::fs::read_dir(&game_folder).await?;
    let mut entries = entries;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.is_file() {
            let extension = path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());

            match extension.as_deref() {
                Some("zip") => {
                    info!("Extracting ZIP: {:?}", path);
                    extract_zip(&path, &output_dir).await?;
                }
                Some("7z") => {
                    info!("Extracting 7Z: {:?}", path);
                    extract_7z(&path, &output_dir).await?;
                }
                _ => continue,
            }
        }
    }

    // Step 6: Show extraction complete
    show_notification(
        "Extraction Complete",
        &format!("{} is ready! The installer will start automatically.", game_title)
    );

    {
        let mut status = state.status.write().await;
        *status = format!("✅ {} ready for installation", game_title);
    }

    Ok(())
}

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

    // Determine installation directory
    let config = state.config.read().await;
    let install_dir = config.extraction.output_dir.join(&game_title);
    drop(config);

    // Run the installer with silent flags
    // FitGirl repacks use Inno Setup which supports these flags:
    // /VERYSILENT - Completely silent install
    // /LANG=english - Force English language
    // /DIR="path" - Install directory
    // /NOCANCEL - Don't allow cancel
    // /NORESTART - Don't restart PC
    let result = tokio::process::Command::new(&installer_path)
        .arg("/VERYSILENT")
        .arg("/LANG=english")
        .arg(format!("/DIR=\"{}\"", install_dir.display()))
        .arg("/NOCANCEL")
        .arg("/NORESTART")
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

                        // Show success notification
                        let title_clone = game_title.clone();
                        tokio::task::spawn_blocking(move || {
                            show_notification(
                                "Installation Complete",
                                &format!("{} has been installed successfully!", title_clone)
                            );
                        });
                    } else {
                        error!("Installation failed with code: {:?}", exit_status.code());
                        let mut status = state.status.write().await;
                        *status = format!("❌ Installation failed: {}", game_title);

                        // Show error notification
                        let title_clone = game_title.clone();
                        tokio::task::spawn_blocking(move || {
                            show_notification(
                                "Installation Failed",
                                &format!("{} installation failed. Check logs for details.", title_clone)
                            );
                        });
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

    // Create local server state for browser communication
    let local_server_state = Arc::new(local_server::LocalServerState::new());

    // Create app state
    let state = Arc::new(AppState {
        config: Arc::new(RwLock::new(config)),
        server_client,
        local_server_state: local_server_state.clone(),
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

    // Start background monitor for local installers
    runtime.spawn({
        let state = state.clone();
        async move {
            monitor_downloads(state).await;
        }
    });

    // Start NEW download processor - polls server and handles full download workflow
    runtime.spawn({
        let state = state.clone();
        async move {
            loop {
                let config = state.config.read().await;
                if !config.server.enabled {
                    drop(config);
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    continue;
                }

                let client_id = config.client.id.clone();
                let output_dir = config.extraction.output_dir.clone();
                let poll_interval = config.server.poll_interval_secs;
                drop(config);

                download_processor::poll_and_process_downloads(
                    state.server_client.clone(),
                    &client_id,
                    &output_dir,
                    poll_interval,
                ).await;
            }
        }
    });

    // Start local HTTP server for browser communication
    runtime.spawn({
        let local_state = local_server_state.clone();
        async move {
            local_server::start_local_server(local_state).await;
        }
    });

    // Start download processor - processes downloads queued from browser
    runtime.spawn({
        let state = state.clone();
        async move {
            process_download_queue(state).await;
        }
    });

    // Create GUI
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 400.0])
            .with_min_inner_size([450.0, 350.0])
            .with_title("Repack Auto-Installer")
            .with_decorations(true)     // Show window borders and title bar
            .with_resizable(true)        // Allow resizing
            .with_maximized(false)       // Don't start maximized
            .with_taskbar(true),         // Show in taskbar
        ..Default::default()
    };

    eframe::run_native(
        "Repack Auto-Installer",
        options,
        Box::new(move |_cc| Ok(Box::new(SettingsWindow::new(state)))),
    )
}
