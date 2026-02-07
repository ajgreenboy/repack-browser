use crate::db;
use crate::downloader::Downloader;
use crate::extractor::Extractor;
use crate::realdebrid::RealDebridClient;
use serde::Serialize;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize)]
pub struct DownloadInfo {
    pub id: i64,
    pub game_id: i64,
    pub game_title: String,
    pub game_size: String,
    pub status: String,
    pub progress: f64,
    pub download_speed: Option<String>,
    pub eta: Option<String>,
    pub file_path: Option<String>,
    pub installer_path: Option<String>,
    pub error_message: Option<String>,
    pub extract_progress: Option<crate::extractor::ExtractionProgress>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub files: Vec<DownloadFileInfo>,
    pub has_md5: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadFileInfo {
    pub id: i64,
    pub filename: String,
    pub file_size: Option<i64>,
    pub file_path: Option<String>,
    pub is_extracted: bool,
}

#[derive(Clone)]
pub struct DownloadManagerConfig {
    pub auto_extract: bool,
    pub delete_archives: bool,
    pub max_concurrent: usize,
}

impl Default for DownloadManagerConfig {
    fn default() -> Self {
        Self {
            auto_extract: true,
            delete_archives: false,
            max_concurrent: 1,
        }
    }
}

pub struct DownloadManager {
    db: SqlitePool,
    downloader: Arc<Downloader>,
    extractor: Arc<Extractor>,
    rd_client: Arc<RealDebridClient>,
    config: DownloadManagerConfig,
    is_processing: Arc<RwLock<bool>>,
}

impl DownloadManager {
    pub fn new(
        db: SqlitePool,
        downloader: Arc<Downloader>,
        rd_client: Arc<RealDebridClient>,
        config: DownloadManagerConfig,
    ) -> Self {
        Self {
            db,
            downloader,
            extractor: Arc::new(Extractor::new()),
            rd_client,
            config,
            is_processing: Arc::new(RwLock::new(false)),
        }
    }

    /// Add a game to the download queue. Returns the download ID.
    pub async fn queue_download(&self, game_id: i64) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        // Check if game exists
        let game = db::get_game_by_id(&self.db, game_id).await
            .map_err(|e| format!("Game not found: {}", e))?;

        // Check for duplicate (active download of same game)
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM downloads WHERE game_id = ? AND status IN ('queued', 'downloading', 'extracting')"
        )
        .bind(game_id)
        .fetch_optional(&self.db)
        .await?;

        if let Some((existing_id,)) = existing {
            return Err(format!(
                "Game '{}' is already in the download queue (ID: {})",
                game.title, existing_id
            ).into());
        }

        let now = chrono::Utc::now().to_rfc3339();

        let result = sqlx::query(
            "INSERT INTO downloads (game_id, status, progress, created_at) VALUES (?, 'queued', 0.0, ?)"
        )
        .bind(game_id)
        .bind(&now)
        .execute(&self.db)
        .await?;

        let download_id = result.last_insert_rowid();
        println!("Queued download {} for game '{}'", download_id, game.title);

        // Trigger queue processing
        self.try_process_queue().await;

