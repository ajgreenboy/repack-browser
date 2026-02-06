use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Serialize, Deserialize)]
pub struct AddMagnetResponse {
    pub id: String,
    pub uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TorrentInfo {
    pub id: String,
    pub filename: String,
    pub status: String,
    pub links: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnrestrictLinkResponse {
    pub download: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadLink {
    pub filename: String,
    pub download_url: String,
    pub size: Option<String>,
}

pub struct RealDebridClient {
    client: Client,
    api_key: String,
}

impl RealDebridClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
            api_key,
        }
    }
    
    /// Add a magnet link to Real-Debrid
    pub async fn add_magnet(&self, magnet_link: &str) -> Result<AddMagnetResponse, Box<dyn std::error::Error>> {
        let response = self.client
            .post("https://api.real-debrid.com/rest/1.0/torrents/addMagnet")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&[("magnet", magnet_link)])
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("Real-Debrid API error: {}", error_text).into());
        }
        
        let result: AddMagnetResponse = response.json().await?;
        Ok(result)
    }
    
    /// Select files from a torrent (use "all" to select all files)
    pub async fn select_files(&self, torrent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let response = self.client
            .post(&format!("https://api.real-debrid.com/rest/1.0/torrents/selectFiles/{}", torrent_id))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&[("files", "all")])
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("Real-Debrid select files error: {}", error_text).into());
        }
        
        Ok(())
    }
    
    /// Get information about a torrent
    pub async fn get_torrent_info(&self, torrent_id: &str) -> Result<TorrentInfo, Box<dyn std::error::Error>> {
        let response = self.client
            .get(&format!("https://api.real-debrid.com/rest/1.0/torrents/info/{}", torrent_id))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("Real-Debrid info error: {}", error_text).into());
        }
        
        let result: TorrentInfo = response.json().await?;
        Ok(result)
    }
    
    /// Wait for a torrent to be ready for download
    pub async fn wait_for_ready(&self, torrent_id: &str, max_wait_secs: u64) -> Result<TorrentInfo, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        
        loop {
            let info = self.get_torrent_info(torrent_id).await?;
            
            // Status can be: magnet_error, magnet_conversion, waiting_files_selection, queued, downloading, downloaded, error, virus, compressing, uploading, dead
            match info.status.as_str() {
                "downloaded" => return Ok(info),
                "error" | "magnet_error" | "virus" | "dead" => {
                    return Err(format!("Torrent failed with status: {}", info.status).into());
                }
                _ => {
                    // Still processing
                    if start.elapsed().as_secs() > max_wait_secs {
                        return Err("Timeout waiting for torrent to be ready".into());
                    }
                    
                    // Wait 2 seconds before checking again
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }
    
    /// Unrestrict a link to get the direct download URL
    pub async fn unrestrict_link(&self, link: &str) -> Result<UnrestrictLinkResponse, Box<dyn std::error::Error>> {
        let response = self.client
            .post("https://api.real-debrid.com/rest/1.0/unrestrict/link")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&[("link", link)])
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("Real-Debrid unrestrict error: {}", error_text).into());
        }
        
        let result: UnrestrictLinkResponse = response.json().await?;
        Ok(result)
    }
    
    /// Process a magnet link and return download links
    /// This is the main function that does everything: add, select, wait, unrestrict
    pub async fn process_magnet(&self, magnet_link: &str) -> Result<Vec<DownloadLink>, Box<dyn std::error::Error>> {
        println!("Processing magnet link...");
        
        // Step 1: Add magnet to Real-Debrid
        let add_result = self.add_magnet(magnet_link).await?;
        println!("Added magnet with ID: {}", add_result.id);
        
        // Step 2: Select all files
        self.select_files(&add_result.id).await?;
        println!("Selected all files");
        
        // Step 3: Wait for torrent to be ready (5 minute timeout)
        // If cached, this should be instant. If not, Real-Debrid will download it.
        println!("Waiting for torrent to be ready...");
        let info = self.wait_for_ready(&add_result.id, 300).await?;
        println!("Torrent ready! Found {} files", info.links.len());
        
        // Step 4: Unrestrict all download links
        let mut downloads = Vec::new();
        for (idx, link) in info.links.iter().enumerate() {
            match self.unrestrict_link(link).await {
                Ok(unrestricted) => {
                    println!("Unrestricted file {}/{}: {}", idx + 1, info.links.len(), unrestricted.filename);
                    downloads.push(DownloadLink {
                        filename: unrestricted.filename,
                        download_url: unrestricted.download,
                        size: None, // Real-Debrid API doesn't provide size in unrestrict response
                    });
                }
                Err(e) => {
                    eprintln!("Failed to unrestrict link {}: {}", link, e);
                }
            }
        }
        
        Ok(downloads)
    }
}
