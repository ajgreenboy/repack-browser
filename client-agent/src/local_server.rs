use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

#[derive(Debug, Deserialize)]
pub struct DownloadRequest {
    pub game_id: i64,
    #[allow(dead_code)]
    pub user_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DownloadResponse {
    pub success: bool,
    pub message: String,
}

/// Shared state for download queue
pub struct LocalServerState {
    pub pending_downloads: Arc<RwLock<Vec<DownloadRequest>>>,
}

impl LocalServerState {
    pub fn new() -> Self {
        Self {
            pending_downloads: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

/// Start local HTTP server on localhost:9999 for browser communication
pub async fn start_local_server(state: Arc<LocalServerState>) {
    let state_filter = warp::any().map(move || state.clone());

    // CORS configuration - allow requests from the web UI
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST", "OPTIONS"])
        .allow_headers(vec!["Content-Type"]);

    // POST /download - Queue a download
    let download_route = warp::path("download")
        .and(warp::post())
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(handle_download_request);

    // GET /health - Health check
    let health_route = warp::path("health")
        .and(warp::get())
        .map(|| warp::reply::json(&serde_json::json!({"status": "ok"})));

    let routes = download_route
        .or(health_route)
        .with(cors);

    log::info!("Starting local server on http://localhost:9999");

    warp::serve(routes)
        .run(([127, 0, 0, 1], 9999))
        .await;
}

async fn handle_download_request(
    req: DownloadRequest,
    state: Arc<LocalServerState>,
) -> Result<impl warp::Reply, warp::Rejection> {
    log::info!("Received download request for game_id: {}", req.game_id);

    // Add to pending downloads queue
    let mut queue = state.pending_downloads.write().await;
    queue.push(req);

    Ok(warp::reply::json(&DownloadResponse {
        success: true,
        message: "Download queued successfully".to_string(),
    }))
}
