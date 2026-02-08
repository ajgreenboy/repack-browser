use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Clone)]
pub struct RealDebridClient {
    api_key: String,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct TorrentInfo {
    #[allow(dead_code)]
    id: String,
    status: String,
    links: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AddMagnetRequest {
    magnet: String,
}

#[derive(Debug, Deserialize)]
struct AddMagnetResponse {
    id: String,
    #[allow(dead_code)]
    uri: String,
}

#[derive(Debug, Serialize)]
struct SelectFilesRequest {
    files: String, // "all" or comma-separated file IDs
}

#[derive(Debug, Deserialize)]
struct UnrestrictResponse {
    download: String,
    #[allow(dead_code)]
    filename: String,
    #[allow(dead_code)]
    filesize: Option<i64>,
}

impl RealDebridClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Convert a magnet link to a direct download link via Real-Debrid
    pub async fn convert_magnet(&self, magnet: &str) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        // Step 1: Add magnet to Real-Debrid
        let add_response: AddMagnetResponse = self
            .client
            .post("https://api.real-debrid.com/rest/1.0/torrents/addMagnet")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&AddMagnetRequest {
                magnet: magnet.to_string(),
            })
            .send()
            .await?
            .json()
            .await?;

        let torrent_id = add_response.id;
        log::info!("Added magnet to Real-Debrid: {}", torrent_id);

        // Step 2: Select all files
        self.client
            .post(&format!(
                "https://api.real-debrid.com/rest/1.0/torrents/selectFiles/{}",
                torrent_id
            ))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&SelectFilesRequest {
                files: "all".to_string(),
            })
            .send()
            .await?;

        // Step 3: Wait for torrent to be ready and get links
        let mut attempts = 0;
        let torrent_info = loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let info: TorrentInfo = self
                .client
                .get(&format!(
                    "https://api.real-debrid.com/rest/1.0/torrents/info/{}",
                    torrent_id
                ))
                .header("Authorization", format!("Bearer {}", self.api_key))
                .send()
                .await?
                .json()
                .await?;

            log::info!("Torrent status: {}", info.status);

            if info.status == "downloaded" || info.status == "waiting_files_selection" {
                break info;
            }

            attempts += 1;
            if attempts > 30 {
                return Err("Torrent took too long to download".into());
            }
        };

        // Step 4: Unrestrict all links
        let mut direct_links = Vec::new();

        for link in torrent_info.links {
            let unrestrict: UnrestrictResponse = self
                .client
                .post("https://api.real-debrid.com/rest/1.0/unrestrict/link")
                .header("Authorization", format!("Bearer {}", self.api_key))
                .form(&[("link", link)])
                .send()
                .await?
                .json()
                .await?;

            direct_links.push(unrestrict.download);
        }

        Ok(direct_links)
    }

    /// Unrestrict a single link (for DDL links that aren't magnets)
    pub async fn unrestrict_link(&self, link: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        let unrestrict: UnrestrictResponse = self
            .client
            .post("https://api.real-debrid.com/rest/1.0/unrestrict/link")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&[("link", link)])
            .send()
            .await?
            .json()
            .await?;

        Ok(unrestrict.download)
    }
}
