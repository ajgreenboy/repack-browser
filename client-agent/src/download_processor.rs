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

/// URL-decode and sanitize filename for Windows
fn sanitize_filename(filename: &str) -> String {
    // First, URL-decode the filename to handle %20, %28, etc.
    let decoded = urlencoding::decode(filename)
        .unwrap_or(std::borrow::Cow::Borrowed(filename))
        .to_string();

    // Then replace Windows invalid characters: < > : " / \ | ? *
    decoded
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

pub async fn poll_and_process_downloads(
    server_client: Arc<ServerClient>,
    client_id: &str,
    output_dir: &Path,
    poll_interval_secs: u64,
) {
    let mut interval = time::interval(Duration::from_secs(poll_interval_secs));
    let downloader = Arc::new(Downloader::new());

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
    server_client: &Arc<ServerClient>,
    downloader: &Arc<Downloader>,
    download: crate::server_client::DownloadQueueItem,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let download_id = download.id;
    let game_title = download.game_title.clone();

    // Ensure output directory exists
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir)?;
        info!("Created output directory: {:?}", output_dir);
    }

    // Step 1: Download files
    info!("Starting download for: {}", game_title);
    report_progress(server_client, download_id, "downloading", 0.0, None, None, None).await?;

    let mut downloaded_files = Vec::new();
    let total_files = download.direct_urls.len();

    for (idx, url) in download.direct_urls.iter().enumerate() {
        info!("Downloading file {}/{}", idx + 1, total_files);

        // Extract filename from URL and sanitize it
        let default_name = format!("file_{}.bin", idx);
        let filename = url.split('/').last()
            .unwrap_or(&default_name)
            .split('?').next()
            .unwrap_or(&default_name);

        // Sanitize filename for Windows (remove invalid characters)
        let sanitized_filename = sanitize_filename(filename);

        let file_path = output_dir.join(&sanitized_filename);
        info!("Downloading to: {:?}", file_path);

        // Download file with retry and backoff
        let max_retries = 3;
        let mut last_error = String::new();
        let mut success = false;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                let delay_secs = 5u64 * (1 << (attempt - 1)); // 5, 10, 20 seconds
                warn!("Retry {}/{} for {} (waiting {}s)", attempt, max_retries, filename, delay_secs);
                tokio::time::sleep(Duration::from_secs(delay_secs)).await;
            }

            // Clone references for progress monitoring task
            let downloader_clone = Arc::clone(downloader);
            let server_client_clone = Arc::clone(server_client);
            let current_idx = idx;

            // Spawn a task to report progress during download
            let progress_task = tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(2));
                loop {
                    interval.tick().await;
                    if let Some(dl_progress) = downloader_clone.get_progress().await {
                        let speed_str = if dl_progress.speed_bytes_per_sec > 0.0 {
                            Some(crate::downloader::format_speed(dl_progress.speed_bytes_per_sec))
                        } else {
                            None
                        };

                        let eta_str = if dl_progress.eta_seconds > 0 {
                            Some(crate::downloader::format_eta(dl_progress.eta_seconds))
                        } else {
                            None
                        };

                        let file_progress = if dl_progress.total_bytes > 0 {
                            (dl_progress.downloaded_bytes as f64 / dl_progress.total_bytes as f64) * 100.0
                        } else {
                            0.0
                        };

                        // Overall progress across all files
                        let overall_progress = ((current_idx as f64 + (file_progress / 100.0)) / total_files as f64) * 100.0;

                        let _ = report_progress(
                            &server_client_clone,
                            download_id,
                            "downloading",
                            overall_progress,
                            speed_str,
                            eta_str,
                            None,
                        ).await;
                    }
                }
            });

            match downloader.download_file(url, &file_path).await {
                Ok(_) => {
                    // Stop progress reporting task
                    progress_task.abort();

                    info!("Downloaded: {}", filename);
                    downloaded_files.push(file_path.clone());

                    // Update progress - file complete
                    let progress = ((idx + 1) as f64 / total_files as f64) * 100.0;
                    report_progress(
                        server_client,
                        download_id,
                        "downloading",
                        progress,
                        None,
                        None,
                        None,
                    ).await?;
                    success = true;
                    break;
                }
                Err(e) => {
                    // Stop progress reporting task
                    progress_task.abort();

                    last_error = format!("{}", e);
                    error!("Download attempt {} failed for {}: {}", attempt + 1, filename, e);
                }
            }
        }

        if !success {
            report_progress(
                server_client,
                download_id,
                "failed",
                0.0,
                None,
                None,
                Some(format!("Download failed after {} retries: {}", max_retries, last_error)),
            ).await?;
            return Err(last_error.into());
        }
    }

    // Step 2: Extract archives
    info!("Download complete. Starting extraction for: {}", game_title);
    report_progress(server_client, download_id, "extracting", 0.0, None, None, None).await?;

    // Sanitize the game title for use as a directory name
    let sanitized_game_title = sanitize_filename(&game_title);
    let extract_dir = output_dir.join(&sanitized_game_title);
    std::fs::create_dir_all(&extract_dir)?;
    info!("Extracting to directory: {:?}", extract_dir);

    for file_path in &downloaded_files {
        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();

            if ext_lower == "zip" || ext_lower == "7z" || ext_lower == "rar" {
                info!("Extracting: {:?}", file_path);

                let extract_result = match ext_lower.as_str() {
                    "zip" => crate::extractor::extract_zip(file_path, &extract_dir).await,
                    "7z" => crate::extractor::extract_7z(file_path, &extract_dir).await,
                    "rar" => crate::extractor::extract_rar(file_path, &extract_dir).await,
                    _ => unreachable!(),
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
    info!("Running silent installation with elevation: {:?}", installer_path);

    // Determine install directory (same parent as installer)
    let install_dir = installer_path
        .parent()
        .unwrap_or(Path::new("C:\\Games"))
        .to_string_lossy()
        .to_string();

    // Build command line arguments for FitGirl installer (InnoSetup)
    let args = format!(
        "/VERYSILENT /DIR=\"{}\" /LANG=english /NOCANCEL /NORESTART",
        install_dir
    );

    // Run installer with UAC elevation on Windows
    #[cfg(windows)]
    {
        run_elevated_process(installer_path, &args).await?;
    }

    #[cfg(not(windows))]
    {
        return Err("Installation is only supported on Windows".to_string());
    }

    Ok(())
}

#[cfg(windows)]
async fn run_elevated_process(exe_path: &Path, args: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use winapi::um::shellapi::ShellExecuteW;
    use winapi::um::winuser::SW_SHOW;

    let exe_path_str = exe_path.to_string_lossy().to_string();

    // Convert strings to wide (UTF-16) for Windows API
    let operation: Vec<u16> = OsStr::new("runas").encode_wide().chain(once(0)).collect();
    let file: Vec<u16> = OsStr::new(&exe_path_str).encode_wide().chain(once(0)).collect();
    let parameters: Vec<u16> = OsStr::new(args).encode_wide().chain(once(0)).collect();

    // Run in a blocking task since ShellExecuteW is synchronous
    let result = tokio::task::spawn_blocking(move || {
        unsafe {
            let result = ShellExecuteW(
                ptr::null_mut(),           // hwnd
                operation.as_ptr(),         // lpOperation - "runas" for elevation
                file.as_ptr(),              // lpFile - executable path
                parameters.as_ptr(),        // lpParameters - command line args
                ptr::null(),                // lpDirectory - use current
                SW_SHOW,                    // nShowCmd - show window
            );

            // ShellExecuteW returns a value > 32 if successful
            if result as usize > 32 {
                Ok(())
            } else {
                Err(format!("ShellExecuteW failed with code: {}", result as i32))
            }
        }
    })
    .await
    .map_err(|e| format!("Failed to spawn elevated process task: {}", e))?;

    result?;

    // Wait for installation to complete
    // Note: ShellExecuteW doesn't wait for the process to complete, so we need to poll
    info!("Installer launched with elevation. Waiting for completion...");

    // Wait for installer process to finish by checking if setup.exe is still running
    let exe_name = exe_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("setup.exe");

    // Poll every 5 seconds to check if installer is still running
    for _ in 0..360 { // Max 30 minutes (360 * 5 seconds)
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        // Check if installer process is still running
        if !is_process_running(exe_name) {
            info!("Installer process completed");
            return Ok(());
        }
    }

    Err("Installation timeout - process did not complete within 30 minutes".to_string())
}

#[cfg(windows)]
fn is_process_running(process_name: &str) -> bool {
    use std::process::Command;

    // Use tasklist to check if process is running
    let output = Command::new("tasklist")
        .arg("/FI")
        .arg(format!("IMAGENAME eq {}", process_name))
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.contains(process_name)
    } else {
        false
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
