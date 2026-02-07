use md5::{Md5, Digest};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncReadExt;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ValidationResult {
    pub total_files: usize,
    pub validated: usize,
    pub failed: usize,
    pub skipped: usize,
    pub status: String,
    pub files: Vec<FileValidation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileValidation {
    pub filename: String,
    pub status: FileStatus,
    pub expected_hash: Option<String>,
    pub actual_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Valid,
    Invalid,
    Missing,
    Skipped,
}

/// Find MD5 checksum files in a directory
pub async fn find_md5_file(dir: &Path) -> Option<PathBuf> {
    let common_names = vec![
        "checksums.md5",
        "md5.txt",
        "MD5.txt",
        "checksum.md5",
        "hashes.md5",
    ];

    // First check for common names
    for name in common_names {
        let path = dir.join(name);
        if path.exists() {
            return Some(path);
        }
    }

    // Then search for any .md5 file
    if let Ok(mut entries) = fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext.eq_ignore_ascii_case("md5") {
                        return Some(path);
                    }
                }
            }
        }
    }

    None
}

/// Parse an MD5 file and return a map of filename -> hash
async fn parse_md5_file(path: &Path) -> Result<Vec<(String, String)>, Box<dyn std::error::Error + Send + Sync>> {
    let content = fs::read_to_string(path).await?;
    let mut checksums = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        // Support multiple formats:
        // 1. "hash *filename" or "hash  filename" (standard md5sum format)
        // 2. "hash filename"
        // 3. "filename hash"

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let (hash, filename) = if parts[0].len() == 32 && parts[0].chars().all(|c| c.is_ascii_hexdigit()) {
            // Format: hash filename
            let hash = parts[0].to_lowercase();
            let filename = parts[1..].join(" ").trim_start_matches('*').to_string();
            (hash, filename)
        } else if parts.len() >= 2 && parts.last().unwrap().len() == 32 {
            // Format: filename hash (less common)
            let hash = parts.last().unwrap().to_lowercase();
            let filename = parts[..parts.len()-1].join(" ");
            (hash, filename)
        } else {
            continue;
        };

        checksums.push((filename, hash));
    }

    Ok(checksums)
}

/// Calculate MD5 hash of a file
async fn calculate_md5(path: &Path) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut file = fs::File::open(path).await?;
    let mut hasher = Md5::new();
    let mut buffer = vec![0u8; 8192];

    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Validate files in a directory against an MD5 checksum file
pub async fn validate_directory(dir: &Path) -> Result<ValidationResult, Box<dyn std::error::Error + Send + Sync>> {
    let md5_file = find_md5_file(dir).await
        .ok_or("No MD5 checksum file found in directory")?;

    println!("Found MD5 file: {}", md5_file.display());

    let checksums = parse_md5_file(&md5_file).await?;

    if checksums.is_empty() {
        return Err("No checksums found in MD5 file".into());
    }

    println!("Validating {} files...", checksums.len());

    let mut results = Vec::new();
    let mut validated = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for (filename, expected_hash) in checksums {
        // Try to find the file (might be in subdirectories)
        let file_path = if let Some(path) = find_file(dir, &filename).await {
            path
        } else {
            results.push(FileValidation {
                filename: filename.clone(),
                status: FileStatus::Missing,
                expected_hash: Some(expected_hash),
                actual_hash: None,
            });
            failed += 1;
            continue;
        };

        // Skip if it's the MD5 file itself
        if file_path == md5_file {
            results.push(FileValidation {
                filename,
                status: FileStatus::Skipped,
                expected_hash: Some(expected_hash),
                actual_hash: None,
            });
            skipped += 1;
            continue;
        }

        println!("  Validating: {}", filename);

        match calculate_md5(&file_path).await {
            Ok(actual_hash) => {
                let is_valid = actual_hash == expected_hash;
                results.push(FileValidation {
                    filename,
                    status: if is_valid { FileStatus::Valid } else { FileStatus::Invalid },
                    expected_hash: Some(expected_hash),
                    actual_hash: Some(actual_hash),
                });
                if is_valid {
                    validated += 1;
                } else {
                    failed += 1;
                }
            }
            Err(e) => {
                eprintln!("  Error calculating hash for {}: {}", filename, e);
                results.push(FileValidation {
                    filename,
                    status: FileStatus::Invalid,
                    expected_hash: Some(expected_hash),
                    actual_hash: None,
                });
                failed += 1;
            }
        }
    }

    let status = if failed > 0 {
        format!("{} files valid, {} failed", validated, failed)
    } else {
        format!("All {} files valid", validated)
    };

    Ok(ValidationResult {
        total_files: results.len(),
        validated,
        failed,
        skipped,
        status,
        files: results,
    })
}

/// Recursively find a file by name in a directory (up to 3 levels deep)
async fn find_file(dir: &Path, filename: &str) -> Option<PathBuf> {
    find_file_recursive(dir, filename, 0, 3).await
}

fn find_file_recursive<'a>(dir: &'a Path, filename: &'a str, depth: usize, max_depth: usize) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<PathBuf>> + Send + 'a>> {
    Box::pin(async move {
        if depth > max_depth {
            return None;
        }

        let mut entries = fs::read_dir(dir).await.ok()?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();

            if path.is_file() {
                if let Some(name) = path.file_name() {
                    if name.to_string_lossy() == filename {
                        return Some(path);
                    }
                }
            } else if path.is_dir() {
                if let Some(found) = find_file_recursive(&path, filename, depth + 1, max_depth).await {
                    return Some(found);
                }
            }
        }

        None
    })
}
