mod client_id;
mod config;
mod extractor;
mod server_client;
mod system_info;

use config::Config;
use extractor::Extractor;
use log::{error, info};
use server_client::ServerClient;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time;

#[derive(Clone)]
struct AppState {
    config: Arc<RwLock<Config>>,
    server_client: Arc<ServerClient>,
    current_extraction: Arc<RwLock<Option<Arc<Extractor>>>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logger
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("FitGirl Client Agent starting...");

    // Load or create configuration
    let mut config = Config::load()?;

    // Generate or load client ID
    let client_id = client_id::generate_or_load_client_id(&mut config);
    info!("Client ID: {}", client_id);

    // Create server client
    let server_client = Arc::new(ServerClient::new(config.server.url.clone()));

    // Create app state
    let state = AppState {
        config: Arc::new(RwLock::new(config.clone())),
        server_client: server_client.clone(),
        current_extraction: Arc::new(RwLock::new(None)),
    };

    // Register with server
    if config.server.enabled {
        match register_with_server(&state).await {
            Ok(_) => info!("Successfully registered with server"),
            Err(e) => error!("Failed to register with server: {}", e),
        }
    }

    // Start background tasks
    let state_clone = state.clone();
    tokio::spawn(async move {
        poll_download_queue(state_clone).await;
    });

    let state_clone = state.clone();
    tokio::spawn(async move {
        report_system_info_periodically(state_clone).await;
    });

    let state_clone = state.clone();
    tokio::spawn(async move {
        watch_extraction_folder(state_clone).await;
    });

    // Keep main thread alive
    info!("Client agent running. Press Ctrl+C to exit.");
    tokio::signal::ctrl_c().await?;

    info!("Shutting down...");
    Ok(())
}

async fn register_with_server(state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = state.config.read().await;

    let sys_info = system_info::gather_system_info(
        &config.client.id,
        &config.client.name,
    );

    state
        .server_client
        .register(&config.client.id, &config.client.name, &sys_info.os_version)
        .await?;

    Ok(())
}

async fn poll_download_queue(state: AppState) {
    let mut interval = time::interval(Duration::from_secs(
        state.config.read().await.server.poll_interval_secs,
    ));

    loop {
        interval.tick().await;

        if !state.config.read().await.server.enabled {
            continue;
        }

        // Check if server is reachable
        if !state.server_client.health_check().await {
            continue;
        }

        let config = state.config.read().await;
        let client_id = config.client.id.clone();
        drop(config);

        // Get download queue from server
        match state.server_client.get_download_queue(&client_id).await {
            Ok(queue) => {
                if !queue.is_empty() {
                    info!("Received {} items in download queue", queue.len());

                    // Process each item
                    for item in queue {
                        info!("Processing: {}", item.game_title);

                        let file_path = PathBuf::from(&item.file_path);
                        if !file_path.exists() {
                            error!("File not found: {}", item.file_path);
                            continue;
                        }

                        // Start extraction
                        if let Err(e) = start_extraction(&state, file_path, item).await {
                            error!("Extraction failed: {}", e);
                        }
                    }
                }
            }
            Err(e) => error!("Failed to get download queue: {}", e),
        }
    }
}

async fn start_extraction(
    state: &AppState,
    archive_path: PathBuf,
    queue_item: server_client::DownloadQueueItem,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = state.config.read().await;
    let output_dir = config.extraction.output_dir.join(&queue_item.game_title);
    let verify_md5 = config.extraction.verify_md5;
    let client_id = config.client.id.clone();
    drop(config);

    info!("Extracting {} to {:?}", queue_item.game_title, output_dir);

    let extractor = Arc::new(Extractor::new(archive_path.to_string_lossy().to_string()));

    // Store current extraction
    {
        let mut current = state.current_extraction.write().await;
        *current = Some(extractor.clone());
    }

    // Start progress reporting task
    let progress = extractor.get_progress();
    let server_client = state.server_client.clone();
    let client_id_clone = client_id.clone();

    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(2));

        loop {
            interval.tick().await;

            let prog = progress.read().await;
            if matches!(
                prog.status,
                extractor::ExtractionStatus::Completed | extractor::ExtractionStatus::Failed
            ) {
                break;
            }

            let _ = server_client.report_progress(&client_id_clone, &prog).await;
        }
    });

    // Perform extraction
    extractor.extract(&archive_path, &output_dir).await?;

    info!("Extraction completed: {}", queue_item.game_title);

    // Verify MD5 if provided
    if verify_md5 {
        if let Some(expected_md5) = queue_item.expected_md5 {
            info!("Verifying MD5...");

            match extractor.verify_md5(&output_dir, &expected_md5).await {
                Ok(true) => info!("MD5 verification passed"),
                Ok(false) => error!("MD5 verification failed!"),
                Err(e) => error!("MD5 verification error: {}", e),
            }
        }
    }

    // Clear current extraction
    {
        let mut current = state.current_extraction.write().await;
        *current = None;
    }

    Ok(())
}

async fn report_system_info_periodically(state: AppState) {
    let mut interval = time::interval(Duration::from_secs(60));

    loop {
        interval.tick().await;

        if !state.config.read().await.server.enabled {
            continue;
        }

        let config = state.config.read().await;
        let sys_info = system_info::gather_system_info(
            &config.client.id,
            &config.client.name,
        );
        drop(config);

        if let Err(e) = state.server_client.report_system_info(&sys_info).await {
            error!("Failed to report system info: {}", e);
        }
    }
}

async fn watch_extraction_folder(state: AppState) {
    let mut interval = time::interval(Duration::from_secs(5));

    loop {
        interval.tick().await;

        // Check if there's already an extraction in progress
        {
            let current = state.current_extraction.read().await;
            if current.is_some() {
                continue;
            }
        }

        // Scan watch directory for archives
        let config = state.config.read().await;
        let watch_dir = config.extraction.watch_dir.clone();
        drop(config);

        if !watch_dir.exists() {
            continue;
        }

        // Look for archive files
        if let Ok(entries) = std::fs::read_dir(&watch_dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if !path.is_file() {
                    continue;
                }

                let extension = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if matches!(extension.as_str(), "zip" | "7z") {
                    info!("Found archive in watch folder: {:?}", path);

                    // Auto-extract
                    let queue_item = server_client::DownloadQueueItem {
                        game_id: 0,
                        game_title: path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Unknown")
                            .to_string(),
                        file_path: path.to_string_lossy().to_string(),
                        expected_md5: None,
                    };

                    if let Err(e) = start_extraction(&state, path.clone(), queue_item).await {
                        error!("Auto-extraction failed: {}", e);
                    } else {
                        // Move to processed folder to avoid re-extraction
                        let processed_dir = watch_dir.join("processed");
                        if std::fs::create_dir_all(&processed_dir).is_ok() {
                            let new_path = processed_dir.join(path.file_name().unwrap());
                            if let Err(e) = std::fs::rename(&path, &new_path) {
                                error!("Failed to move processed archive: {}", e);
                            } else {
                                info!("Moved processed archive to: {:?}", new_path);
                            }
                        }
                    }
                }
            }
        }
    }
}
