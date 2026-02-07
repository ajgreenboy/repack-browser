mod client_id;
mod config;
mod extractor;
mod server_client;
mod system_info;

use config::Config;
use eframe::egui;
use extractor::{ExtractionProgress, ExtractionStatus, Extractor};
use log::{error, info};
use rfd::FileDialog;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

struct FitGirlClientApp {
    config: Config,
    runtime: Arc<Runtime>,

    // UI State
    selected_archive: Option<PathBuf>,
    selected_destination: Option<PathBuf>,
    is_extracting: bool,
    current_progress: Option<Arc<RwLock<ExtractionProgress>>>,
    status_message: String,

    // Settings
    show_settings: bool,
    server_url: String,
    run_on_startup: bool,
}

impl Default for FitGirlClientApp {
    fn default() -> Self {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Info)
            .init();

        let config = Config::load().unwrap_or_else(|e| {
            error!("Failed to load config: {}", e);
            Config::default()
        });

        let runtime = Arc::new(Runtime::new().expect("Failed to create Tokio runtime"));

        Self {
            server_url: config.server.url.clone(),
            run_on_startup: is_in_startup(),
            config,
            runtime,
            selected_archive: None,
            selected_destination: None,
            is_extracting: false,
            current_progress: None,
            status_message: "Ready".to_string(),
            show_settings: false,
        }
    }
}

impl eframe::App for FitGirlClientApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("FitGirl Client Agent");

            ui.add_space(10.0);

            // Explanation (2 sentences max)
            ui.label(egui::RichText::new(
                "This app extracts game archives (.zip, .7z) to your chosen location. \
                It needs access to your files to read archives and write extracted files."
            ).italics().color(egui::Color32::GRAY));

            ui.add_space(20.0);

            // Tabs
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.show_settings, false, "📦 Extract");
                ui.selectable_value(&mut self.show_settings, true, "⚙ Settings");
            });

            ui.separator();
            ui.add_space(10.0);

            if self.show_settings {
                self.show_settings_tab(ui);
            } else {
                self.show_extract_tab(ui, ctx);
            }
        });
    }
}

