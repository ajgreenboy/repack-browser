use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub speed_bytes_per_sec: f64,
    pub eta_seconds: u64,
}

pub struct Downloader {
    client: reqwest::Client,
    progress: Arc<RwLock<Option<DownloadProgress>>>,
}

impl Downloader {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                // No overall timeout - downloads can be very large
                .build()
                .expect("Failed to build HTTP client"),
            progress: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_progress(&self) -> Option<DownloadProgress> {
        self.progress.read().await.clone()
    }

    /// Download a file from a URL with progress tracking
    pub async fn download_file(
        &self,
        url: &str,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Starting download: {}", url);

        // Create parent directories if needed
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Start download
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()).into());
        }

        let total_bytes = response.content_length().unwrap_or(0);
        log::info!("Content length: {} bytes", total_bytes);

        // Create output file
        let mut file = tokio::fs::File::create(output_path).await?;
        let mut downloaded_bytes: u64 = 0;
        let start_time = std::time::Instant::now();

        // Download in chunks
        let mut stream = response.bytes_stream();
        use futures::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;

            downloaded_bytes += chunk.len() as u64;

            // Update progress
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                downloaded_bytes as f64 / elapsed
            } else {
                0.0
            };

            let eta = if speed > 0.0 && total_bytes > downloaded_bytes {
                ((total_bytes - downloaded_bytes) as f64 / speed) as u64
            } else {
                0
            };

            let progress = DownloadProgress {
                total_bytes,
                downloaded_bytes,
                speed_bytes_per_sec: speed,
                eta_seconds: eta,
            };

            *self.progress.write().await = Some(progress);
        }

        file.flush().await?;
        log::info!("Download completed: {:?}", output_path);

        Ok(())
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

pub fn format_speed(bytes_per_sec: f64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec as u64))
}

pub fn format_eta(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}
