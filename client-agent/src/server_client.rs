use crate::extractor::ExtractionProgress;
use crate::system_info::SystemInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ServerClient {
    base_url: String,
    client: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct RegisterRequest {
    client_id: String,
    client_name: String,
    os_version: String,
}

#[derive(Debug, Deserialize)]
struct RegisterResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DownloadQueueItem {
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

#[derive(Debug, Serialize)]
pub struct ProgressUpdate {
    pub status: String,
    pub progress: f64,
    pub download_speed: Option<String>,
    pub eta: Option<String>,
    pub error_message: Option<String>,
}

impl ServerClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    pub async fn register(
        &self,
        client_id: &str,
        client_name: &str,
        os_version: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/clients/register", self.base_url);

        let request = RegisterRequest {
            client_id: client_id.to_string(),
            client_name: client_name.to_string(),
            os_version: os_version.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Registration failed: {}", response.status()).into());
        }

        let result: RegisterResponse = response.json().await?;
        Ok(result.success)
    }

    pub async fn get_download_queue(
        &self,
        client_id: &str,
    ) -> Result<Vec<DownloadQueueItem>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/downloads/queue?client_id={}", self.base_url, client_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let queue: Vec<DownloadQueueItem> = response.json().await?;
        Ok(queue)
    }

    pub async fn update_download_progress(
        &self,
        download_id: i64,
        update: &ProgressUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/downloads/{}/progress", self.base_url, download_id);

        let response = self.client
            .post(&url)
            .json(update)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to update progress: {}", response.status()).into());
        }

        Ok(())
    }

    pub async fn report_progress(
        &self,
        client_id: &str,
        progress: &ExtractionProgress,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/clients/{}/progress", self.base_url, client_id);

        self.client
            .post(&url)
            .json(progress)
            .send()
            .await?;

        Ok(())
    }

    pub async fn report_system_info(
        &self,
        system_info: &SystemInfo,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "{}/api/clients/{}/system-info",
            self.base_url, system_info.client_id
        );

        self.client
            .post(&url)
            .json(system_info)
            .send()
            .await?;

        Ok(())
    }

    pub async fn health_check(&self) -> bool {
        let url = format!("{}/api/health", self.base_url);

        self.client
            .get(&url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}
