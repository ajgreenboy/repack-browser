mod db;
mod downloader;
mod download_manager;
mod extractor;
mod installation_assistant;
mod installation_checker;
mod installation_monitor;
mod md5_validator;
mod rawg;
mod realdebrid;
mod scraper;  // Keep old scraper for backward compatibility during transition
mod scrapers;
mod system_info;

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;
use tower_http::{
    cors::CorsLayer,
    services::ServeDir,
};

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    rd_client: Arc<realdebrid::RealDebridClient>,
    scrape_status: Arc<RwLock<ScrapeStatus>>,
    download_manager: Arc<download_manager::DownloadManager>,
    rawg_api_key: String,
    scraper_registry: Arc<scrapers::registry::ScraperRegistry>,
}

#[derive(Clone, Serialize)]
struct ScrapeStatus {
    is_running: bool,
    #[serde(flatten)]
    progress: scrapers::ScrapeProgress,
    last_result: Option<String>,
    last_completed: Option<String>,
}

impl Default for ScrapeStatus {
    fn default() -> Self {
        Self {
            is_running: false,
            progress: scrapers::ScrapeProgress::default(),
            last_result: None,
            last_completed: None,
        }
    }
}

#[derive(Serialize)]
struct GamesResponse {
    games: Vec<db::Game>,
    total: i64,
    page: i64,
    per_page: i64,
    total_pages: i64,
}

#[derive(Deserialize)]
struct AddMagnetRequest {
    game_id: i64,
}

#[derive(Deserialize)]
struct QueueDownloadRequest {
    game_id: i64,
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    downloads: Option<Vec<realdebrid::DownloadLink>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    download_id: Option<i64>,
}

#[derive(Serialize)]
struct DownloadsResponse {
    downloads: Vec<download_manager::DownloadInfo>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rd_api_key = std::env::var("RD_API_KEY")
        .unwrap_or_else(|_| {
            eprintln!("Warning: RD_API_KEY not set. Real-Debrid integration will not work.");
            String::new()
        });

    let rawg_api_key = std::env::var("RAWG_API_KEY")
        .unwrap_or_else(|_| {
            eprintln!("Warning: RAWG_API_KEY not set. Game images/metadata from RAWG will not be available.");
            eprintln!("  Get a free key at https://rawg.io/apidocs");
            String::new()
        });

    let db_path = std::env::var("DATABASE_PATH")
        .unwrap_or_else(|_| {
            let current_dir = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            
            let data_dir = current_dir.join("data");
            std::fs::create_dir_all(&data_dir).ok();
            
            format!("sqlite:{}?mode=rwc", data_dir.join("games.db").display())
        });
    
    println!("📁 Database location: {}", db_path);
    let db = db::init_db(&db_path).await?;

    // Download configuration from env vars
    let download_dir = std::env::var("DOWNLOAD_DIR")
        .unwrap_or_else(|_| {
            let current_dir = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            current_dir.join("downloads").to_string_lossy().to_string()
        });

