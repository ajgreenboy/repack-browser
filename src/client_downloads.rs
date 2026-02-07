/// Client-side download management
/// This module handles the new architecture where clients download to their own PCs
use crate::db;
use crate::realdebrid::RealDebridClient;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct ClientDownloadInfo {
    pub id: i64,
    pub game_id: i64,
    pub game_title: String,
    pub game_size: String,
    pub magnet_link: String,
    pub direct_urls: Vec<String>,
    pub status: String,
    pub progress: f64,
    pub download_speed: Option<String>,
    pub eta: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateDownloadRequest {
    pub game_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct ProgressUpdate {
    pub status: String,  // "downloading", "extracting", "installing", "completed", "failed"
    pub progress: f64,   // 0.0 to 100.0
    pub download_speed: Option<String>,
    pub eta: Option<String>,
    pub error_message: Option<String>,
}

pub struct ClientDownloadManager {
    db: SqlitePool,
    rd_client: Arc<RealDebridClient>,
}

impl ClientDownloadManager {
    pub fn new(db: SqlitePool, rd_client: Arc<RealDebridClient>) -> Self {
        Self { db, rd_client }
    }

    /// Create a new download (called when user clicks download button)
    /// This:
    /// 1. Converts magnet to direct URLs via Real-Debrid
    /// 2. Creates download record with user_id
    /// 3. Returns download ID
    pub async fn create_download(
        &self,
        user_id: i64,
        game_id: i64,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        // Get game info
        let game = db::get_game_by_id(&self.db, game_id).await
            .map_err(|e| format!("Game not found: {}", e))?;

        // Check for duplicate active download
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM downloads
             WHERE game_id = ? AND user_id = ?
             AND status IN ('pending', 'downloading', 'extracting', 'installing')"
        )
        .bind(game_id)
        .bind(user_id)
        .fetch_optional(&self.db)
        .await?;

        if let Some((existing_id,)) = existing {
            return Err(format!(
                "Game '{}' is already in your download queue (ID: {})",
                game.title, existing_id
            ).into());
        }

        // Get fresh RD API key from database settings
        let api_key = db::get_setting(&self.db, "rd_api_key").await
            .map_err(|e| format!("Failed to load RD API key: {}", e))?
            .ok_or("Real-Debrid API key not configured. Please add it in Settings.")?;

        if api_key.is_empty() {
            return Err("Real-Debrid API key is empty. Please configure it in Settings.".into());
        }

        // Create fresh RD client with database API key
        let rd_client = RealDebridClient::new(api_key);

        // Convert magnet to direct URLs via Real-Debrid
        println!("Converting magnet for game '{}'...", game.title);
        let download_links = rd_client.process_link(&game.magnet_link).await
            .map_err(|e| format!("Real-Debrid conversion failed: {}", e))?;

        if download_links.is_empty() {
            return Err("No files found in torrent".into());
        }

        // Extract URLs from DownloadLink structs
        let direct_urls: Vec<String> = download_links.iter()
            .map(|link| link.download_url.clone())
            .collect();

        println!("Got {} direct download URLs", direct_urls.len());

        // Create download record with 'pending' status
        let now = chrono::Utc::now().to_rfc3339();
        let direct_urls_json = serde_json::to_string(&direct_urls)?;

        let result = sqlx::query(
            "INSERT INTO downloads
             (game_id, user_id, status, progress, created_at, file_path)
             VALUES (?, ?, 'pending', 0.0, ?, ?)"
        )
        .bind(game_id)
        .bind(user_id)
        .bind(&now)
        .bind(&direct_urls_json)  // Store direct URLs in file_path field (temp solution)
        .execute(&self.db)
        .await?;

        let download_id = result.last_insert_rowid();
        println!("Created download {} for user {} game '{}'", download_id, user_id, game.title);

        Ok(download_id)
    }

    /// Get pending downloads for a client
    /// Returns downloads where:
    /// - user_id matches the client's user
    /// - status is 'pending', 'downloading', 'extracting', or 'installing'
    pub async fn get_client_queue(
        &self,
        client_id: &str,
    ) -> Result<Vec<ClientDownloadInfo>, Box<dyn std::error::Error + Send + Sync>> {
        // Get client info to find user_id
        let client = db::get_client(&self.db, client_id).await?;

        let user_id = client
            .and_then(|c| c.user_id)
            .ok_or("Client not linked to a user")?;

        // Get pending downloads for this user
        let rows: Vec<db::DownloadRow> = sqlx::query_as(
            "SELECT
                d.id, d.game_id, d.status, d.progress, d.download_speed, d.eta,
                d.file_path, d.installer_path, d.error_message, d.created_at, d.completed_at,
                d.client_id, d.user_id,
                g.title as game_title, g.file_size as game_size
             FROM downloads d
             JOIN games g ON d.game_id = g.id
             WHERE d.user_id = ? AND d.status IN ('pending', 'downloading', 'extracting', 'installing')
             ORDER BY d.created_at ASC"
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await?;

        // Convert to ClientDownloadInfo
        let mut downloads = Vec::new();
        for row in rows {
            // Get game to retrieve magnet link
            let game = db::get_game_by_id(&self.db, row.game_id).await?;

            // Parse direct URLs from file_path (temp storage)
            let direct_urls: Vec<String> = row.file_path
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            downloads.push(ClientDownloadInfo {
                id: row.id,
                game_id: row.game_id,
                game_title: row.game_title,
                game_size: row.game_size,
                magnet_link: game.magnet_link,
                direct_urls,
                status: row.status,
                progress: row.progress,
                download_speed: row.download_speed,
                eta: row.eta,
                error_message: row.error_message,
                created_at: row.created_at,
            });
        }

        Ok(downloads)
    }

    /// Update download progress (called by client)
    pub async fn update_progress(
        &self,
        download_id: i64,
        update: ProgressUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Update download status in database
        sqlx::query(
            "UPDATE downloads
             SET status = ?, progress = ?, download_speed = ?, eta = ?, error_message = ?
             WHERE id = ?"
        )
        .bind(&update.status)
        .bind(update.progress)
        .bind(&update.download_speed)
        .bind(&update.eta)
        .bind(&update.error_message)
        .bind(download_id)
        .execute(&self.db)
        .await?;

        // If completed or failed, set completed_at timestamp
        if update.status == "completed" || update.status == "failed" {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE downloads SET completed_at = ? WHERE id = ?")
                .bind(&now)
                .bind(download_id)
                .execute(&self.db)
                .await?;
        }

        Ok(())
    }

    /// Link a client to a user
    pub async fn link_client_to_user(
        &self,
        client_id: &str,
        user_id: i64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        sqlx::query("UPDATE clients SET user_id = ? WHERE client_id = ?")
            .bind(user_id)
            .bind(client_id)
            .execute(&self.db)
            .await?;

        Ok(())
    }
}
