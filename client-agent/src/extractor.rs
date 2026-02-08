use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

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

    pub async fn extract_zip(
        &self,
        archive_path: &Path,
        output_dir: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let file = File::open(archive_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        // Calculate total size
        let mut total_size: u64 = 0;
        for i in 0..archive.len() {
            if let Ok(file) = archive.by_index(i) {
                total_size += file.size();
            }
        }

        {
            let mut prog = self.progress.write().await;
            prog.total_bytes = total_size;
        }

        let mut extracted: u64 = 0;
        let start_time = std::time::Instant::now();

        for i in 0..archive.len() {
            let file_size = {
                let mut file = archive.by_index(i)?;
                let outpath = output_dir.join(file.name());

                if file.name().ends_with('/') {
                    std::fs::create_dir_all(&outpath)?;
                    0
                } else {
                    if let Some(p) = outpath.parent() {
                        std::fs::create_dir_all(p)?;
                    }

                    let mut outfile = File::create(&outpath)?;
                    std::io::copy(&mut file, &mut outfile)?;
                    file.size()
                }
            }; // Drop file here before await

            extracted += file_size;

            // Update progress (file is dropped, safe to await)
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

    pub async fn extract_rar(
        &self,
        archive_path: &Path,
        output_dir: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Use 7-Zip command line for RAR extraction (7z.exe can extract RAR files)
        // Try multiple common 7-Zip installation paths
        let seven_zip_paths = vec![
            r"C:\Program Files\7-Zip\7z.exe",
            r"C:\Program Files (x86)\7-Zip\7z.exe",
            "7z.exe", // In PATH
        ];

        let mut seven_zip_exe = None;
        for path in &seven_zip_paths {
            if std::path::Path::new(path).exists() || path == &"7z.exe" {
                seven_zip_exe = Some(path);
                break;
            }
        }

        let seven_zip_exe = seven_zip_exe
            .ok_or("7-Zip not found. Please install 7-Zip from https://www.7-zip.org/")?;

        // Run 7z.exe x <archive> -o<output_dir> -y
        let output = tokio::process::Command::new(seven_zip_exe)
            .arg("x")  // Extract with full paths
            .arg(archive_path)
            .arg(format!("-o{}", output_dir.display()))
            .arg("-y")  // Yes to all prompts
            .output()
            .await
            .map_err(|e| format!("Failed to run 7-Zip: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("7-Zip extraction failed: {}", stderr).into());
        }

        {
            let mut prog = self.progress.write().await;
            prog.status = ExtractionStatus::Completed;
            prog.progress_percent = 100.0;
        }

        Ok(())
    }

}

// Standalone helper functions for simple extraction without progress tracking

pub async fn extract_zip(
    archive_path: &Path,
    output_dir: &Path,
) -> Result<(), String> {
    let extractor = Extractor::new(archive_path.to_string_lossy().to_string());
    extractor.extract_zip(archive_path, output_dir).await
        .map_err(|e| e.to_string())
}

pub async fn extract_7z(
    archive_path: &Path,
    output_dir: &Path,
) -> Result<(), String> {
    let extractor = Extractor::new(archive_path.to_string_lossy().to_string());
    extractor.extract_7z(archive_path, output_dir).await
        .map_err(|e| e.to_string())
}

pub async fn extract_rar(
    archive_path: &Path,
    output_dir: &Path,
) -> Result<(), String> {
    let extractor = Extractor::new(archive_path.to_string_lossy().to_string());
    extractor.extract_rar(archive_path, output_dir).await
        .map_err(|e| e.to_string())
}