    let auto_extract = std::env::var("AUTO_EXTRACT")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);

    let delete_archives = std::env::var("DELETE_ARCHIVES")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    println!("📂 Download directory: {}", download_dir);
    println!("📦 Auto-extract: {}", auto_extract);
    println!("🗑️  Delete archives after extraction: {}", delete_archives);

    let rd_client = Arc::new(realdebrid::RealDebridClient::new(rd_api_key));
    let dl_downloader = Arc::new(downloader::Downloader::new(download_dir.into()));

    let dm_config = download_manager::DownloadManagerConfig {
        auto_extract,
        delete_archives,
        max_concurrent: 1,
    };

    let dm = Arc::new(download_manager::DownloadManager::new(
        db.clone(),
        dl_downloader,
        rd_client.clone(),
        dm_config,
    ));

    // Resume any queued downloads from previous session
    dm.try_process_queue().await;

    // Initialize scraper registry
    let mut scraper_registry = scrapers::registry::ScraperRegistry::new();
    scraper_registry.register(Arc::new(scrapers::fitgirl::FitGirlScraper::new()));
    scraper_registry.register(Arc::new(scrapers::steamrip::SteamRipScraper::new()));
    let scraper_registry = Arc::new(scraper_registry);

    let state = AppState {
        db: db.clone(),
        rd_client,
        scrape_status: Arc::new(RwLock::new(ScrapeStatus::default())),
        download_manager: dm,
        rawg_api_key,
        scraper_registry,
    };

    let frontend_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.join("frontend")))
        .unwrap_or_else(|| std::path::PathBuf::from("./frontend"));
    
    println!("📂 Frontend directory: {}", frontend_dir.display());

    let app = Router::new()
        // Existing routes
        .route("/api/games", get(get_games))
        .route("/api/games/genres", get(get_genres))
        .route("/api/games/random", get(get_random_game))
        .route("/api/games/favorites", get(get_favorites))
        .route("/api/games/favorites/:id", post(add_favorite))
        .route("/api/games/favorites/:id", delete(remove_favorite))
        .route("/api/games/upload", post(upload_csv))
        .route("/api/games/rescrape", post(rescrape))
        .route("/api/scrape-status", get(get_scrape_status))
        .route("/api/sources", get(get_sources))
        .route("/api/realdebrid/add", post(add_to_realdebrid))
        // Download management routes
        .route("/api/downloads", get(get_downloads))
        .route("/api/downloads", post(queue_download))
        .route("/api/downloads/:id", get(get_download_status))
        .route("/api/downloads/:id", delete(cancel_download))
        .route("/api/downloads/:id/retry", post(retry_download))
        .route("/api/downloads/:id/remove", delete(remove_download))
        .route("/api/downloads/:id/install", post(launch_install))
        .route("/api/downloads/:id/installed", post(mark_installed))
        .route("/api/downloads/:id/validate", post(validate_download))
        .route("/api/downloads/:id/delete", delete(delete_download))
        .route("/api/downloads/scan", post(scan_existing_games))
        .route("/api/downloads/files/:file_id", get(download_file))
        // Settings routes
        .route("/api/settings", get(get_settings))
        .route("/api/settings", post(save_settings))
        // System information
        .route("/api/system-info", get(get_system_info))
        .route("/api/pre-install-check/:game_id", get(check_pre_install))
        // Installation assistant
        .route("/api/assistant/actions", post(get_assistant_actions))
        .route("/api/assistant/install-dll", post(assistant_install_dll))
        .route("/api/assistant/add-exclusion", post(assistant_add_exclusion))
        .route("/api/assistant/toggle-av", post(assistant_toggle_av))
        .route("/api/assistant/dependency-info/:dep", get(get_dependency_info))
        // Installation monitoring
        .route("/api/installation/logs/:game_id", get(get_installation_history))
        .route("/api/installation/stats", get(get_installation_stats))
        .route("/api/installation/analyze/:log_id", get(analyze_failed_installation))
        // Client management
        .route("/api/clients/register", post(register_client))
        .route("/api/clients/:client_id/queue", get(get_client_queue))
        .route("/api/clients/:client_id/progress", post(update_client_progress))
        .route("/api/clients/:client_id/system-info", post(update_client_system_info))
        .route("/api/clients", get(get_all_clients))
        // Health check
        .route("/api/health", get(health_check))
        // Static files
        .nest_service("/", ServeDir::new(frontend_dir))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "0.0.0.0:3000";
    println!("🚀 Server running on http://{}", addr);
    println!("📊 Frontend available at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ─── Game endpoints ───

async fn get_games(
    State(state): State<AppState>,
    Query(query): Query<db::GameQuery>,
) -> Result<Json<GamesResponse>, StatusCode> {
    let per_page = query.per_page.unwrap_or(50);
    let page = query.page.unwrap_or(1);

    let (games, total) = db::query_games(&state.db, query)
        .await
        .map_err(|e| {
            eprintln!("Error querying games: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let total_pages = (total as f64 / per_page as f64).ceil() as i64;
    Ok(Json(GamesResponse {
        games,
        total,
        page,
        per_page,
        total_pages,
    }))
}

// ─── Genres ───

async fn get_genres(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let genres = db::get_all_genres(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "genres": genres.into_iter().map(|(name, count)| {
            serde_json::json!({ "name": name, "count": count })
        }).collect::<Vec<_>>()
    })))
}

// ─── Random Game ───

async fn get_random_game(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let game = db::get_random_game(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "game": game })))
}

// ─── Favorites ───

async fn get_favorites(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let fav_ids_str = db::get_setting(&state.db, "favorites")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .unwrap_or_default();

    let ids: Vec<i64> = if fav_ids_str.is_empty() {
        vec![]
    } else {
        fav_ids_str.split(',')
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .collect()
    };

    if ids.is_empty() {
        return Ok(Json(serde_json::json!({ "favorites": [], "ids": [] })));
    }

    // Fetch game details for each favorite
    let mut games = Vec::new();
    for id in &ids {
        if let Ok(game) = db::get_game_by_id(&state.db, *id).await {
            games.push(game);
        }
    }

    Ok(Json(serde_json::json!({
        "favorites": games,
        "ids": ids
    })))
}

async fn add_favorite(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    let current = db::get_setting(&state.db, "favorites")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
            success: false, message: e.to_string(), downloads: None, download_id: None,
        })))?
        .unwrap_or_default();

    let mut ids: Vec<i64> = if current.is_empty() {
        vec![]
    } else {
        current.split(',').filter_map(|s| s.trim().parse::<i64>().ok()).collect()
    };

    if !ids.contains(&id) {
        ids.push(id);
        let new_val = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        db::set_setting(&state.db, "favorites", &new_val).await.map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
                success: false, message: e.to_string(), downloads: None, download_id: None,
            }))
        })?;
    }

    Ok(Json(ApiResponse {
        success: true,
        message: "Added to favorites".to_string(),
        downloads: None,
        download_id: None,
    }))
}

async fn remove_favorite(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    let current = db::get_setting(&state.db, "favorites")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
            success: false, message: e.to_string(), downloads: None, download_id: None,
        })))?
        .unwrap_or_default();

    let ids: Vec<i64> = current.split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .filter(|i| *i != id)
        .collect();

    let new_val = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    db::set_setting(&state.db, "favorites", &new_val).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
            success: false, message: e.to_string(), downloads: None, download_id: None,
        }))
    })?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Removed from favorites".to_string(),
        downloads: None,
        download_id: None,
    }))
}

