use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::fs;

#[derive(Debug, Clone, PartialEq)]
pub enum ArchiveType {
    Zip,
    SevenZip,
    Rar,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractionProgress {
    /// Current file being extracted
    pub current_file: String,
    /// Number of files extracted so far
    pub files_done: u64,
    /// Total number of files in the archive (0 if unknown)
    pub files_total: u64,
    /// Percentage 0.0 - 100.0
    pub percent: f64,
    /// Human readable status message
    pub message: String,
}

impl Default for ExtractionProgress {
    fn default() -> Self {
        Self {
            current_file: String::new(),
            files_done: 0,
            files_total: 0,
            percent: 0.0,
            message: "Starting extraction...".to_string(),
        }
    }
}

pub struct Extractor {
    /// Shared progress state keyed by download_id
    progress: Arc<RwLock<HashMap<i64, ExtractionProgress>>>,
}

impl Extractor {
    pub fn new() -> Self {
        Self {
            progress: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Detect archive type from file extension
    pub fn get_archive_type(path: &Path) -> Option<ArchiveType> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "zip" => Some(ArchiveType::Zip),
            "7z" => Some(ArchiveType::SevenZip),
            "rar" => Some(ArchiveType::Rar),
            _ => None,
        }
    }

    /// Check if a file is an archive we can extract
    pub fn is_archive(path: &Path) -> bool {
        Self::get_archive_type(path).is_some()
    }

    /// Get current extraction progress for a download
    pub async fn get_progress(&self, download_id: i64) -> Option<ExtractionProgress> {
        let progress = self.progress.read().await;
        progress.get(&download_id).cloned()
    }

    /// Clear progress for a download
    pub async fn clear_progress(&self, download_id: i64) {
        let mut progress = self.progress.write().await;
        progress.remove(&download_id);
    }

    /// Extract an archive to the destination directory with progress tracking.
    /// `download_id` is used to key the progress state.
    /// Returns a list of extracted file paths.
    pub async fn extract_archive(
        &self,
        archive_path: &Path,
        dest_dir: &Path,
        download_id: i64,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
        fs::create_dir_all(dest_dir).await?;

        let archive_type = Self::get_archive_type(archive_path)
            .ok_or_else(|| format!("Unknown archive type: {}", archive_path.display()))?;

        // Initialize progress
        {
            let mut progress = self.progress.write().await;
            progress.insert(download_id, ExtractionProgress {
                message: format!("Preparing to extract {}...", 
                    archive_path.file_name().unwrap_or_default().to_string_lossy()),
                ..Default::default()
            });
        }

        let result = match archive_type {
            ArchiveType::Zip => {
                self.extract_zip(archive_path, dest_dir, download_id).await
            }
            ArchiveType::SevenZip | ArchiveType::Rar => {
                self.extract_with_7zip(archive_path, dest_dir, download_id).await
            }
        };

        // Mark extraction complete or failed in progress
        {
            let mut progress = self.progress.write().await;
            if let Some(p) = progress.get_mut(&download_id) {
                match &result {
                    Ok(files) => {
                        p.percent = 100.0;
                        p.message = format!("Extraction complete — {} files", files.len());
                    }
                    Err(e) => {
                        p.message = format!("Extraction failed: {}", e);
                    }
                }
            }
        }

        result
    }

    /// Extract ZIP files using the zip crate with per-file progress
    async fn extract_zip(
        &self,
        archive_path: &Path,
        dest_dir: &Path,
        download_id: i64,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
        let archive_path = archive_path.to_path_buf();
        let dest_dir = dest_dir.to_path_buf();
        let progress = self.progress.clone();

        tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(&archive_path)?;
            let mut archive = zip::ZipArchive::new(file)?;
            let total = archive.len() as u64;
            let mut extracted_files = Vec::new();

            // Update total count
            {
                let mut prog = progress.blocking_write();
                if let Some(p) = prog.get_mut(&download_id) {
                    p.files_total = total;
                    p.message = format!("Extracting {} files...", total);
                }
            }

            for i in 0..archive.len() {
                let mut file = archive.by_index(i)?;
                let outpath = dest_dir.join(file.mangled_name());
                let name = file.name().to_string();

                if name.ends_with('/') {
                    std::fs::create_dir_all(&outpath)?;
                } else {
                    if let Some(parent) = outpath.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let mut outfile = std::fs::File::create(&outpath)?;
                    std::io::copy(&mut file, &mut outfile)?;
                    extracted_files.push(outpath);
                }

                // Update progress every file (or every 10 if there are tons)
                let files_done = (i + 1) as u64;
                let should_update = total < 100 || files_done % 10 == 0 || files_done == total;

                if should_update {
                    let pct = if total > 0 {
                        (files_done as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    };

                    let short_name = short_filename(&name);
                    let mut prog = progress.blocking_write();
                    if let Some(p) = prog.get_mut(&download_id) {
                        p.files_done = files_done;
                        p.percent = pct;
                        p.current_file = short_name.clone();
                        p.message = format!(
                            "Extracting {}/{} — {}",
                            files_done, total, short_name
                        );
                    }
                }
            }

            Ok(extracted_files)
        })
        .await?
    }

