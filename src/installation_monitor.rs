use crate::db;
use crate::system_info::SystemInfo;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallMonitorState {
    pub log_id: i64,
    pub game_id: i64,
    pub started_at: String,
    pub status: MonitorStatus,
    pub ram_usage_peak_gb: f64,
    pub ram_usage_current_gb: f64,
    pub duration_seconds: u64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MonitorStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

pub struct InstallationMonitor {
    pool: SqlitePool,
    log_id: i64,
    game_id: i64,
    started_at: Instant,
    peak_ram_gb: Arc<RwLock<f64>>,
    is_running: Arc<RwLock<bool>>,
}

impl InstallationMonitor {
    /// Start a new installation monitor
    pub async fn start(
        pool: SqlitePool,
        game_id: i64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Create installation log entry
        let log_id = db::insert_installation_log(&pool, Some(game_id), "running").await?;

        let peak_ram_gb = Arc::new(RwLock::new(0.0));
        let is_running = Arc::new(RwLock::new(true));

        let monitor = Self {
            pool,
            log_id,
            game_id,
            started_at: Instant::now(),
            peak_ram_gb,
            is_running,
        };

        // Spawn background task to monitor RAM
        monitor.spawn_ram_monitor();

        Ok(monitor)
    }

    /// Spawn background task to monitor RAM usage
    fn spawn_ram_monitor(&self) {
        let peak_ram = self.peak_ram_gb.clone();
        let is_running = self.is_running.clone();

        tokio::spawn(async move {
            while *is_running.read().await {
                let sys_info = SystemInfo::gather().await;

                let ram_used = sys_info.ram_total_gb - sys_info.ram_available_gb;

                let mut peak = peak_ram.write().await;
                if ram_used > *peak {
                    *peak = ram_used;
                }
                drop(peak);

                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }

    /// Get current monitor state
    pub async fn get_state(&self) -> InstallMonitorState {
        let duration = self.started_at.elapsed().as_secs();
        let peak_ram = *self.peak_ram_gb.read().await;

        // Get current RAM
        let sys_info = SystemInfo::gather().await;
        let current_ram = sys_info.ram_total_gb - sys_info.ram_available_gb;

        InstallMonitorState {
            log_id: self.log_id,
            game_id: self.game_id,
            started_at: chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::seconds(duration as i64))
                .unwrap()
                .to_rfc3339(),
            status: MonitorStatus::Running,
            ram_usage_peak_gb: peak_ram,
            ram_usage_current_gb: current_ram,
            duration_seconds: duration,
            error_message: None,
        }
    }

    /// Mark installation as completed successfully
    pub async fn complete(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Stop RAM monitoring
        *self.is_running.write().await = false;

        let duration_minutes = self.started_at.elapsed().as_secs() / 60;
        let peak_ram = *self.peak_ram_gb.read().await;

        db::update_installation_log(
            &self.pool,
            self.log_id,
            "completed",
            None,
            None,
            Some(peak_ram),
            Some(duration_minutes as i64),
        )
        .await?;

        Ok(())
    }

    /// Mark installation as failed
    pub async fn fail(
        &self,
        error_code: Option<String>,
        error_message: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Stop RAM monitoring
        *self.is_running.write().await = false;

        let duration_minutes = self.started_at.elapsed().as_secs() / 60;
        let peak_ram = *self.peak_ram_gb.read().await;

        db::update_installation_log(
            &self.pool,
            self.log_id,
            "failed",
            error_code,
            Some(error_message),
            Some(peak_ram),
            Some(duration_minutes as i64),
        )
        .await?;

        Ok(())
    }

    /// Mark installation as cancelled
    pub async fn cancel(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Stop RAM monitoring
        *self.is_running.write().await = false;

        let duration_minutes = self.started_at.elapsed().as_secs() / 60;
        let peak_ram = *self.peak_ram_gb.read().await;

        db::update_installation_log(
            &self.pool,
            self.log_id,
            "cancelled",
            None,
            Some("Installation cancelled by user".to_string()),
            Some(peak_ram),
            Some(duration_minutes as i64),
        )
        .await?;

        Ok(())
    }
}

/// Get installation history for a game
pub async fn get_installation_history(
    pool: &SqlitePool,
    game_id: i64,
) -> Result<Vec<db::InstallationLog>, sqlx::Error> {
    db::get_installation_logs_for_game(pool, game_id).await
}

/// Get all installation logs (for admin/debugging)
pub async fn get_all_installation_logs(
    pool: &SqlitePool,
) -> Result<Vec<db::InstallationLog>, sqlx::Error> {
    db::get_all_installation_logs(pool).await
}

/// Analyze failed installations and provide recommendations
pub fn analyze_installation_failure(log: &db::InstallationLog) -> Vec<String> {
    let mut recommendations = Vec::new();

    if let Some(ref error_msg) = log.error_message {
        let error_lower = error_msg.to_lowercase();

        // Common error patterns
        if error_lower.contains("unarc.dll") || error_lower.contains("isdone.dll") {
            recommendations.push("Missing DLL: Install unarc.dll and ISDone.dll to System32".to_string());
            recommendations.push("Use the Installation Assistant to auto-install required DLLs".to_string());
        }

        if error_lower.contains("access denied") || error_lower.contains("permission") {
            recommendations.push("Permission error: Run the installer as Administrator".to_string());
            recommendations.push("Add installation folder to antivirus exclusions".to_string());
        }

        if error_lower.contains("disk") || error_lower.contains("space") {
            recommendations.push("Insufficient disk space: Free up at least 50GB on your installation drive".to_string());
        }

        if error_lower.contains("memory") || error_lower.contains("ram") {
            recommendations.push("Low memory: Close unnecessary programs before installing".to_string());
            recommendations.push("Consider increasing virtual memory (page file) size".to_string());
        }

        if error_lower.contains("crc") || error_lower.contains("checksum") || error_lower.contains("corrupt") {
            recommendations.push("Corrupted files: Re-download the game files".to_string());
            recommendations.push("Verify MD5 checksums before installation".to_string());
        }

        if error_lower.contains("antivirus") || error_lower.contains("defender") {
            recommendations.push("Antivirus interference: Temporarily disable Windows Defender Real-Time Protection".to_string());
            recommendations.push("Add installation folder to antivirus exclusions before retrying".to_string());
        }
    }

    // RAM-based recommendations
    if let Some(peak_ram) = log.ram_usage_peak {
        if peak_ram > 12.0 {
            recommendations.push(format!(
                "High RAM usage ({:.1}GB peak): This is normal for large game installations",
                peak_ram
            ));
        }
    }

    // Duration-based recommendations
    if let Some(duration) = log.install_duration_minutes {
        if duration < 5 {
            recommendations.push("Installation failed quickly - likely a setup issue rather than file corruption".to_string());
        }
    }

    // Generic fallbacks
    if recommendations.is_empty() {
        recommendations.push("Run pre-installation check to identify system issues".to_string());
        recommendations.push("Ensure all Windows updates are installed".to_string());
        recommendations.push("Try running the installer in compatibility mode (Windows 7/8)".to_string());
    }

    recommendations
}

/// Get installation statistics for analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationStats {
    pub total_installs: i64,
    pub successful_installs: i64,
    pub failed_installs: i64,
    pub avg_duration_minutes: f64,
    pub avg_ram_usage_gb: f64,
    pub success_rate: f64,
}

pub async fn get_installation_stats(pool: &SqlitePool) -> Result<InstallationStats, sqlx::Error> {
    let logs = db::get_all_installation_logs(pool).await?;

    let total = logs.len() as i64;
    let successful = logs.iter().filter(|l| l.status == "completed").count() as i64;
    let failed = logs.iter().filter(|l| l.status == "failed").count() as i64;

    let total_duration: i64 = logs
        .iter()
        .filter_map(|l| l.install_duration_minutes)
        .sum();

    let total_ram: f64 = logs.iter().filter_map(|l| l.ram_usage_peak).sum();

    let avg_duration = if total > 0 {
        total_duration as f64 / total as f64
    } else {
        0.0
    };

    let avg_ram = if total > 0 {
        total_ram / total as f64
    } else {
        0.0
    };

    let success_rate = if total > 0 {
        (successful as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    Ok(InstallationStats {
        total_installs: total,
        successful_installs: successful,
        failed_installs: failed,
        avg_duration_minutes: avg_duration,
        avg_ram_usage_gb: avg_ram,
        success_rate,
    })
}