async fn upload_csv(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    let field = multipart
        .next_field()
        .await
        .map_err(|e| {
            (StatusCode::BAD_REQUEST, Json(ApiResponse {
                success: false,
                message: format!("Failed to read upload: {}", e),
                downloads: None,
                download_id: None,
            }))
        })?
        .ok_or_else(|| {
            (StatusCode::BAD_REQUEST, Json(ApiResponse {
                success: false,
                message: "No file provided".to_string(),
                downloads: None,
                download_id: None,
            }))
        })?;

    if field.name() != Some("file") {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse {
            success: false,
            message: "Expected field named 'file'".to_string(),
            downloads: None,
            download_id: None,
        })));
    }

    let data = field.bytes().await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ApiResponse {
            success: false,
            message: format!("Failed to read file data: {}", e),
            downloads: None,
            download_id: None,
        }))
    })?;

    let mut reader = csv::Reader::from_reader(data.as_ref());
    let mut games = Vec::new();

    for (i, result) in reader.records().enumerate() {
        let record = match result {
            Ok(r) => r,
            Err(e) => {
                eprintln!("CSV parse error at row {}: {}", i + 1, e);
                continue;
            }
        };

        if record.len() < 3 {
            eprintln!("CSV row {} has fewer than 3 columns, skipping", i + 1);
            continue;
        }

        let title = record.get(0).unwrap_or("").trim().to_string();
        let file_size = record.get(1).unwrap_or("").trim().to_string();
        let magnet_link = record.get(2).unwrap_or("").trim().to_string();

        if title.is_empty() {
            eprintln!("CSV row {} has empty title, skipping", i + 1);
            continue;
        }
        if !magnet_link.starts_with("magnet:?") {
            eprintln!("CSV row {} has invalid magnet link, skipping", i + 1);
            continue;
        }

        games.push(db::GameInsert {
            search_title: Some(db::clean_search_title(&title)),
            title,
            source: "fitgirl".to_string(),  // CSV uploads default to fitgirl
            file_size,
            magnet_link,
            genres: None,
            company: None,
            original_size: None,
            thumbnail_url: None,
            screenshots: None,
            source_url: None,
            post_date: None,
        });
    }

    if games.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse {
            success: false,
            message: "No valid games found in CSV. Expected format: Title,Size,magnet:?...".to_string(),
            downloads: None,
            download_id: None,
        })));
    }

    let count = db::replace_all_games(&state.db, games)
        .await
        .map_err(|e| {
            eprintln!("Database error during CSV import: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
                success: false,
                message: "Database error during import".to_string(),
                downloads: None,
                download_id: None,
            }))
        })?;

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Imported {} games", count),
        downloads: None,
        download_id: None,
    }))
}

#[derive(Deserialize)]
struct RescrapeParams {
    #[serde(default)]
    source: Option<String>,  // "fitgirl", "steamrip", or "all"
}