    /// Extract RAR/7z files by shelling out to 7-Zip with progress parsing
    async fn extract_with_7zip(
        &self,
        archive_path: &Path,
        dest_dir: &Path,
        download_id: i64,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
        let seven_zip = find_7zip().ok_or(
            "7-Zip not found. Please install 7-Zip to extract RAR/7z files. \
             Download from https://www.7-zip.org/"
        )?;

        // First: list the archive to get total file count
        let list_output = tokio::process::Command::new(&seven_zip)
            .arg("l")
            .arg("-ba")  // bare format, no headers
            .arg(archive_path.as_os_str())
            .output()
            .await?;

        let total_files = if list_output.status.success() {
            let stdout = String::from_utf8_lossy(&list_output.stdout);
            stdout.lines().filter(|l| !l.trim().is_empty()).count() as u64
        } else {
            0
        };

        {
            let mut progress = self.progress.write().await;
            if let Some(p) = progress.get_mut(&download_id) {
                p.files_total = total_files;
                p.message = if total_files > 0 {
                    format!("Extracting {} files...", total_files)
                } else {
                    "Extracting...".to_string()
                };
            }
        }

        // Run extraction with progress output enabled (-bsp1 sends progress to stdout)
        use tokio::io::AsyncBufReadExt;

        let mut child = tokio::process::Command::new(&seven_zip)
            .arg("x")
            .arg(format!("-o{}", dest_dir.display()))
            .arg("-y")
            .arg("-bsp1")  // Enable progress output to stdout
            .arg("-bb1")   // Show names of extracted files
            .arg(archive_path.as_os_str())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take()
            .ok_or("Failed to capture 7-Zip stdout")?;

        let progress = self.progress.clone();
        let parse_handle = tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut files_done: u64 = 0;

            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // 7zip progress lines look like:
                //   "  0% - filename.ext"      (percentage based)
                //   " 45%"                      (just percentage)
                //   "- filename.ext"            (file being extracted)
                //   "+ filename.ext"            (file extracted)

                let mut updated = false;

                // Try to parse percentage from lines like " 45%" or "  0% - file.ext"
                if let Some(pct) = parse_7zip_percent(trimmed) {
                    let mut prog = progress.write().await;
                    if let Some(p) = prog.get_mut(&download_id) {
                        p.percent = pct;
                        updated = true;
                    }
                }

                // Try to extract filename from "- filename" lines
                if trimmed.starts_with("- ") || trimmed.starts_with("+ ") {
                    let fname = &trimmed[2..];
                    if !fname.is_empty() {
                        files_done += 1;
                        let short = short_filename(fname);
                        let mut prog = progress.write().await;
                        if let Some(p) = prog.get_mut(&download_id) {
                            p.files_done = files_done;
                            p.current_file = short.clone();
                            // If we don't have a percentage from 7zip, estimate from file count
                            if total_files > 0 && !updated {
                                p.percent = (files_done as f64 / total_files as f64) * 100.0;
                            }
                            p.message = format!(
                                "Extracting {}{} — {}",
                                files_done,
                                if total_files > 0 { format!("/{}", total_files) } else { String::new() },
                                short
                            );
                        }
                    }
                }
            }
        });

        let status = child.wait().await?;
        // Wait for the stdout parser to finish
        let _ = parse_handle.await;

        if !status.success() {
            return Err("7-Zip extraction failed".into());
        }

        let extracted_files = collect_files(dest_dir).await?;
        Ok(extracted_files)
    }
}

/// Parse a percentage from 7-Zip output lines like " 45%" or "  0% - file.ext"
fn parse_7zip_percent(line: &str) -> Option<f64> {
    // Look for a pattern like "XX%" at the start of the line
    let trimmed = line.trim();
    // Find first '%' character
    if let Some(pct_pos) = trimmed.find('%') {
        let before = trimmed[..pct_pos].trim();
        // The part before % might be just a number, or might have other chars
        // Take the last numeric token
        let num_str: String = before.chars().rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .chars().rev().collect();
        if let Ok(pct) = num_str.parse::<f64>() {
            if (0.0..=100.0).contains(&pct) {
                return Some(pct);
            }
        }
    }
    None
}

/// Shorten a filename/path for display (just show the last component, truncated)
fn short_filename(name: &str) -> String {
    let fname = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if fname.len() > 50 {
        format!("{}...{}", &fname[..25], &fname[fname.len()-20..])
    } else {
        fname.to_string()
    }
}

/// Find the 7-Zip executable
fn find_7zip() -> Option<String> {
    let common_paths = [
        r"C:\Program Files\7-Zip\7z.exe",
        r"C:\Program Files (x86)\7-Zip\7z.exe",
    ];

    for path in &common_paths {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    if which_7z().is_some() {
        return Some("7z".to_string());
    }

    None
}

/// Check if 7z is available in PATH
fn which_7z() -> Option<()> {
    std::process::Command::new("7z")
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .filter(|s| s.success())
        .map(|_| ())
}

/// Recursively collect all files in a directory
async fn collect_files(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
    let mut files = Vec::new();
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            let sub_files = Box::pin(collect_files(&path)).await?;
            files.extend(sub_files);
        } else {
            files.push(path);
        }
    }

    Ok(files)
}