        Ok(download_id)
    }

    /// Trigger queue processing if not already running
    pub async fn try_process_queue(&self) {
        let mut is_processing = self.is_processing.write().await;
        if *is_processing {
            return;
        }
        *is_processing = true;
        drop(is_processing);

        let db = self.db.clone();
        let downloader = self.downloader.clone();
        let extractor = self.extractor.clone();
        let rd_client = self.rd_client.clone();
        let config = self.config.clone();
        let is_processing = self.is_processing.clone();

        tokio::spawn(async move {
            loop {
                // Get next queued download
                let next: Option<(i64, i64)> = sqlx::query_as(
                    "SELECT id, game_id FROM downloads WHERE status = 'queued' ORDER BY created_at ASC LIMIT 1"
                )
                .fetch_optional(&db)
                .await
                .unwrap_or(None);

                let Some((download_id, game_id)) = next else {
                    break;
                };

                // Process this download
                if let Err(e) = process_download(
                    &db,
                    &downloader,
                    &extractor,
                    &rd_client,
                    &config,
                    download_id,
                    game_id,
                ).await {
                    eprintln!("Download {} failed: {}", download_id, e);
                    let _ = update_download_status(&db, download_id, "failed", Some(&e.to_string())).await;
                }

                // Clear downloader progress for this download
                downloader.clear_progress(download_id).await;
            }

            let mut flag = is_processing.write().await;
            *flag = false;
        });
    }

    /// Get all downloads with their info
    pub async fn get_downloads(&self) -> Result<Vec<DownloadInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let rows: Vec<db::DownloadRow> = sqlx::query_as(
            r#"
            SELECT d.id, d.game_id, d.status, d.progress, d.download_speed, d.eta,
                   d.file_path, d.installer_path, d.error_message, d.created_at, d.completed_at,
                   g.title as game_title, g.file_size as game_size, d.client_id
            FROM downloads d
            JOIN games g ON d.game_id = g.id
            ORDER BY d.created_at DESC
            "#
        )
        .fetch_all(&self.db)
        .await?;

        let mut downloads = Vec::new();

        for row in rows {
            // Get download files
            let files: Vec<db::DownloadFileRow> = sqlx::query_as(
                "SELECT id, filename, file_size, file_path, is_extracted FROM download_files WHERE download_id = ?"
            )
            .bind(row.id)
            .fetch_all(&self.db)
            .await
            .unwrap_or_default();

            // Merge with live progress from downloader if actively downloading
            let (progress, speed, eta) = if row.status == "downloading" {
                if let Some(live) = self.downloader.get_progress(row.id).await {
                    let pct = if live.total_bytes > 0 {
                        (live.bytes_downloaded as f64 / live.total_bytes as f64) * 100.0
                    } else {
                        row.progress
                    };
                    let speed_str = format_speed(live.speed);
                    let eta_str = if live.speed > 0.0 && live.total_bytes > live.bytes_downloaded {
                        let remaining_bytes = live.total_bytes - live.bytes_downloaded;
                        let secs = remaining_bytes as f64 / live.speed;
                        Some(format_eta(secs))
                    } else {
                        None
                    };
                    (pct, Some(speed_str), eta_str)
                } else {
                    (row.progress, row.download_speed.clone(), row.eta.clone())
                }
            } else {
                (row.progress, row.download_speed.clone(), row.eta.clone())
            };

            // Merge extraction progress if extracting
            let extract_progress = if row.status == "extracting" {
                self.extractor.get_progress(row.id).await
            } else {
                None
            };

            // Check if MD5 file exists for completed downloads
            let has_md5 = if let Some(ref path) = row.file_path {
                if row.status == "completed" || row.status == "installed" {
                    let dir = std::path::Path::new(path);
                    crate::md5_validator::find_md5_file(dir).await.is_some()
                } else {
                    false
                }
            } else {
                false
            };

            downloads.push(DownloadInfo {
                id: row.id,
                game_id: row.game_id,
                game_title: row.game_title,
                game_size: row.game_size,
                status: row.status,
                progress,
                download_speed: speed,
                eta,
                file_path: row.file_path,
                installer_path: row.installer_path,
                error_message: row.error_message,
                extract_progress,
                created_at: row.created_at,
                completed_at: row.completed_at,
                files: files.into_iter().map(|f| DownloadFileInfo {
                    id: f.id,
                    filename: f.filename,
                    file_size: f.file_size,
                    file_path: f.file_path,
                    is_extracted: f.is_extracted,
                }).collect(),
                has_md5,
            });
        }

        Ok(downloads)
    }

    /// Get downloads assigned to a specific client that are ready for extraction
    /// Returns downloads with status 'completed' (downloaded but not extracted yet)
    pub async fn get_client_queue(&self, client_id: &str) -> Result<Vec<DownloadInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let rows: Vec<db::DownloadRow> = sqlx::query_as(
            r#"
            SELECT d.id, d.game_id, d.status, d.progress, d.download_speed, d.eta,
                   d.file_path, d.installer_path, d.error_message, d.created_at, d.completed_at,
                   g.title as game_title, g.file_size as game_size, d.client_id
            FROM downloads d
            JOIN games g ON d.game_id = g.id
            WHERE d.client_id = ? AND d.status IN ('completed', 'extracting')
            ORDER BY d.created_at ASC
            "#
        )
        .bind(client_id)
        .fetch_all(&self.db)
        .await?;

        let mut downloads = Vec::new();

        for row in rows {
            // Get download files
            let files: Vec<db::DownloadFileRow> = sqlx::query_as(
                "SELECT id, filename, file_size, file_path, is_extracted FROM download_files WHERE download_id = ?"
            )
            .bind(row.id)
            .fetch_all(&self.db)
            .await
            .unwrap_or_default();

            // Get extraction progress if extracting
            let extract_progress = if row.status == "extracting" {
                self.extractor.get_progress(row.id).await
            } else {
                None
            };

            // Check if MD5 file exists
            let has_md5 = if let Some(ref path) = row.file_path {
                let dir = std::path::Path::new(path);
                crate::md5_validator::find_md5_file(dir).await.is_some()
            } else {
                false
            };

            downloads.push(DownloadInfo {
                id: row.id,
                game_id: row.game_id,
                game_title: row.game_title,
                game_size: row.game_size,
                status: row.status,
                progress: row.progress,
                download_speed: row.download_speed.clone(),
                eta: row.eta.clone(),
                file_path: row.file_path,
                installer_path: row.installer_path,
                error_message: row.error_message,
                extract_progress,
                created_at: row.created_at,
                completed_at: row.completed_at,
                files: files.into_iter().map(|f| DownloadFileInfo {
                    id: f.id,
                    filename: f.filename,
                    file_size: f.file_size,
                    file_path: f.file_path,
                    is_extracted: f.is_extracted,
                }).collect(),
                has_md5,
            });
        }

        Ok(downloads)
    }

    /// Get a single download's info
    pub async fn get_download(&self, download_id: i64) -> Result<DownloadInfo, Box<dyn std::error::Error + Send + Sync>> {
        let downloads = self.get_downloads().await?;
        downloads.into_iter()
            .find(|d| d.id == download_id)
            .ok_or_else(|| "Download not found".into())
    }

    /// Cancel a download
    pub async fn cancel_download(&self, download_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Signal cancellation to the downloader
        self.downloader.cancel(download_id).await;

        // Update DB status
        update_download_status(&self.db, download_id, "failed", Some("Cancelled by user")).await?;

        Ok(())
    }

    /// Remove a download record (only completed/failed)
    pub async fn remove_download(&self, download_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let status: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM downloads WHERE id = ?"
        )
        .bind(download_id)
        .fetch_optional(&self.db)
        .await?;

        match status {
            Some((s,)) if s == "completed" || s == "failed" => {
                sqlx::query("DELETE FROM download_files WHERE download_id = ?")
                    .bind(download_id)
                    .execute(&self.db)
                    .await?;
                sqlx::query("DELETE FROM downloads WHERE id = ?")
                    .bind(download_id)
                    .execute(&self.db)
                    .await?;
                Ok(())
            }
            Some((s,)) => Err(format!("Cannot remove download with status '{}'. Cancel it first.", s).into()),
            None => Err("Download not found".into()),
        }
    }

    /// Retry a failed download
    pub async fn retry_download(&self, download_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let status: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM downloads WHERE id = ?"
        )
        .bind(download_id)
        .fetch_optional(&self.db)
        .await?;

        match status {
            Some((s,)) if s == "failed" => {
                // Reset to queued
                sqlx::query(
                    "UPDATE downloads SET status = 'queued', progress = 0.0, error_message = NULL, download_speed = NULL, eta = NULL WHERE id = ?"
                )
                .bind(download_id)
                .execute(&self.db)
                .await?;

                // Remove old file records
                sqlx::query("DELETE FROM download_files WHERE download_id = ?")
                    .bind(download_id)
                    .execute(&self.db)
                    .await?;

                // Trigger processing
                self.try_process_queue().await;
                Ok(())
            }
            Some((s,)) => Err(format!("Cannot retry download with status '{}'", s).into()),
            None => Err("Download not found".into()),
        }
    }

    /// Launch the installer for a completed download.
    /// Opens the setup executable so the user can click through the install wizard.
    pub async fn launch_installer(&self, download_id: i64) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT status, installer_path FROM downloads WHERE id = ?"
        )
        .bind(download_id)
        .fetch_optional(&self.db)
        .await?;

        match row {
            Some((status, Some(installer))) if status == "completed" => {
                let path = std::path::Path::new(&installer);
                if !path.exists() {
                    return Err(format!("Installer not found at: {}", installer).into());
                }

                println!("Launching installer: {}", installer);

                // Launch the installer as a detached process
                // On Windows this will trigger UAC if the installer needs admin
                #[cfg(target_os = "windows")]
                {
                    // Use cmd /C start to properly detach and handle UAC
                    tokio::process::Command::new("cmd")
                        .args(&["/C", "start", "", &installer])
                        .spawn()
                        .map_err(|e| format!("Failed to launch installer: {}", e))?;
                }

                #[cfg(not(target_os = "windows"))]
                {
                    // On Linux/Mac, just try to execute it (unlikely scenario for FitGirl repacks)
                    tokio::process::Command::new(&installer)
                        .spawn()
                        .map_err(|e| format!("Failed to launch installer: {}", e))?;
                }

                // Update status to indicate installation was launched
                let _ = sqlx::query(
                    "UPDATE downloads SET status = 'installing' WHERE id = ?"
                )
                .bind(download_id)
                .execute(&self.db)
                .await;

                Ok(installer)
            }
            Some((status, None)) if status == "completed" => {
                Err("No installer found for this download. You may need to browse the folder manually.".into())
            }
            Some((status, _)) => {
                Err(format!("Cannot install: download status is '{}'", status).into())
            }
            None => Err("Download not found".into()),
        }
    }

    /// Mark an installing download back to completed (user finished or cancelled install)
    pub async fn mark_installed(&self, download_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let status: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM downloads WHERE id = ?"
        )
        .bind(download_id)
        .fetch_optional(&self.db)
        .await?;

        match status {
            Some((s,)) if s == "installing" || s == "completed" => {
                sqlx::query("UPDATE downloads SET status = 'installed' WHERE id = ?")
                    .bind(download_id)
                    .execute(&self.db)
                    .await?;
                Ok(())
            }
            Some((s,)) => Err(format!("Cannot mark as installed: status is '{}'", s).into()),
            None => Err("Download not found".into()),
        }
    }

    /// Scan /mnt/storage/games for existing game directories and import them as downloads
    pub async fn scan_existing_games(&self) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let download_dir = self.downloader.download_dir();
        let mut entries = tokio::fs::read_dir(&download_dir).await?;
        let mut imported = 0;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Skip if not a directory
            if !path.is_dir() {
                continue;
            }

            let dir_name = match path.file_name() {
                Some(name) => name.to_string_lossy().to_string(),
                None => continue,
            };

            // Check if this directory is already tracked in downloads
            let existing: Option<(i64,)> = sqlx::query_as(
                "SELECT id FROM downloads WHERE file_path = ?"
            )
            .bind(path.to_string_lossy().as_ref())
            .fetch_optional(&self.db)
            .await?;

            if existing.is_some() {
                continue; // Already tracked
            }

            // Try to find matching game by title
            let game_id: Option<i64> = {
                // Try exact match first
                let exact: Option<(i64,)> = sqlx::query_as(
                    "SELECT id FROM games WHERE title LIKE ?"
                )
                .bind(format!("%{}%", dir_name))
                .fetch_optional(&self.db)
                .await?;

                exact.map(|(id,)| id)
            };

            // Look for installer
            let installer_path = find_installer(&path).await;

            // Create download record
            let now = chrono::Utc::now().to_rfc3339();
            let result = sqlx::query(
                "INSERT INTO downloads (game_id, status, progress, file_path, installer_path, created_at, completed_at)
                 VALUES (?, 'completed', 100.0, ?, ?, ?, ?)"
            )
            .bind(game_id.unwrap_or(-1)) // Use -1 for unknown games
            .bind(path.to_string_lossy().as_ref())
            .bind(installer_path.as_ref().map(|p| p.to_string_lossy().to_string()))
            .bind(&now)
            .bind(&now)
            .execute(&self.db)
            .await?;

            println!("Imported existing game: {}", dir_name);
            imported += 1;
        }

        Ok(imported)
    }

    /// Permanently delete a download and its files from disk
    pub async fn delete_download(&self, download_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get download info
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT status, file_path FROM downloads WHERE id = ?"
        )
        .bind(download_id)
        .fetch_optional(&self.db)
        .await?;

        let (status, file_path) = row.ok_or("Download not found")?;

        // Only allow deletion of completed, failed, or installed downloads
        if !["completed", "failed", "installed", "installing"].contains(&status.as_str()) {
            return Err(format!("Cannot delete download with status '{}'. Cancel it first.", status).into());
        }

        // Delete files from disk if path exists
        if let Some(path_str) = file_path {
            let path = std::path::Path::new(&path_str);
            if path.exists() {
                println!("Deleting files at: {}", path.display());
                if path.is_dir() {
                    tokio::fs::remove_dir_all(&path).await?;
                } else {
                    tokio::fs::remove_file(&path).await?;
                }
                println!("Deleted: {}", path.display());
            }
        }

        // Delete download files records
        sqlx::query("DELETE FROM download_files WHERE download_id = ?")
            .bind(download_id)
            .execute(&self.db)
            .await?;

        // Delete download record
        sqlx::query("DELETE FROM downloads WHERE id = ?")
            .bind(download_id)
            .execute(&self.db)
            .await?;

        Ok(())
    }
}