async fn rescrape(
    State(state): State<AppState>,
    Query(params): Query<RescrapeParams>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    {
        let status = state.scrape_status.read().await;
        if status.is_running {
            return Err((StatusCode::CONFLICT, Json(ApiResponse {
                success: false,
                message: "A scrape is already in progress".to_string(),
                downloads: None,
                download_id: None,
            })));
        }
    }

    {
        let mut status = state.scrape_status.write().await;
        status.is_running = true;
        status.last_result = None;
        status.progress = scrapers::ScrapeProgress::default();
    }

    let scrape_status = state.scrape_status.clone();
    let db = state.db.clone();
    let scraper_registry = state.scraper_registry.clone();

    // Determine which sources to scrape
    let source_filter = params.source.unwrap_or_else(|| "all".to_string());
    let sources_to_scrape: Vec<String> = if source_filter == "all" {
        vec!["fitgirl".to_string(), "steamrip".to_string()]
    } else {
        vec![source_filter]
    };

    // Read RAWG key from DB first, fall back to env var
    let rawg_key = db::get_setting(&state.db, "rawg_api_key")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| state.rawg_api_key.clone());

    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            println!("Starting scrape for sources: {:?}", sources_to_scrape);

            // Create shared progress for the scraper
            let scrape_progress = Arc::new(RwLock::new(scrapers::ScrapeProgress::default()));

            // Spawn a task to sync scraper progress back to ScrapeStatus every second
            let sync_progress = scrape_progress.clone();
            let sync_status = scrape_status.clone();
            let sync_task = tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let p = sync_progress.read().await.clone();
                    let mut s = sync_status.write().await;
                    if !s.is_running {
                        break;
                    }
                    s.progress = p;
                }
            });

            // Scrape from all requested sources
            let mut all_scraped_games = Vec::new();
            for source_name in sources_to_scrape {
                if let Some(scraper) = scraper_registry.get(&source_name) {
                    println!("Scraping from source: {}", scraper.source_label());
                    match scraper.scrape_all_games(scrape_progress.clone()).await {
                        Ok(games) => {
                            println!("Got {} games from {}", games.len(), scraper.source_label());
                            all_scraped_games.extend(games);
                        }
                        Err(e) => {
                            eprintln!("Failed to scrape from {}: {}", scraper.source_label(), e);
                        }
                    }
                } else {
                    eprintln!("Unknown source: {}", source_name);
                }
            }

            let result = if !all_scraped_games.is_empty() {
                {
                    let total = all_scraped_games.len();
                    let with_img = all_scraped_games.iter().filter(|g| g.thumbnail_url.is_some()).count();
                    let with_genres = all_scraped_games.iter().filter(|g| g.genres.is_some()).count();
                    println!(
                        "WP scrape got {}/{} images, {}/{} genres — checking RAWG for gaps...",
                        with_img, total, with_genres, total
                    );

                    // RAWG enrichment — only for games MISSING images or genres
                    if !rawg_key.is_empty() {
                        // Load existing metadata cache from DB to avoid re-querying RAWG
                        let metadata_cache = db::get_metadata_cache(&db).await.unwrap_or_default();
                        let cache_size = metadata_cache.len();
                        if cache_size > 0 {
                            println!("Loaded RAWG cache with {} entries from existing DB", cache_size);
                        }

                        // Apply cache first
                        let mut cache_hits = 0;
                        for game in all_scraped_games.iter_mut() {
                            if game.thumbnail_url.is_some() && game.genres.is_some() {
                                continue;
                            }
                            let norm = game.title.to_lowercase()
                                .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ");
                            if let Some((cached_thumb, cached_genres)) = metadata_cache.get(&norm) {
                                if game.thumbnail_url.is_none() && cached_thumb.is_some() {
                                    game.thumbnail_url = cached_thumb.clone();
                                    cache_hits += 1;
                                }
                                if game.genres.is_none() && cached_genres.is_some() {
                                    game.genres = cached_genres.clone();
                                }
                            }
                        }
                        if cache_hits > 0 {
                            println!("RAWG cache filled {} games without API calls", cache_hits);
                        }

                        let missing_indices: Vec<usize> = all_scraped_games.iter().enumerate()
                            .filter(|(_, g)| g.thumbnail_url.is_none() || g.genres.is_none())
                            .map(|(i, _)| i)
                            .collect();

                        if missing_indices.is_empty() {
                            println!("All games have images and genres from WP — skipping RAWG");
                        } else {
                            println!("RAWG enriching {} games missing images/genres...", missing_indices.len());
                            let titles: Vec<String> = missing_indices.iter()
                                .map(|&i| all_scraped_games[i].title.clone())
                                .collect();
                            let metadata = rawg::enrich_games(&titles, &rawg_key, scrape_progress.clone()).await;

                            let mut images_applied = 0;
                            let mut genres_applied = 0;
                            for (j, meta) in metadata.into_iter().enumerate() {
                                let i = missing_indices[j];
                                if let Some(meta) = meta {
                                    if all_scraped_games[i].thumbnail_url.is_none() && meta.image_url.is_some() {
                                        all_scraped_games[i].thumbnail_url = meta.image_url;
                                        images_applied += 1;
                                    }
                                    if all_scraped_games[i].genres.is_none() && meta.genres.is_some() {
                                        all_scraped_games[i].genres = meta.genres;
                                        genres_applied += 1;
                                    }
                                }
                            }
                            println!(
                                "RAWG filled: {} images, {} genres",
                                images_applied, genres_applied
                            );
                        }
                    } else {
                        let missing = total - with_img;
                        if missing > 0 {
                            println!(
                                "⚠ {} games missing images — set RAWG_API_KEY in Settings to fill gaps",
                                missing
                            );
                        }
                    }

                    // Update progress to saving phase
                    {
                        let mut p = scrape_progress.write().await;
                        p.phase = "saving".to_string();
                        p.message = format!("Saving {} games to database...", all_scraped_games.len());
                        p.progress = 98.0;
                    }
                    // Sync once more
                    {
                        let p = scrape_progress.read().await.clone();
                        let mut s = scrape_status.write().await;
                        s.progress = p;
                    }

                    println!("Scraped {} games, deduplicating...", all_scraped_games.len());

                    // Deduplicate by normalized title — keep the entry with the most metadata
                    let before_dedup = all_scraped_games.len();
                    {
                        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                        let mut keep = vec![false; all_scraped_games.len()];
                        for (i, g) in all_scraped_games.iter().enumerate() {
                            let norm = g.title.to_lowercase()
                                .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ");
                            if let Some(&prev) = seen.get(&norm) {
                                // Keep whichever has more metadata (thumbnail, genres, screenshots)
                                let score = |idx: usize| -> usize {
                                    let g = &all_scraped_games[idx];
                                    (if g.thumbnail_url.is_some() { 1 } else { 0 })
                                    + (if g.genres.is_some() { 1 } else { 0 })
                                    + (if g.screenshots.is_some() { 1 } else { 0 })
                                    + (if g.company.is_some() { 1 } else { 0 })
                                };
                                if score(i) > score(prev) {
                                    keep[prev] = false;
                                    keep[i] = true;
                                    seen.insert(norm, i);
                                }
                                // else keep the previous one
                            } else {
                                seen.insert(norm, i);
                                keep[i] = true;
                            }
                        }
                        let mut idx = 0;
                        all_scraped_games.retain(|_| { let k = keep[idx]; idx += 1; k });
                    }
                    if before_dedup != all_scraped_games.len() {
                        println!("Deduped: {} → {} games ({} duplicates removed)",
                            before_dedup, all_scraped_games.len(), before_dedup - all_scraped_games.len());
                    }

                    println!("Inserting {} games into database...", all_scraped_games.len());

                    // Convert scraped games to database inserts
                    let game_inserts: Vec<db::GameInsert> = all_scraped_games
                        .into_iter()
                        .map(|g| {
                            let search_title = Some(db::clean_search_title(&g.title));
                            db::GameInsert {
                                title: g.title,
                                source: g.source,  // Use the source field from ScrapedGame
                                file_size: g.file_size,
                                magnet_link: g.download_link,
                                genres: g.genres,
                                company: g.company,
                                original_size: g.original_size,
                                thumbnail_url: g.thumbnail_url,
                                screenshots: g.screenshots,
                                source_url: g.source_url,
                                post_date: g.post_date,
                                search_title,
                            }
                        })
                        .collect();

                    match db::replace_all_games(&db, game_inserts).await {
                        Ok(count) => {
                            println!("Successfully inserted {} games", count);
                            format!("Successfully scraped and inserted {} games", count)
                        }
                        Err(e) => {
                            eprintln!("Error inserting games: {}", e);
                            format!("Scrape succeeded but database insert failed: {}", e)
                        }
                    }
                }
            } else {
                "No games were scraped from any source".to_string()
            };

            let mut status = scrape_status.write().await;
            status.is_running = false;
            status.last_result = Some(result);
            status.last_completed = Some(chrono::Utc::now().to_rfc3339());

            sync_task.abort();
        })
    });

    Ok(Json(ApiResponse {
        success: true,
        message: "Scraping started in background. Poll /api/scrape-status for updates.".to_string(),
        downloads: None,
        download_id: None,
    }))
}

