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

#[derive(Debug, Deserialize)]
pub struct DownloadQueueItem {
    pub game_id: i64,
    pub game_title: String,
    pub file_path: String,
    pub expected_md5: Option<String>,
}

impl ServerClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    pub async fn register(
        &self,
        client_id: &str,
        client_name: &str,
        os_version: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
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
    ) -> Result<Vec<DownloadQueueItem>, Box<dyn std::error::Error>> {
        let url = format!("{}/api/clients/{}/queue", self.base_url, client_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let queue: Vec<DownloadQueueItem> = response.json().await?;
        Ok(queue)
    }

    pub async fn report_progress(
        &self,
        client_id: &str,
        progress: &ExtractionProgress,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
    ) -> Result<(), Box<dyn std::error::Error>> {
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