/// Process a single download: RD → download files → extract
async fn process_download(
    db: &SqlitePool,
    downloader: &Downloader,
    extractor: &Extractor,
    rd_client: &RealDebridClient,
    config: &DownloadManagerConfig,
    download_id: i64,
    game_id: i64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let game = db::get_game_by_id(db, game_id).await?;
    println!("Processing download {} for '{}'", download_id, game.title);

    // Update status to downloading
    update_download_status(db, download_id, "downloading", None).await?;

    // Step 1: Process magnet through Real-Debrid
    // Get API key from database (takes priority over env var)
    let api_key = db::get_setting(db, "rd_api_key").await.ok().flatten()
        .filter(|k| !k.is_empty())
        .ok_or("Real-Debrid API key not set. Please configure it in Settings.")?;
    
    // Create RD client with fresh API key from database
    let rd_client = crate::realdebrid::RealDebridClient::new(api_key);

    println!("  Processing download link with Real-Debrid...");
    let rd_downloads = rd_client.process_link(&game.magnet_link).await
        .map_err(|e| format!("Real-Debrid error: {}", e))?;

    if rd_downloads.is_empty() {
        return Err("No download links from Real-Debrid".into());
    }

    println!("  Got {} download links from Real-Debrid", rd_downloads.len());

    // Create a subdirectory for this game
    let safe_title = sanitize_filename(&game.title);
    let game_dir = downloader.download_dir().join(&safe_title);
    tokio::fs::create_dir_all(&game_dir).await?;

    // Step 2: Download each file
    let mut downloaded_files = Vec::new();
    let total_files = rd_downloads.len();

    for (idx, dl) in rd_downloads.iter().enumerate() {
        println!("  Downloading file {}/{}: {}", idx + 1, total_files, dl.filename);

        // Record the file in DB
        let file_result = sqlx::query(
            "INSERT INTO download_files (download_id, filename, file_path) VALUES (?, ?, ?)"
        )
        .bind(download_id)
        .bind(&dl.filename)
        .bind(game_dir.join(&dl.filename).to_string_lossy().as_ref())
        .execute(db)
        .await?;

        let _file_id = file_result.last_insert_rowid();

        // Download the file
        match downloader.download_file(&dl.download_url, &dl.filename, download_id).await {
            Ok(path) => {
                // Update file size
                if let Ok(metadata) = tokio::fs::metadata(&path).await {
                    let _ = sqlx::query(
                        "UPDATE download_files SET file_size = ? WHERE download_id = ? AND filename = ?"
                    )
                    .bind(metadata.len() as i64)
                    .bind(download_id)
                    .bind(&dl.filename)
                    .execute(db)
                    .await;
                }

                // Move file to game directory if it's not already there
                let dest = game_dir.join(&dl.filename);
                if path != dest {
                    if let Err(e) = tokio::fs::rename(&path, &dest).await {
                        // rename might fail cross-device, try copy+delete
                        tokio::fs::copy(&path, &dest).await
                            .map_err(|e2| format!("Failed to move file: rename={}, copy={}", e, e2))?;
                        let _ = tokio::fs::remove_file(&path).await;
                    }
                }

                downloaded_files.push(dest);

                // Update overall progress
                let pct = ((idx + 1) as f64 / total_files as f64) * 100.0;
                let _ = sqlx::query("UPDATE downloads SET progress = ? WHERE id = ?")
                    .bind(pct)
                    .bind(download_id)
                    .execute(db)
                    .await;
            }
            Err(e) => {
                // Check if cancelled
                if e.to_string().contains("cancelled") {
                    return Err(e);
                }
                return Err(format!("Failed to download {}: {}", dl.filename, e).into());
            }
        }
    }

    // Step 3: Extract archives if enabled
    if config.auto_extract {
        let archives: Vec<_> = downloaded_files.iter()
            .filter(|f| crate::extractor::Extractor::is_archive(f))
            .cloned()
            .collect();

        if !archives.is_empty() {
            update_download_status(db, download_id, "extracting", None).await?;
            println!("  Extracting {} archive(s)...", archives.len());

            for archive in &archives {
                match extractor.extract_archive(archive, &game_dir, download_id).await {
                    Ok(extracted) => {
                        println!("  Extracted {} files from {}", extracted.len(), archive.display());

                        // Mark file as extracted
                        let fname = archive.file_name().unwrap_or_default().to_string_lossy();
                        let _ = sqlx::query(
                            "UPDATE download_files SET is_extracted = 1 WHERE download_id = ? AND filename = ?"
                        )
                        .bind(download_id)
                        .bind(fname.as_ref())
                        .execute(db)
                        .await;
                    }
                    Err(e) => {
                        eprintln!("  Warning: Failed to extract {}: {}", archive.display(), e);
                        // Don't fail the whole download for extraction errors
                    }
                }
            }

            // Clear extraction progress
            extractor.clear_progress(download_id).await;

            // Validate extraction: check if any .exe files were extracted
            println!("  Validating extraction...");
            match validate_extraction(&game_dir).await {
                Ok(true) => {
                    println!("  ✓ Extraction validated - found installer executable(s)");
                }
                Ok(false) => {
                    let warning = "Extraction completed but no .exe installer found. Files may still be compressed.";
                    eprintln!("  ⚠ Warning: {}", warning);
                    // Don't fail, but log the issue in case manual intervention is needed
                }
                Err(e) => {
                    eprintln!("  ⚠ Warning: Extraction validation error: {}", e);
                }
            }

            // Delete archives after extraction if configured
            if config.delete_archives {
                for archive in &archives {
                    let _ = tokio::fs::remove_file(archive).await;
                }
            }
        }
    }

    // Step 4: Detect installer executable
    let installer_path = find_installer(&game_dir).await;
    if let Some(ref installer) = installer_path {
        println!("  Found installer: {}", installer.display());
    }

    // Step 5: Mark as completed
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE downloads SET status = 'completed', progress = 100.0, file_path = ?, installer_path = ?, completed_at = ? WHERE id = ?"
    )
    .bind(game_dir.to_string_lossy().as_ref())
    .bind(installer_path.as_ref().map(|p| p.to_string_lossy().to_string()))
    .bind(&now)
    .bind(download_id)
    .execute(db)
    .await?;

    println!("Download {} completed: '{}'", download_id, game.title);
    Ok(())
}