async fn get_scrape_status(
    State(state): State<AppState>,
) -> Json<ScrapeStatus> {
    let status = state.scrape_status.read().await;
    Json(status.clone())
}

#[derive(Serialize)]
struct SourcesResponse {
    sources: Vec<db::SourceStat>,
}

async fn get_sources(
    State(state): State<AppState>,
) -> Result<Json<SourcesResponse>, StatusCode> {
    let stats = db::get_source_stats(&state.db)
        .await
        .map_err(|e| {
            eprintln!("Error getting source stats: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(SourcesResponse { sources: stats }))
}

async fn add_to_realdebrid(
    State(state): State<AppState>,
    Json(payload): Json<AddMagnetRequest>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    let game = db::get_game_by_id(&state.db, payload.game_id)
        .await
        .map_err(|e| {
            eprintln!("Error fetching game {}: {}", payload.game_id, e);
            (StatusCode::NOT_FOUND, Json(ApiResponse {
                success: false,
                message: "Game not found".to_string(),
                downloads: None,
                download_id: None,
            }))
        })?;

    // Check DB for API key first, fall back to startup env var
    let rd_client = if let Ok(Some(db_key)) = db::get_setting(&state.db, "rd_api_key").await {
        if !db_key.is_empty() {
            Arc::new(realdebrid::RealDebridClient::new(db_key))
        } else {
            state.rd_client.clone()
        }
    } else {
        state.rd_client.clone()
    };

    // Use the universal process_link function that handles both magnets and DDL
    match rd_client.process_link(&game.magnet_link).await {
        Ok(downloads) => {
            if downloads.is_empty() {
                Ok(Json(ApiResponse {
                    success: false,
                    message: "No download links available".to_string(),
                    downloads: None,
                    download_id: None,
                }))
            } else {
                Ok(Json(ApiResponse {
                    success: true,
                    message: format!("'{}' is ready to download! Found {} file(s).", game.title, downloads.len()),
                    downloads: Some(downloads),
                    download_id: None,
                }))
            }
        }
        Err(e) => {
            eprintln!("Real-Debrid error for game '{}': {}", game.title, e);
            Ok(Json(ApiResponse {
                success: false,
                message: format!("Real-Debrid error: {}", e),
                downloads: None,
                download_id: None,
            }))
        }
    }
}

// ─── Download management endpoints ───

async fn get_downloads(
    State(state): State<AppState>,
) -> Result<Json<DownloadsResponse>, StatusCode> {
    let downloads = state.download_manager.get_downloads()
        .await
        .map_err(|e| {
            eprintln!("Error getting downloads: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(DownloadsResponse { downloads }))
}

async fn queue_download(
    State(state): State<AppState>,
    Json(payload): Json<QueueDownloadRequest>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    match state.download_manager.queue_download(payload.game_id).await {
        Ok(download_id) => {
            Ok(Json(ApiResponse {
                success: true,
                message: "Added to download queue".to_string(),
                downloads: None,
                download_id: Some(download_id),
            }))
        }
        Err(e) => {
            Err((StatusCode::BAD_REQUEST, Json(ApiResponse {
                success: false,
                message: e.to_string(),
                downloads: None,
                download_id: None,
            })))
        }
    }
}

async fn get_download_status(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<download_manager::DownloadInfo>, StatusCode> {
    state.download_manager.get_download(id)
        .await
        .map(Json)
        .map_err(|e| {
            eprintln!("Error getting download {}: {}", id, e);
            StatusCode::NOT_FOUND
        })
}

async fn cancel_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    state.download_manager.cancel_download(id)
        .await
        .map(|_| Json(ApiResponse {
            success: true,
            message: "Download cancelled".to_string(),
            downloads: None,
            download_id: None,
        }))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse {
            success: false,
            message: e.to_string(),
            downloads: None,
            download_id: None,
        })))
}

async fn retry_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    state.download_manager.retry_download(id)
        .await
        .map(|_| Json(ApiResponse {
            success: true,
            message: "Download requeued".to_string(),
            downloads: None,
            download_id: None,
        }))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse {
            success: false,
            message: e.to_string(),
            downloads: None,
            download_id: None,
        })))
}

