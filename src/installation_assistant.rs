use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantAction {
    pub id: String,
    pub name: String,
    pub description: String,
    pub action_type: ActionType,
    pub required: bool,
    pub auto_applicable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    InstallDll,
    AddAvExclusion,
    InstallDependency,
    DisableAv,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantResult {
    pub success: bool,
    pub message: String,
    pub actions_taken: Vec<String>,
    pub errors: Vec<String>,
}

/// Get list of recommended actions based on system state
pub fn get_recommended_actions(
    missing_dlls: &[String],
    missing_dependencies: &[String],
    antivirus_active: bool,
    install_path: Option<&str>,
) -> Vec<AssistantAction> {
    let mut actions = Vec::new();

    // DLL installation actions
    for dll in missing_dlls {
        if dll.contains("unarc") {
            actions.push(AssistantAction {
                id: "install_unarc".to_string(),
                name: "Install unarc.dll".to_string(),
                description: "Download and install unarc.dll to System32 (required for FitGirl repacks)".to_string(),
                action_type: ActionType::InstallDll,
                required: true,
                auto_applicable: true,
            });
        }
        if dll.contains("ISDone") {
            actions.push(AssistantAction {
                id: "install_isdone".to_string(),
                name: "Install ISDone.dll".to_string(),
                description: "Download and install ISDone.dll to System32 (required for game installers)".to_string(),
                action_type: ActionType::InstallDll,
                required: true,
                auto_applicable: true,
            });
        }
    }

    // Dependency installation actions
    for dep in missing_dependencies {
        if dep.contains("DirectX") {
            actions.push(AssistantAction {
                id: "install_directx".to_string(),
                name: "Install DirectX Runtime".to_string(),
                description: "Download and install DirectX End-User Runtime (required for many games)".to_string(),
                action_type: ActionType::InstallDependency,
                required: true,
                auto_applicable: false, // Requires user interaction
            });
        }
        if dep.contains(".NET") {
            actions.push(AssistantAction {
                id: "install_dotnet".to_string(),
                name: "Install .NET Framework 4.8".to_string(),
                description: "Download and install .NET Framework 4.8 (required for some installers)".to_string(),
                action_type: ActionType::InstallDependency,
                required: true,
                auto_applicable: false,
            });
        }
        if dep.contains("Visual C++") {
            actions.push(AssistantAction {
                id: "install_vcredist".to_string(),
                name: format!("Install {}", dep),
                description: format!("Download and install {} (required for many games)", dep),
                action_type: ActionType::InstallDependency,
                required: true,
                auto_applicable: false,
            });
        }
    }

    // Antivirus actions
    if antivirus_active {
        if let Some(path) = install_path {
            actions.push(AssistantAction {
                id: "add_av_exclusion".to_string(),
                name: "Add Antivirus Exclusion".to_string(),
                description: format!("Add {} to Windows Defender exclusions to prevent installation failures", path),
                action_type: ActionType::AddAvExclusion,
                required: false,
                auto_applicable: true,
            });
        }

        actions.push(AssistantAction {
            id: "disable_av_temp".to_string(),
            name: "Temporarily Disable Antivirus".to_string(),
            description: "⚠️ Disable Windows Defender Real-Time Protection during installation (will re-enable after)".to_string(),
            action_type: ActionType::DisableAv,
            required: false,
            auto_applicable: false, // Requires explicit permission
        });
    }

    actions
}

/// Execute DLL installation
pub async fn install_dll(dll_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        let (url, filename) = match dll_name {
            "unarc" => (
                "https://github.com/FitGirl-Repacks/UnaRC/raw/master/unarc.dll",
                "unarc.dll"
            ),
            "ISDone" => (
                "https://github.com/FitGirl-Repacks/UnaRC/raw/master/ISDone.dll",
                "ISDone.dll"
            ),
            _ => return Err("Unknown DLL".into()),
        };

        // Download DLL
        let client = reqwest::Client::new();
        let bytes = client.get(url)
            .send()
            .await?
            .bytes()
            .await?;

        // Determine system directories
        let system32 = PathBuf::from("C:\\Windows\\System32");
        let syswow64 = PathBuf::from("C:\\Windows\\SysWOW64");

        // Write to both directories (for 32-bit and 64-bit support)
        let dest32 = system32.join(filename);
        let dest64 = syswow64.join(filename);

        std::fs::write(&dest32, &bytes)?;
        std::fs::write(&dest64, &bytes)?;

        Ok(format!("Successfully installed {} to System32 and SysWOW64", filename))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("DLL installation only supported on Windows".into())
    }
}

