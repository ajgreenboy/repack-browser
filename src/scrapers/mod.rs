use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod fitgirl;
pub mod steamrip;
pub mod registry;

/// Type of download link
#[derive(Debug, Clone, PartialEq)]
pub enum LinkType {
    Magnet,     // BitTorrent magnet link
    DirectDL,   // Direct download link for Real-Debrid
}

/// A scraped game with all metadata
#[derive(Debug, Clone)]
pub struct ScrapedGame {
    pub title: String,
    pub source: String,            // Source identifier ("fitgirl", "steamrip")
    pub file_size: String,
    pub download_link: String,     // Magnet or DDL
    pub link_type: LinkType,       // Distinguish link types
    pub genres: Option<String>,
    pub company: Option<String>,
    pub original_size: Option<String>,
    pub thumbnail_url: Option<String>,
    pub screenshots: Option<String>,
    pub source_url: Option<String>,
    pub post_date: Option<String>,
}

/// Shared progress state for scraping
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScrapeProgress {
    pub phase: String,
    pub pages_found: i64,
    pub games_scraped: i64,
    pub games_total: i64,
    pub progress: f64,
    pub message: String,
    // Metadata counters
    pub with_thumbnail: i64,
    pub with_genres: i64,
    pub with_company: i64,
    pub with_original_size: i64,
    pub magnets_found: i64,
    pub posts_without_magnet: i64,
}

impl Default for ScrapeProgress {
    fn default() -> Self {
        Self {
            phase: "fetching_pages".to_string(),
            pages_found: 0,
            games_scraped: 0,
            games_total: 0,
            progress: 0.0,
            message: "Starting...".to_string(),
            with_thumbnail: 0,
            with_genres: 0,
            with_company: 0,
            with_original_size: 0,
            magnets_found: 0,
            posts_without_magnet: 0,
        }
    }
}

/// Trait for game scrapers
#[async_trait]
pub trait GameScraper: Send + Sync {
    /// Scrape all games from this source
    async fn scrape_all_games(
        &self,
        progress: Arc<RwLock<ScrapeProgress>>
    ) -> Result<Vec<ScrapedGame>, Box<dyn std::error::Error>>;

    /// Get the internal source name (e.g., "fitgirl", "steamrip")
    fn source_name(&self) -> &'static str;

    /// Get the human-readable source label (e.g., "FitGirl Repacks", "SteamRIP")
    fn source_label(&self) -> &'static str;
}