async fn update_download_status(
    db: &SqlitePool,
    download_id: i64,
    status: &str,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    if let Some(err_msg) = error {
        sqlx::query("UPDATE downloads SET status = ?, error_message = ? WHERE id = ?")
            .bind(status)
            .bind(err_msg)
            .bind(download_id)
            .execute(db)
            .await?;
    } else {
        sqlx::query("UPDATE downloads SET status = ? WHERE id = ?")
            .bind(status)
            .bind(download_id)
            .execute(db)
            .await?;
    }
    Ok(())
}

/// Sanitize a string for use as a directory name
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Validate that extraction actually produced executable files.
/// Returns Ok(true) if .exe files are found, Ok(false) if not, Err on filesystem errors.
async fn validate_extraction(dir: &std::path::Path) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    Ok(has_exe_files(dir).await)
}

/// Recursively check if a directory contains any .exe files
async fn has_exe_files(dir: &std::path::Path) -> bool {
    match scan_for_exe(dir, 0, 3).await {
        Ok(found) => found,
        Err(_) => false,
    }
}

/// Recursively scan for .exe files up to a maximum depth
fn scan_for_exe(dir: &std::path::Path, current_depth: usize, max_depth: usize) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool, std::io::Error>> + Send + '_>> {
    Box::pin(async move {
        if current_depth > max_depth {
            return Ok(false);
        }

        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext.eq_ignore_ascii_case("exe") {
                        return Ok(true);
                    }
                }
            } else if path.is_dir() {
                if scan_for_exe(&path, current_depth + 1, max_depth).await? {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    })
}