/// Add directory to Windows Defender exclusions
pub async fn add_av_exclusion(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        // Use PowerShell to add exclusion
        let script = format!("Add-MpPreference -ExclusionPath '{}'", path);

        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()?;

        if output.status.success() {
            Ok(format!("Added {} to Windows Defender exclusions", path))
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to add exclusion: {}", error).into())
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("Antivirus exclusions only supported on Windows".into())
    }
}

/// Temporarily disable Windows Defender Real-Time Protection
pub async fn disable_realtime_protection() -> Result<String, Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        let script = "Set-MpPreference -DisableRealtimeMonitoring $true";

        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .output()?;

        if output.status.success() {
            Ok("Disabled Windows Defender Real-Time Protection (remember to re-enable after installation)".to_string())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to disable protection: {} (may require administrator privileges)", error).into())
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("Antivirus control only supported on Windows".into())
    }
}

/// Re-enable Windows Defender Real-Time Protection
pub async fn enable_realtime_protection() -> Result<String, Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        let script = "Set-MpPreference -DisableRealtimeMonitoring $false";

        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .output()?;

        if output.status.success() {
            Ok("Re-enabled Windows Defender Real-Time Protection".to_string())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to re-enable protection: {}", error).into())
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("Antivirus control only supported on Windows".into())
    }
}

/// Get download URLs and instructions for dependencies
pub fn get_dependency_installer_info(dependency: &str) -> Option<DependencyInfo> {
    if dependency.contains("DirectX") {
        Some(DependencyInfo {
            name: "DirectX End-User Runtime".to_string(),
            url: "https://www.microsoft.com/en-us/download/details.aspx?id=35".to_string(),
            instructions: vec![
                "1. Download the DirectX End-User Runtime installer".to_string(),
                "2. Run the installer and follow the prompts".to_string(),
                "3. Restart your computer after installation".to_string(),
            ],
            auto_installable: false,
        })
    } else if dependency.contains(".NET Framework 4.8") {
        Some(DependencyInfo {
            name: ".NET Framework 4.8".to_string(),
            url: "https://dotnet.microsoft.com/download/dotnet-framework/net48".to_string(),
            instructions: vec![
                "1. Download .NET Framework 4.8 Runtime installer".to_string(),
                "2. Run the installer as administrator".to_string(),
                "3. Restart your computer after installation".to_string(),
            ],
            auto_installable: false,
        })
    } else if dependency.contains("Visual C++ 2015-2022") || dependency.contains("VC++ 2015-2022") {
        Some(DependencyInfo {
            name: "Visual C++ 2015-2022 Redistributable".to_string(),
            url: "https://aka.ms/vs/17/release/vc_redist.x64.exe".to_string(),
            instructions: vec![
                "1. Download and run vc_redist.x64.exe".to_string(),
                "2. Accept the license agreement and click Install".to_string(),
                "3. Restart if prompted".to_string(),
            ],
            auto_installable: true, // Can be silently installed
        })
    } else if dependency.contains("Visual C++ 2013") {
        Some(DependencyInfo {
            name: "Visual C++ 2013 Redistributable".to_string(),
            url: "https://aka.ms/highdpimfc2013x64enu".to_string(),
            instructions: vec![
                "1. Download and run vcredist_x64.exe".to_string(),
                "2. Accept the license agreement and click Install".to_string(),
            ],
            auto_installable: true,
        })
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInfo {
    pub name: String,
    pub url: String,
    pub instructions: Vec<String>,
    pub auto_installable: bool,
}

/// Download and silently install a dependency
pub async fn auto_install_dependency(dependency: &str) -> Result<String, Box<dyn std::error::Error>> {
    let info = get_dependency_installer_info(dependency)
        .ok_or("Unknown dependency")?;

    if !info.auto_installable {
        return Err("This dependency requires manual installation".into());
    }

    // Download installer
    let client = reqwest::Client::new();
    let bytes = client.get(&info.url)
        .send()
        .await?
        .bytes()
        .await?;

    // Save to temp file
    let temp_path = std::env::temp_dir().join(format!("{}.exe", dependency.replace(" ", "_")));
    std::fs::write(&temp_path, bytes)?;

    // Run silent installation
    let output = std::process::Command::new(&temp_path)
        .args(["/install", "/quiet", "/norestart"])
        .output()?;

    // Clean up
    let _ = std::fs::remove_file(&temp_path);

    if output.status.success() {
        Ok(format!("Successfully installed {}", info.name))
    } else {
        Err(format!("Installation failed: {}", String::from_utf8_lossy(&output.stderr)).into())
    }
}
