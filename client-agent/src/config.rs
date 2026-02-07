use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub client: ClientConfig,
    pub server: ServerConfig,
    pub extraction: ExtractionConfig,
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub url: String,
    pub enabled: bool,
    pub poll_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfig {
    pub output_dir: PathBuf,
    pub watch_dir: PathBuf,
    pub delete_after_extract: bool,
    pub verify_md5: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub report_interval_secs: u64,
    pub track_ram_usage: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            client: ClientConfig {
                id: String::new(), // Will be generated
                name: hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "Unknown PC".to_string()),
            },
            server: ServerConfig {
                url: "http://homelab:3030".to_string(),
                enabled: true,
                poll_interval_secs: 30,
            },
            extraction: ExtractionConfig {
                output_dir: PathBuf::from("C:\\Games"),
                watch_dir: PathBuf::from(
                    dirs::download_dir()
                        .unwrap_or_else(|| PathBuf::from("C:\\Users\\Public\\Downloads"))
                ),
                delete_after_extract: false,
                verify_md5: true,
            },
            monitoring: MonitoringConfig {
                report_interval_secs: 2,
                track_ram_usage: true,
            },
        }
    }
}

impl Config {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("RepackClient")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::config_path();

        if !path.exists() {
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }

        let contents = std::fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;

        let path = Self::config_path();
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;

        Ok(())
    }
}