async fn remove_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    state.download_manager.remove_download(id)
        .await
        .map(|_| Json(ApiResponse {
            success: true,
            message: "Download removed".to_string(),
            downloads: None,
            download_id: None,
        }))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse {
            success: false,
            message: e.to_string(),
            downloads: None,
            download_id: None,
        })))
}

async fn launch_install(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    state.download_manager.launch_installer(id)
        .await
        .map(|path| Json(ApiResponse {
            success: true,
            message: format!("Installer launched: {}", path),
            downloads: None,
            download_id: None,
        }))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse {
            success: false,
            message: e.to_string(),
            downloads: None,
            download_id: None,
        })))
}

async fn mark_installed(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    state.download_manager.mark_installed(id)
        .await
        .map(|_| Json(ApiResponse {
            success: true,
            message: "Marked as installed".to_string(),
            downloads: None,
            download_id: None,
        }))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse {
            success: false,
            message: e.to_string(),
            downloads: None,
            download_id: None,
        })))
}

async fn validate_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<md5_validator::ValidationResult>, (StatusCode, String)> {
    // Get download info to find the directory
    let download = state.download_manager.get_download(id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Download not found: {}", e)))?;

    let file_path = download.file_path
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Download has no file path".to_string()))?;

    let dir = std::path::Path::new(&file_path);

    if !dir.exists() {
        return Err((StatusCode::NOT_FOUND, "Download directory does not exist".to_string()));
    }

    if !dir.is_dir() {
        return Err((StatusCode::BAD_REQUEST, "Download path is not a directory".to_string()));
    }

    println!("Validating MD5 checksums for download {} in {}", id, dir.display());

    md5_validator::validate_directory(dir)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Validation error: {}", e)))
}

async fn delete_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    state.download_manager.delete_download(id)
        .await
        .map(|_| Json(ApiResponse {
            success: true,
            message: "Download and files deleted permanently".to_string(),
            downloads: None,
            download_id: None,
        }))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse {
            success: false,
            message: e.to_string(),
            downloads: None,
            download_id: None,
        })))
}

async fn scan_existing_games(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    match state.download_manager.scan_existing_games().await {
        Ok(count) => {
            Ok(Json(ApiResponse {
                success: true,
                message: format!("Scanned and imported {} existing game(s)", count),
                downloads: None,
                download_id: None,
            }))
        }
        Err(e) => {
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
                success: false,
                message: format!("Scan failed: {}", e),
                downloads: None,
                download_id: None,
            })))
        }
    }
}

async fn download_file(
    State(state): State<AppState>,
    Path(file_id): Path<i64>,
) -> Result<Response, (StatusCode, String)> {
    // Get file info from database
    let file_info: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT filename, file_path FROM download_files WHERE id = ?"
    )
    .bind(file_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {}", e)))?;

    let (filename, file_path) = file_info
        .ok_or_else(|| (StatusCode::NOT_FOUND, "File not found".to_string()))?;

    let path = file_path
        .ok_or_else(|| (StatusCode::NOT_FOUND, "File path not available".to_string()))?;

    let file_path = std::path::Path::new(&path);

    if !file_path.exists() {
        return Err((StatusCode::NOT_FOUND, "File does not exist on disk".to_string()));
    }

    // Open the file
    let file = tokio::fs::File::open(&file_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to open file: {}", e)))?;

    // Get file size
    let metadata = file.metadata()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read metadata: {}", e)))?;
    let file_size = metadata.len();

    // Create a stream
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    // Build response with appropriate headers
    let content_disposition = format!("attachment; filename=\"{}\"", filename);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, content_disposition)
        .header(header::CONTENT_LENGTH, file_size.to_string())
        .body(body)
        .unwrap())
}

// ─── Settings ───