impl FitGirlClientApp {
    fn show_extract_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // File picker
        ui.horizontal(|ui| {
            ui.label("Archive:");
            if ui.button("📂 Select File...").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("Archives", &["zip", "7z"])
                    .pick_file()
                {
                    self.selected_archive = Some(path);
                }
            }
        });

        if let Some(ref archive) = self.selected_archive {
            ui.label(format!("  → {}", archive.display()));
        } else {
            ui.label(egui::RichText::new("  No file selected").italics().weak());
        }

        ui.add_space(10.0);

        // Destination picker
        ui.horizontal(|ui| {
            ui.label("Extract to:");
            if ui.button("📁 Select Folder...").clicked() {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.selected_destination = Some(path);
                }
            }
        });

        if let Some(ref dest) = self.selected_destination {
            ui.label(format!("  → {}", dest.display()));
        } else {
            ui.label(egui::RichText::new("  No folder selected").italics().weak());
        }

        ui.add_space(20.0);

        // Extract button
        let can_extract = self.selected_archive.is_some()
            && self.selected_destination.is_some()
            && !self.is_extracting;

        if ui.add_enabled(can_extract, egui::Button::new("▶ Extract")).clicked() {
            self.start_extraction(ctx);
        }

        ui.add_space(20.0);

        // Progress
        if let Some(ref progress) = self.current_progress {
            let runtime = self.runtime.clone();
            let prog = runtime.block_on(async {
                progress.read().await.clone()
            });

            ui.separator();
            ui.add_space(10.0);

            ui.label(egui::RichText::new(&self.status_message).strong());

            let progress_fraction = prog.progress_percent / 100.0;
            let progress_bar = egui::ProgressBar::new(progress_fraction as f32)
                .text(format!("{:.1}%", prog.progress_percent));
            ui.add(progress_bar);

            ui.label(format!(
                "Speed: {:.2} MB/s | ETA: {}s | {}/{} bytes",
                prog.speed_mbps,
                prog.eta_seconds,
                prog.extracted_bytes,
                prog.total_bytes
            ));

            // Check if extraction is complete
            if matches!(prog.status, ExtractionStatus::Completed | ExtractionStatus::Failed) {
                self.is_extracting = false;

                if matches!(prog.status, ExtractionStatus::Completed) {
                    self.status_message = "✅ Extraction completed!".to_string();
                } else {
                    self.status_message = "❌ Extraction failed!".to_string();
                }

                self.current_progress = None;
            }

            // Request repaint for smooth progress updates
            ctx.request_repaint();
        } else if !self.is_extracting {
            ui.label(egui::RichText::new(&self.status_message).color(egui::Color32::DARK_GRAY));
        }
    }

    fn show_settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(10.0);

        // Server URL
        ui.horizontal(|ui| {
            ui.label("Server URL:");
            ui.text_edit_singleline(&mut self.server_url);
        });

        ui.add_space(10.0);

        // Startup option
        if ui.checkbox(&mut self.run_on_startup, "🚀 Run on Windows startup").changed() {
            if self.run_on_startup {
                add_to_startup();
            } else {
                remove_from_startup();
            }
        }

        ui.add_space(20.0);

        // Save button
        if ui.button("💾 Save Settings").clicked() {
            self.config.server.url = self.server_url.clone();
            if let Err(e) = self.config.save() {
                error!("Failed to save config: {}", e);
                self.status_message = format!("Failed to save settings: {}", e);
            } else {
                self.status_message = "Settings saved!".to_string();
                info!("Settings saved");
            }
        }

        ui.add_space(20.0);
        ui.separator();

        // Info
        ui.label(format!("Client ID: {}", self.config.client.id));
        ui.label(format!("Client Name: {}", self.config.client.name));
        ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
    }

    fn start_extraction(&mut self, ctx: &egui::Context) {
        let archive = self.selected_archive.clone().unwrap();
        let destination = self.selected_destination.clone().unwrap();

        self.is_extracting = true;
        self.status_message = "Starting extraction...".to_string();

        let extractor = Arc::new(Extractor::new(archive.to_string_lossy().to_string()));
        let progress = extractor.get_progress();
        self.current_progress = Some(progress.clone());

        // Spawn extraction task
        let runtime = self.runtime.clone();
        let archive_clone = archive.clone();
        let destination_clone = destination.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            runtime.block_on(async move {
                info!("Starting extraction: {:?} -> {:?}", archive_clone, destination_clone);

                match extractor.extract(&archive_clone, &destination_clone).await {
                    Ok(_) => {
                        info!("Extraction completed successfully");
                    }
                    Err(e) => {
                        error!("Extraction failed: {}", e);
                    }
                }

                // Request final UI update
                ctx_clone.request_repaint();
            });
        });
    }
}

// Windows startup functions
#[cfg(windows)]
fn add_to_startup() {
    let exe_path = std::env::current_exe().unwrap();
    let exe_path_str = exe_path.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run_key) = hkcu.open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", KEY_SET_VALUE) {
        if let Err(e) = run_key.set_value("FitGirlClient", &exe_path_str.as_ref()) {
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
        if let Err(e) = run_key.delete_value("FitGirlClient") {
            error!("Failed to remove from startup: {}", e);
        } else {
            info!("Removed from Windows startup");
        }
    }
}

#[cfg(windows)]
fn is_in_startup() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run_key) = hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") {
        run_key.get_value::<String, _>("FitGirlClient").is_ok()
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

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 500.0])
            .with_min_inner_size([500.0, 400.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(&include_bytes!("../assets/icon.png")[..])
                    .unwrap_or_default(),
            ),
        ..Default::default()
    };

    eframe::run_native(
        "FitGirl Client Agent",
        options,
        Box::new(|_cc| Ok(Box::new(FitGirlClientApp::default()))),
    )
}
