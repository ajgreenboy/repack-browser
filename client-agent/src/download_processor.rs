/// New download processor - handles the full download workflow
/// 1. Poll server for pending downloads
/// 2. Download files using direct URLs
/// 3. Extract archives
/// 4. Install game
/// 5. Report progress at each step

use crate::downloader::Downloader;
use crate::server_client::{ProgressUpdate, ServerClient};
use log::{error, info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::time::{self, Duration};

pub async fn poll_and_process_downloads(
    server_client: Arc<ServerClient>,
    client_id: &str,
    output_dir: &Path,
    poll_interval_secs: u64,
) {
    let mut interval = time::interval(Duration::from_secs(poll_interval_secs));
    let downloader = Arc::new(Downloader::new(output_dir.to_path_buf()));

    loop {
        interval.tick().await;

        // Poll server for pending downloads
        match server_client.get_download_queue(client_id).await {
            Ok(queue) => {
                for download in queue {
                    if download.status != "pending" {
                        continue;  // Skip non-pending downloads
                    }

                    if download.direct_urls.is_empty() {
                        error!("Download {} has no direct URLs", download.id);
                        continue;
                    }

                    info!("Processing download: {} (ID: {})", download.game_title, download.id);

                    // Process this download
                    if let Err(e) = process_single_download(
                        &server_client,
                        &downloader,
                        download,
                        output_dir,
                    ).await {
                        error!("Failed to process download: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to poll download queue: {}", e);
            }
        }
    }
}

async fn process_single_download(
    server_client: &ServerClient,
    downloader: &Downloader,
    download: crate::server_client::DownloadQueueItem,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let download_id = download.id;
    let game_title = download.game_title.clone();

    // Step 1: Download files
    info!("Starting download for: {}", game_title);
    report_progress(server_client, download_id, "downloading", 0.0, None, None, None).await?;

    let mut downloaded_files = Vec::new();

    for (idx, url) in download.direct_urls.iter().enumerate() {
        info!("Downloading file {}/{}", idx + 1, download.direct_urls.len());

        // Extract filename from URL
        let filename = url.split('/').last()
            .unwrap_or(&format!("file_{}.bin", idx))
            .split('?').next()
            .unwrap_or(&format!("file_{}.bin", idx));

        let file_path = output_dir.join(filename);

        // Download file
        match downloader.download_file(url, &file_path).await {
            Ok(_) => {
                info!("Downloaded: {}", filename);
                downloaded_files.push(file_path);

                // Update progress
                let progress = ((idx + 1) as f64 / download.direct_urls.len() as f64) * 100.0;
                report_progress(
                    server_client,
                    download_id,
                    "downloading",
                    progress,
                    None,
                    None,
                    None,
                ).await?;
            }
            Err(e) => {
                error!("Failed to download {}: {}", filename, e);
                report_progress(
                    server_client,
                    download_id,
                    "failed",
                    0.0,
                    None,
                    None,
                    Some(format!("Download failed: {}", e)),
                ).await?;
                return Err(e);
            }
        }
    }

    // Step 2: Extract archives
    info!("Download complete. Starting extraction for: {}", game_title);
    report_progress(server_client, download_id, "extracting", 0.0, None, None, None).await?;

    let extract_dir = output_dir.join(&game_title);
    std::fs::create_dir_all(&extract_dir)?;

    for file_path in &downloaded_files {
        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();

            if ext_lower == "zip" || ext_lower == "7z" {
                info!("Extracting: {:?}", file_path);

                let extract_result = if ext_lower == "zip" {
                    crate::extractor::extract_zip(file_path, &extract_dir).await
                } else {
                    crate::extractor::extract_7z(file_path, &extract_dir).await
                };

                match extract_result {
                    Ok(_) => {
                        info!("Extracted: {:?}", file_path);
                    }
                    Err(e) => {
                        error!("Extraction failed: {}", e);
                        report_progress(
                            server_client,
                            download_id,
                            "failed",
                            0.0,
                            None,
                            None,
                            Some(format!("Extraction failed: {}", e)),
                        ).await?;
                        return Err(e.into());
                    }
                }
            }
        }
    }

    // Step 3: Find and run installer
    info!("Extraction complete. Looking for installer: {}", game_title);
    report_progress(server_client, download_id, "installing", 0.0, None, None, None).await?;

    // Look for setup.exe in extracted folder
    let installer_path = find_installer(&extract_dir)?;

    info!("Found installer: {:?}", installer_path);

    // Run silent installation
    match run_silent_install(&installer_path).await {
        Ok(_) => {
            info!("Installation complete: {}", game_title);
            report_progress(server_client, download_id, "completed", 100.0, None, None, None).await?;

            // Show notification
            #[cfg(windows)]
            tokio::task::spawn_blocking(move || {
                show_notification(
                    "Installation Complete",
                    &format!("{} has been installed successfully!", game_title),
                );
            });
        }
        Err(e) => {
            error!("Installation failed: {}", e);
            report_progress(
                server_client,
                download_id,
                "failed",
                0.0,
                None,
                None,
                Some(format!("Installation failed: {}", e)),
            ).await?;
            return Err(e.into());
        }
    }

    Ok(())
}

async fn report_progress(
    server_client: &ServerClient,
    download_id: i64,
    status: &str,
    progress: f64,
    download_speed: Option<String>,
    eta: Option<String>,
    error_message: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let update = ProgressUpdate {
        status: status.to_string(),
        progress,
        download_speed,
        eta,
        error_message,
    };

    server_client.update_download_progress(download_id, &update).await
}

fn find_installer(dir: &Path) -> Result<PathBuf, String> {
    // Look for setup.exe, install.exe, etc.
    let installer_names = vec!["setup.exe", "install.exe", "installer.exe"];

    // First, check root directory
    for name in &installer_names {
        let path = dir.join(name);
        if path.exists() {
            return Ok(path);
        }
    }

    // If not found, search subdirectories (one level deep)
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                for name in &installer_names {
                    let installer_path = path.join(name);
                    if installer_path.exists() {
                        return Ok(installer_path);
                    }
                }
            }
        }
    }

    Err(format!("No installer found in {:?}", dir))
}

async fn run_silent_install(installer_path: &Path) -> Result<(), String> {
    info!("Running silent installation: {:?}", installer_path);

    // FitGirl repack silent install flags
    let output = tokio::process::Command::new(installer_path)
        .arg("/VERYSILENT")
        .arg("/LANG=english")
        .arg("/NOCANCEL")
        .arg("/NORESTART")
        .output()
        .await
        .map_err(|e| format!("Failed to run installer: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Installer exited with code: {:?}",
            output.status.code()
        ))
    }
}

#[cfg(windows)]
fn show_notification(title: &str, message: &str) {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;

    let title_wide: Vec<u16> = OsStr::new(title).encode_wide().chain(once(0)).collect();
    let message_wide: Vec<u16> = OsStr::new(message).encode_wide().chain(once(0)).collect();

    unsafe {
        use winapi::um::winuser::{MessageBoxW, MB_ICONINFORMATION, MB_OK};
        MessageBoxW(
            std::ptr::null_mut(),
            message_wide.as_ptr(),
            title_wide.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(not(windows))]
fn show_notification(_title: &str, _message: &str) {
    // No-op on non-Windows platforms
}