#[derive(Serialize)]
struct SettingsResponse {
    success: bool,
    settings: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
struct SettingsPayload {
    settings: std::collections::HashMap<String, String>,
}

/// Allowed setting keys (whitelist for security)
const ALLOWED_SETTINGS: &[&str] = &["rawg_api_key", "rd_api_key"];

/// Mask an API key for display: show first 4 and last 4 chars
fn mask_key(key: &str) -> String {
    if key.len() <= 10 {
        return "*".repeat(key.len());
    }
    format!("{}...{}", &key[..4], &key[key.len()-4..])
}

async fn get_settings(
    State(state): State<AppState>,
) -> Json<SettingsResponse> {
    let pairs = db::get_all_settings(&state.db).await.unwrap_or_default();
    let mut settings = std::collections::HashMap::new();

    for (key, value) in pairs {
        if ALLOWED_SETTINGS.contains(&key.as_str()) {
            settings.insert(format!("{}_masked", key), mask_key(&value));
            settings.insert(format!("{}_set", key), "true".to_string());
        }
    }

    for &key in ALLOWED_SETTINGS {
        if !settings.contains_key(&format!("{}_set", key)) {
            settings.insert(format!("{}_set", key), "false".to_string());
            settings.insert(format!("{}_masked", key), String::new());
        }
    }

    Json(SettingsResponse {
        success: true,
        settings,
    })
}

async fn save_settings(
    State(state): State<AppState>,
    Json(payload): Json<SettingsPayload>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    for (key, value) in &payload.settings {
        if !ALLOWED_SETTINGS.contains(&key.as_str()) {
            return Err((StatusCode::BAD_REQUEST, Json(ApiResponse {
                success: false,
                message: format!("Unknown setting: {}", key),
                downloads: None,
                download_id: None,
            })));
        }

        let trimmed = value.trim();
        if trimmed.is_empty() {
            db::delete_setting(&state.db, key).await.map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
                    success: false,
                    message: format!("Failed to delete setting: {}", e),
                    downloads: None,
                    download_id: None,
                }))
            })?;
        } else {
            db::set_setting(&state.db, key, trimmed).await.map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
                    success: false,
                    message: format!("Failed to save setting: {}", e),
                    downloads: None,
                    download_id: None,
                }))
            })?;
        }
    }

    Ok(Json(ApiResponse {
        success: true,
        message: "Settings saved".to_string(),
        downloads: None,
        download_id: None,
    }))
}

/// Get current system information
async fn get_system_info(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let system_info = system_info::SystemInfo::gather().await;

    // Save to database
    let _ = db::insert_system_check(
        &state.db,
        Some(system_info.ram_available_gb),
        Some(system_info.temp_space_gb),
        Some(system_info.cpu_cores),
        Some(system_info.antivirus_active),
        if system_info.missing_dlls.is_empty() {
            None
        } else {
            Some(system_info.missing_dlls.join(", "))
        },
        if system_info.missing_dependencies.is_empty() {
            None
        } else {
            Some(system_info.missing_dependencies.join(", "))
        },
        Some(format!("{:?}", system_info.overall_status)),
    )
    .await;

    Json(serde_json::json!({
        "ram_total_gb": system_info.ram_total_gb,
        "ram_available_gb": system_info.ram_available_gb,
        "temp_space_gb": system_info.temp_space_gb,
        "cpu_cores": system_info.cpu_cores,
        "antivirus_active": system_info.antivirus_active,
        "missing_dlls": system_info.missing_dlls,
        "missing_dependencies": system_info.missing_dependencies,
        "overall_status": system_info.overall_status,
        "issues": system_info.get_issues(),
        "recommendations": system_info.get_recommendations(),
    }))
}

/// Check if system is ready for game installation
async fn check_pre_install(
    State(state): State<AppState>,
    Path(game_id): Path<i64>,
) -> Result<Json<installation_checker::PreInstallCheckResult>, (StatusCode, String)> {
    match installation_checker::check_pre_installation(&state.db, game_id).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Pre-installation check failed: {}", e),
        )),
    }
}

// ─── Installation Assistant Handlers ───

#[derive(Deserialize)]
struct AssistantActionsRequest {
    missing_dlls: Vec<String>,
    missing_dependencies: Vec<String>,
    antivirus_active: bool,
    install_path: Option<String>,
}

async fn get_assistant_actions(
    Json(req): Json<AssistantActionsRequest>,
) -> Json<Vec<installation_assistant::AssistantAction>> {
    let actions = installation_assistant::get_recommended_actions(
        &req.missing_dlls,
        &req.missing_dependencies,
        req.antivirus_active,
        req.install_path.as_deref(),
    );
    Json(actions)
}

#[derive(Deserialize)]
struct InstallDllRequest {
    dll_name: String,
}

