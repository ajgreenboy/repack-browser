use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionProgress {
    pub file_path: String,
    pub total_bytes: u64,
    pub extracted_bytes: u64,
    pub progress_percent: f64,
    pub speed_mbps: f64,
    pub eta_seconds: u64,
    pub status: ExtractionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractionStatus {
    Extracting,
    Verifying,
    Completed,
    Failed,
}

pub struct Extractor {
    progress: Arc<RwLock<ExtractionProgress>>,
}

impl Extractor {
    pub fn new(file_path: String) -> Self {
        Self {
            progress: Arc::new(RwLock::new(ExtractionProgress {
                file_path,
                total_bytes: 0,
                extracted_bytes: 0,
                progress_percent: 0.0,
                speed_mbps: 0.0,
                eta_seconds: 0,
                status: ExtractionStatus::Extracting,
            })),
        }
    }

    pub fn get_progress(&self) -> Arc<RwLock<ExtractionProgress>> {
        self.progress.clone()
    }

    pub async fn extract_zip(
        &self,
        archive_path: &Path,
        output_dir: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let file = File::open(archive_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        // Calculate total size
        let total_size: u64 = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok())
            .map(|f| f.size())
            .sum();

        {
            let mut prog = self.progress.write().await;
            prog.total_bytes = total_size;
        }

        let mut extracted: u64 = 0;
        let start_time = std::time::Instant::now();

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = output_dir.join(file.name());

            if file.name().ends_with('/') {
                std::fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    std::fs::create_dir_all(p)?;
                }

                let mut outfile = File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }

            extracted += file.size();

            // Update progress
            {
                let mut prog = self.progress.write().await;
                prog.extracted_bytes = extracted;
                prog.progress_percent = (extracted as f64 / total_size as f64) * 100.0;

                let elapsed = start_time.elapsed().as_secs_f64();
                if elapsed > 0.0 {
                    let speed_bps = extracted as f64 / elapsed;
                    prog.speed_mbps = speed_bps / 1024.0 / 1024.0;

                    let remaining_bytes = total_size - extracted;
                    prog.eta_seconds = (remaining_bytes as f64 / speed_bps) as u64;
                }
            }
        }

        {
            let mut prog = self.progress.write().await;
            prog.status = ExtractionStatus::Completed;
            prog.progress_percent = 100.0;
        }

        Ok(())
    }

    pub async fn extract_7z(
        &self,
        archive_path: &Path,
        output_dir: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Use sevenz-rust for 7z extraction
        sevenz_rust::decompress_file(archive_path, output_dir)
            .map_err(|e| format!("7z extraction failed: {}", e))?;

        {
            let mut prog = self.progress.write().await;
            prog.status = ExtractionStatus::Completed;
            prog.progress_percent = 100.0;
        }

        Ok(())
    }

    pub async fn extract(
        &self,
        archive_path: &Path,
        output_dir: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(output_dir)?;

        let extension = archive_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "zip" => self.extract_zip(archive_path, output_dir).await,
            "7z" => self.extract_7z(archive_path, output_dir).await,
            _ => Err(format!("Unsupported archive format: {}", extension).into()),
        }
    }

    pub async fn verify_md5(
        &self,
        directory: &Path,
        expected_md5: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        {
            let mut prog = self.progress.write().await;
            prog.status = ExtractionStatus::Verifying;
        }

        // Calculate MD5 of all files in directory
        let mut files: Vec<PathBuf> = WalkDir::new(directory)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();

        files.sort();

        let mut hasher = md5::Context::new();

        for file_path in files {
            let mut file = File::open(&file_path)?;
            let mut buffer = [0; 8192];

            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hasher.consume(&buffer[..n]);
            }
        }

        let digest = format!("{:x}", hasher.compute());
        let matches = digest.eq_ignore_ascii_case(expected_md5);

        {
            let mut prog = self.progress.write().await;
            prog.status = if matches {
                ExtractionStatus::Completed
            } else {
                ExtractionStatus::Failed
            };
        }

        Ok(matches)
    }
}
