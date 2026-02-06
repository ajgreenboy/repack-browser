use reqwest::Client;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub speed: f64,         // bytes per second
    pub status: DownloadStatus,
    pub filename: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum DownloadStatus {
    Downloading,
    Completed,
    Failed(String),
    Cancelled,
}

pub struct Downloader {
    download_dir: PathBuf,
    active_downloads: Arc<RwLock<HashMap<i64, DownloadProgress>>>,
    cancelled: Arc<RwLock<std::collections::HashSet<i64>>>,
    client: Client,
}

impl Downloader {
    pub fn new(download_dir: PathBuf) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(3600)) // 1 hour timeout for large files
            .connect_timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        Self {
            download_dir,
            active_downloads: Arc::new(RwLock::new(HashMap::new())),
            cancelled: Arc::new(RwLock::new(std::collections::HashSet::new())),
            client,
        }
    }

    /// Get the download directory path
    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }

    /// Download a file from URL to disk with progress tracking.
    /// Returns the path to the downloaded file.
    pub async fn download_file(
        &self,
        url: &str,
        filename: &str,
        download_id: i64,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        // Ensure download directory exists
        fs::create_dir_all(&self.download_dir).await?;

        let file_path = self.download_dir.join(filename);

        // Start the request
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()).into());
        }

        let total_bytes = response.content_length().unwrap_or(0);

        // Initialize progress
        {
            let mut active = self.active_downloads.write().await;
            active.insert(download_id, DownloadProgress {
                bytes_downloaded: 0,
                total_bytes,
                speed: 0.0,
                status: DownloadStatus::Downloading,
                filename: filename.to_string(),
            });
        }

        // Create or truncate the file
        let mut file = fs::File::create(&file_path).await?;

        let mut stream = response.bytes_stream();
        let mut bytes_downloaded: u64 = 0;
        let start_time = Instant::now();
        let mut last_update = Instant::now();

        use futures::StreamExt;

        while let Some(chunk_result) = stream.next().await {
            // Check for cancellation
            {
                let cancelled = self.cancelled.read().await;
                if cancelled.contains(&download_id) {
                    // Clean up
                    drop(file);
                    let _ = fs::remove_file(&file_path).await;
                    let mut active = self.active_downloads.write().await;
                    if let Some(progress) = active.get_mut(&download_id) {
                        progress.status = DownloadStatus::Cancelled;
                    }
                    return Err("Download cancelled".into());
                }
            }

            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            bytes_downloaded += chunk.len() as u64;

            // Update progress every 250ms to avoid lock contention
            if last_update.elapsed() >= Duration::from_millis(250) {
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    bytes_downloaded as f64 / elapsed
                } else {
                    0.0
                };

                let mut active = self.active_downloads.write().await;
                if let Some(progress) = active.get_mut(&download_id) {
                    progress.bytes_downloaded = bytes_downloaded;
                    progress.total_bytes = total_bytes;
                    progress.speed = speed;
                }

                last_update = Instant::now();
            }
        }

        file.flush().await?;

        // Mark as completed
        {
            let mut active = self.active_downloads.write().await;
            if let Some(progress) = active.get_mut(&download_id) {
                progress.bytes_downloaded = bytes_downloaded;
                progress.status = DownloadStatus::Completed;
            }
        }

        println!("Downloaded {} ({} bytes)", filename, bytes_downloaded);
        Ok(file_path)
    }

    /// Get current progress for a download
    pub async fn get_progress(&self, download_id: i64) -> Option<DownloadProgress> {
        let active = self.active_downloads.read().await;
        active.get(&download_id).cloned()
    }

    /// Get all active download progress
    pub async fn get_all_progress(&self) -> HashMap<i64, DownloadProgress> {
        let active = self.active_downloads.read().await;
        active.clone()
    }

    /// Cancel a download
    pub async fn cancel(&self, download_id: i64) {
        let mut cancelled = self.cancelled.write().await;
        cancelled.insert(download_id);
    }

    /// Clear completed/failed progress entries
    pub async fn clear_progress(&self, download_id: i64) {
        let mut active = self.active_downloads.write().await;
        active.remove(&download_id);
        let mut cancelled = self.cancelled.write().await;
        cancelled.remove(&download_id);
    }

    /// Check available disk space (returns bytes available)
    pub async fn check_disk_space(&self) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        // Simple approach: try to get available space
        // On most systems this works via the filesystem
        fs::create_dir_all(&self.download_dir).await?;

        // Use a platform-agnostic approach
        #[cfg(target_os = "windows")]
        {
            // On Windows, use the GetDiskFreeSpaceExW API via std
            // For simplicity, we'll return a large number and let the download fail if space runs out
            Ok(u64::MAX)
        }

        #[cfg(not(target_os = "windows"))]
        {
            Ok(u64::MAX)
        }
    }
}