async fn assistant_install_dll(
    Json(req): Json<InstallDllRequest>,
) -> Result<Json<ApiResponse>, (StatusCode, String)> {
    match installation_assistant::install_dll(&req.dll_name).await {
        Ok(message) => Ok(Json(ApiResponse {
            success: true,
            message,
            downloads: None,
            download_id: None,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DLL installation failed: {}", e),
        )),
    }
}

#[derive(Deserialize)]
struct AddExclusionRequest {
    path: String,
}

async fn assistant_add_exclusion(
    Json(req): Json<AddExclusionRequest>,
) -> Result<Json<ApiResponse>, (StatusCode, String)> {
    match installation_assistant::add_av_exclusion(&req.path).await {
        Ok(message) => Ok(Json(ApiResponse {
            success: true,
            message,
            downloads: None,
            download_id: None,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to add exclusion: {}", e),
        )),
    }
}

#[derive(Deserialize)]
struct ToggleAvRequest {
    enable: bool,
}

async fn assistant_toggle_av(
    Json(req): Json<ToggleAvRequest>,
) -> Result<Json<ApiResponse>, (StatusCode, String)> {
    let result = if req.enable {
        installation_assistant::enable_realtime_protection().await
    } else {
        installation_assistant::disable_realtime_protection().await
    };

    match result {
        Ok(message) => Ok(Json(ApiResponse {
            success: true,
            message,
            downloads: None,
            download_id: None,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to toggle antivirus: {}", e),
        )),
    }
}

async fn get_dependency_info(
    Path(dep): Path<String>,
) -> Result<Json<installation_assistant::DependencyInfo>, (StatusCode, String)> {
    match installation_assistant::get_dependency_installer_info(&dep) {
        Some(info) => Ok(Json(info)),
        None => Err((
            StatusCode::NOT_FOUND,
            format!("No installer information available for: {}", dep),
        )),
    }
}

// ─── Installation Monitoring Handlers ───

async fn get_installation_history(
    State(state): State<AppState>,
    Path(game_id): Path<i64>,
) -> Result<Json<Vec<db::InstallationLog>>, (StatusCode, String)> {
    match installation_monitor::get_installation_history(&state.db, game_id).await {
        Ok(logs) => Ok(Json(logs)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get installation history: {}", e),
        )),
    }
}

async fn get_installation_stats(
    State(state): State<AppState>,
) -> Result<Json<installation_monitor::InstallationStats>, (StatusCode, String)> {
    match installation_monitor::get_installation_stats(&state.db).await {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get installation stats: {}", e),
        )),
    }
}

async fn analyze_failed_installation(
    State(state): State<AppState>,
    Path(log_id): Path<i64>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Get the log
    let logs = installation_monitor::get_all_installation_logs(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let log = logs
        .iter()
        .find(|l| l.id == log_id)
        .ok_or((StatusCode::NOT_FOUND, "Installation log not found".to_string()))?;

    let recommendations = installation_monitor::analyze_installation_failure(log);

    Ok(Json(serde_json::json!({
        "log": log,
        "recommendations": recommendations,
    })))
}

// ─── Client Management Handlers ───

#[derive(Deserialize)]
struct RegisterClientRequest {
    client_id: String,
    client_name: String,
    os_version: String,
}

#[derive(Serialize)]
struct RegisterClientResponse {
    success: bool,
    message: String,
}

async fn register_client(
    State(state): State<AppState>,
    Json(payload): Json<RegisterClientRequest>,
) -> Result<Json<RegisterClientResponse>, (StatusCode, Json<RegisterClientResponse>)> {
    match db::register_client(
        &state.db,
        &payload.client_id,
        &payload.client_name,
        &payload.os_version,
    )
    .await
    {
        Ok(_) => Ok(Json(RegisterClientResponse {
            success: true,
            message: format!("Client {} registered successfully", payload.client_name),
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RegisterClientResponse {
                success: false,
                message: format!("Failed to register client: {}", e),
            }),
        )),
    }
}

#[derive(Serialize)]
struct QueueItem {
    game_id: i64,
    game_title: String,
    file_path: String,
    expected_md5: Option<String>,
}

async fn get_client_queue(
    State(_state): State<AppState>,
    Path(_client_id): Path<String>,
) -> Json<Vec<QueueItem>> {
    // For now, return empty queue
    // In the future, this could return games the client should download/extract
    Json(Vec::new())
}

#[derive(Deserialize)]
struct ProgressUpdate {
    file_path: String,
    total_bytes: i64,
    extracted_bytes: i64,
    progress_percent: f64,
    speed_mbps: f64,
    eta_seconds: i64,
    status: String,
}

async fn update_client_progress(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Json(payload): Json<ProgressUpdate>,
) -> Result<StatusCode, (StatusCode, String)> {
    db::upsert_client_progress(
        &state.db,
        &client_id,
        None,
        &payload.file_path,
        payload.total_bytes,
        payload.extracted_bytes,
        payload.progress_percent,
        payload.speed_mbps,
        payload.eta_seconds,
        &payload.status,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct SystemInfoUpdate {
    ram_total_gb: f64,
    ram_available_gb: f64,
    disk_space_gb: f64,
    cpu_cores: i64,
    missing_dlls: Vec<String>,
}

async fn update_client_system_info(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Json(payload): Json<SystemInfoUpdate>,
) -> Result<StatusCode, (StatusCode, String)> {
    let missing_dlls = if payload.missing_dlls.is_empty() {
        None
    } else {
        Some(payload.missing_dlls.join(", "))
    };

    db::update_client_system_info(
        &state.db,
        &client_id,
        payload.ram_total_gb,
        payload.ram_available_gb,
        payload.disk_space_gb,
        payload.cpu_cores,
        missing_dlls,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::OK)
}

async fn get_all_clients(
    State(state): State<AppState>,
) -> Result<Json<Vec<db::Client>>, (StatusCode, String)> {
    let clients = db::get_all_clients(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(clients))
}

async fn health_check(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
    Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "db": db_ok,
    }))
}
