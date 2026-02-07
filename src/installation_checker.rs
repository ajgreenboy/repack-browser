use crate::db;
use crate::system_info::{SystemInfo, SystemStatus};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreInstallCheckResult {
    pub can_proceed: bool,
    pub overall_status: CheckStatus,
    pub system_info: SystemInfo,
    pub game_requirements: Option<db::GameRequirement>,
    pub checks: Vec<CheckItem>,
    pub warnings: Vec<String>,
    pub blockers: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,       // All checks passed, safe to proceed
    Warning,    // Some issues but can proceed with caution
    Blocked,    // Critical issues, installation will likely fail
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckItem {
    pub name: String,
    pub status: CheckItemStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckItemStatus {
    Pass,
    Warning,
    Fail,
}

pub async fn check_pre_installation(
    pool: &SqlitePool,
    game_id: i64,
) -> Result<PreInstallCheckResult, Box<dyn std::error::Error>> {
    // Gather system information
    let system_info = SystemInfo::gather().await;

    // Get game details
    let game = db::get_game_by_id(pool, game_id).await?;

    // Get game requirements if available
    let game_requirements = db::get_game_requirements(pool, game_id).await.ok().flatten();

    // Perform checks
    let mut checks = Vec::new();
    let mut warnings = Vec::new();
    let mut blockers = Vec::new();
    let mut recommendations = Vec::new();

    // Check 1: RAM availability
    let ram_check = check_ram(&system_info, &game_requirements);
    checks.push(ram_check.clone());
    match ram_check.status {
        CheckItemStatus::Fail => blockers.push(ram_check.message.clone()),
        CheckItemStatus::Warning => warnings.push(ram_check.message.clone()),
        _ => {}
    }

    // Check 2: Disk space
    let disk_check = check_disk_space(&system_info, &game, &game_requirements);
    checks.push(disk_check.clone());
    match disk_check.status {
        CheckItemStatus::Fail => blockers.push(disk_check.message.clone()),
        CheckItemStatus::Warning => warnings.push(disk_check.message.clone()),
        _ => {}
    }

    // Check 3: Required DLLs
    let dll_check = check_dlls(&system_info);
    checks.push(dll_check.clone());
    match dll_check.status {
        CheckItemStatus::Fail => blockers.push(dll_check.message.clone()),
        CheckItemStatus::Warning => warnings.push(dll_check.message.clone()),
        _ => {}
    }

    // Check 4: Dependencies
    let dep_check = check_dependencies(&system_info, &game_requirements);
    checks.push(dep_check.clone());
    match dep_check.status {
        CheckItemStatus::Fail => blockers.push(dep_check.message.clone()),
        CheckItemStatus::Warning => warnings.push(dep_check.message.clone()),
        _ => {}
    }

    // Check 5: Antivirus
    let av_check = check_antivirus(&system_info);
    checks.push(av_check.clone());
    match av_check.status {
        CheckItemStatus::Warning => warnings.push(av_check.message.clone()),
        _ => {}
    }

    // Check 6: CPU
    let cpu_check = check_cpu(&system_info, &game_requirements);
    checks.push(cpu_check.clone());
    match cpu_check.status {
        CheckItemStatus::Warning => warnings.push(cpu_check.message.clone()),
        _ => {}
    }

    // Determine overall status
    let overall_status = if !blockers.is_empty() {
        CheckStatus::Blocked
    } else if !warnings.is_empty() {
        CheckStatus::Warning
    } else {
        CheckStatus::Pass
    };

    let can_proceed = matches!(overall_status, CheckStatus::Pass | CheckStatus::Warning);

    // Generate recommendations
    recommendations.extend(system_info.get_recommendations());

    if system_info.antivirus_active {
        recommendations.push("Add installation folder to antivirus exclusions before proceeding".to_string());
    }

    if !system_info.missing_dlls.is_empty() {
        recommendations.push(
            "Download and install missing DLLs from https://www.dll-files.com/ or use the auto-installer".to_string()
        );
    }

    if !blockers.is_empty() {
        recommendations.push("⚠️ DO NOT PROCEED - Critical issues must be resolved first".to_string());
    }

    Ok(PreInstallCheckResult {
        can_proceed,
        overall_status,
        system_info,
        game_requirements,
        checks,
        warnings,
        blockers,
        recommendations,
    })
}

fn check_ram(system_info: &SystemInfo, game_reqs: &Option<db::GameRequirement>) -> CheckItem {
    let available = system_info.ram_available_gb;

    // Check against game requirements if available
    if let Some(reqs) = game_reqs {
        if let Some(rec_ram) = reqs.rec_ram_gb {
            if available < rec_ram as f64 {
                if let Some(min_ram) = reqs.min_ram_gb {
                    if available < min_ram as f64 {
                        return CheckItem {
                            name: "RAM".to_string(),
                            status: CheckItemStatus::Fail,
                            message: format!(
                                "Insufficient RAM: {:.1}GB available, {}GB minimum required",
                                available, min_ram
                            ),
                        };
                    }
                }
                return CheckItem {
                    name: "RAM".to_string(),
                    status: CheckItemStatus::Warning,
                    message: format!(
                        "Low RAM: {:.1}GB available, {}GB recommended",
                        available, rec_ram
                    ),
                };
            }
        }
    }

    // General RAM check
    if available < 4.0 {
        CheckItem {
            name: "RAM".to_string(),
            status: CheckItemStatus::Fail,
            message: format!("Critical: Only {:.1}GB RAM available, 4GB minimum required", available),
        }
    } else if available < 8.0 {
        CheckItem {
            name: "RAM".to_string(),
            status: CheckItemStatus::Warning,
            message: format!("Low RAM: {:.1}GB available, 8GB recommended", available),
        }
    } else {
        CheckItem {
            name: "RAM".to_string(),
            status: CheckItemStatus::Pass,
            message: format!("✓ RAM: {:.1}GB available", available),
        }
    }
}

fn check_disk_space(
    system_info: &SystemInfo,
    game: &db::Game,
    game_reqs: &Option<db::GameRequirement>,
) -> CheckItem {
    let available = system_info.temp_space_gb;

    // Try to parse game file size
    let estimated_space_needed = if let Some(reqs) = game_reqs {
        reqs.disk_space_gb.unwrap_or(20) as f64
    } else {
        // Parse from file_size string (e.g., "50 GB")
        parse_size_to_gb(&game.file_size).unwrap_or(20.0)
    };

    // Installation typically needs 2-3x the compressed size
    let install_space_needed = estimated_space_needed * 2.5;

    if available < install_space_needed {
        CheckItem {
            name: "Disk Space".to_string(),
            status: CheckItemStatus::Fail,
            message: format!(
                "Insufficient disk space: {:.1}GB available, {:.1}GB needed for installation",
                available, install_space_needed
            ),
        }
    } else if available < install_space_needed * 1.5 {
        CheckItem {
            name: "Disk Space".to_string(),
            status: CheckItemStatus::Warning,
            message: format!(
                "Low disk space: {:.1}GB available, {:.1}GB recommended",
                available,
                install_space_needed * 1.5
            ),
        }
    } else {
        CheckItem {
            name: "Disk Space".to_string(),
            status: CheckItemStatus::Pass,
            message: format!("✓ Disk Space: {:.1}GB available", available),
        }
    }
}

fn check_dlls(system_info: &SystemInfo) -> CheckItem {
    if system_info.missing_dlls.is_empty() {
        CheckItem {
            name: "Required DLLs".to_string(),
            status: CheckItemStatus::Pass,
            message: "✓ All required DLLs present".to_string(),
        }
    } else {
        // unarc.dll and ISDone.dll are critical for FitGirl repacks
        let critical = system_info
            .missing_dlls
            .iter()
            .any(|dll| dll.contains("unarc") || dll.contains("ISDone"));

        if critical {
            CheckItem {
                name: "Required DLLs".to_string(),
                status: CheckItemStatus::Fail,
                message: format!(
                    "Missing critical DLLs: {} - Installation will fail",
                    system_info.missing_dlls.join(", ")
                ),
            }
        } else {
            CheckItem {
                name: "Required DLLs".to_string(),
                status: CheckItemStatus::Warning,
                message: format!(
                    "Missing DLLs: {} - May cause issues",
                    system_info.missing_dlls.join(", ")
                ),
            }
        }
    }
}

fn check_dependencies(
    system_info: &SystemInfo,
    game_reqs: &Option<db::GameRequirement>,
) -> CheckItem {
    let mut missing = system_info.missing_dependencies.clone();

    // Check against game-specific requirements
    if let Some(reqs) = game_reqs {
        if let Some(ref dx) = reqs.requires_directx {
            if !dx.is_empty() && system_info.missing_dependencies.iter().any(|d| d.contains("DirectX")) {
                missing.push(format!("DirectX {} (required by game)", dx));
            }
        }
        if let Some(ref dotnet) = reqs.requires_dotnet {
            if !dotnet.is_empty() && system_info.missing_dependencies.iter().any(|d| d.contains(".NET")) {
                missing.push(format!(".NET {} (required by game)", dotnet));
            }
        }
    }

    missing.sort();
    missing.dedup();

    if missing.is_empty() {
        CheckItem {
            name: "Dependencies".to_string(),
            status: CheckItemStatus::Pass,
            message: "✓ All dependencies installed".to_string(),
        }
    } else {
        CheckItem {
            name: "Dependencies".to_string(),
            status: CheckItemStatus::Warning,
            message: format!(
                "Missing dependencies: {} - Install before proceeding",
                missing.join(", ")
            ),
        }
    }
}

fn check_antivirus(system_info: &SystemInfo) -> CheckItem {
    if system_info.antivirus_active {
        CheckItem {
            name: "Antivirus".to_string(),
            status: CheckItemStatus::Warning,
            message: "Antivirus is active - May interfere with installation (consider adding exclusions)".to_string(),
        }
    } else {
        CheckItem {
            name: "Antivirus".to_string(),
            status: CheckItemStatus::Pass,
            message: "✓ No active antivirus detected".to_string(),
        }
    }
}

fn check_cpu(system_info: &SystemInfo, game_reqs: &Option<db::GameRequirement>) -> CheckItem {
    let cores = system_info.cpu_cores;

    if let Some(reqs) = game_reqs {
        if let Some(ref min_cpu) = reqs.min_cpu {
            // Simple heuristic: assume min_cpu mentions core count
            if min_cpu.contains("quad") || min_cpu.contains("4") {
                if cores < 4 {
                    return CheckItem {
                        name: "CPU".to_string(),
                        status: CheckItemStatus::Warning,
                        message: format!(
                            "Low CPU: {} cores available, 4+ cores recommended ({})",
                            cores, min_cpu
                        ),
                    };
                }
            }
        }
    }

    if cores < 2 {
        CheckItem {
            name: "CPU".to_string(),
            status: CheckItemStatus::Warning,
            message: format!("Low CPU: {} core(s) - 4+ cores recommended", cores),
        }
    } else {
        CheckItem {
            name: "CPU".to_string(),
            status: CheckItemStatus::Pass,
            message: format!("✓ CPU: {} cores available", cores),
        }
    }
}

/// Parse size string like "50 GB" to GB as f64
fn parse_size_to_gb(size_str: &str) -> Option<f64> {
    let cleaned = size_str.to_lowercase().replace(",", "");

    if cleaned.contains("gb") {
        cleaned
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<f64>().ok())
    } else if cleaned.contains("mb") {
        cleaned
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .map(|mb| mb / 1024.0)
    } else {
        None
    }
}