/// Recursively search a directory for installer executables.
/// Looks for common FitGirl repack installer names.
async fn find_installer(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    // Priority-ordered list of installer filename patterns
    let installer_patterns: &[&str] = &[
        "setup.exe",
        "setup-fitgirl.exe",
        "install.exe",
        "installer.exe",
    ];

    // First pass: check top-level directory for exact matches (most common)
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let lower = name.to_lowercase();
                    // Exact pattern match
                    for pattern in installer_patterns {
                        if lower == *pattern {
                            return Some(path);
                        }
                    }
                    // Broader: any exe starting with "setup"
                    if lower.starts_with("setup") && lower.ends_with(".exe") {
                        candidates.push(path.clone());
                    }
                }
            }
        }
        if let Some(c) = candidates.into_iter().next() {
            return Some(c);
        }
    }

    // Second pass: search one level of subdirectories
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_installer_in_dir(&path, installer_patterns).await {
                    return Some(found);
                }
            }
        }
    }

    None
}

/// Search a single directory (non-recursive) for installer exe files
async fn find_installer_in_dir(dir: &std::path::Path, patterns: &[&str]) -> Option<std::path::PathBuf> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let lower = name.to_lowercase();
                for pattern in patterns {
                    if lower == *pattern {
                        return Some(path);
                    }
                }
                if lower.starts_with("setup") && lower.ends_with(".exe") {
                    candidates.push(path.clone());
                }
            }
        }
    }

    candidates.into_iter().next()
}

fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_000_000_000.0 {
        format!("{:.1} GB/s", bytes_per_sec / 1_000_000_000.0)
    } else if bytes_per_sec >= 1_000_000.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_000_000.0)
    } else if bytes_per_sec >= 1_000.0 {
        format!("{:.1} KB/s", bytes_per_sec / 1_000.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

fn format_eta(seconds: f64) -> String {
    let secs = seconds as u64;
    if secs >= 3600 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}
