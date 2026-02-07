use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub ram_total_gb: f64,
    pub ram_available_gb: f64,
    pub temp_space_gb: f64,
    pub cpu_cores: i64,
    pub antivirus_active: bool,
    pub missing_dlls: Vec<String>,
    pub missing_dependencies: Vec<String>,
    pub overall_status: SystemStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SystemStatus {
    Ready,      // All checks passed
    Warning,    // Some issues but can proceed
    Critical,   // Serious issues, installation likely to fail
}

impl SystemInfo {
    pub async fn gather() -> Self {
        let ram_total_gb = get_total_ram_gb();
        let ram_available_gb = get_available_ram_gb();
        let temp_space_gb = get_temp_space_gb();
        let cpu_cores = get_cpu_cores();
        let antivirus_active = is_antivirus_active();
        let missing_dlls = check_missing_dlls();
        let missing_dependencies = check_missing_dependencies();

        // Determine overall status
        let overall_status = Self::calculate_status(
            ram_available_gb,
            temp_space_gb,
            &missing_dlls,
            &missing_dependencies,
        );

        Self {
            ram_total_gb,
            ram_available_gb,
            temp_space_gb,
            cpu_cores,
            antivirus_active,
            missing_dlls,
            missing_dependencies,
            overall_status,
        }
    }

    fn calculate_status(
        ram_available_gb: f64,
        temp_space_gb: f64,
        missing_dlls: &[String],
        missing_dependencies: &[String],
    ) -> SystemStatus {
        // Critical: Less than 4GB RAM or less than 10GB temp space
        if ram_available_gb < 4.0 || temp_space_gb < 10.0 {
            return SystemStatus::Critical;
        }

        // Warning: Missing critical DLLs or dependencies
        if !missing_dlls.is_empty() || !missing_dependencies.is_empty() {
            return SystemStatus::Warning;
        }

        // Warning: Low RAM (4-8GB) or low space (10-20GB)
        if ram_available_gb < 8.0 || temp_space_gb < 20.0 {
            return SystemStatus::Warning;
        }

        SystemStatus::Ready
    }

    pub fn get_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.ram_available_gb < 4.0 {
            issues.push(format!("⚠️ Critical: Only {:.1}GB RAM available (need 4GB minimum)", self.ram_available_gb));
        } else if self.ram_available_gb < 8.0 {
            issues.push(format!("⚠️ Warning: Only {:.1}GB RAM available (8GB recommended)", self.ram_available_gb));
        }

        if self.temp_space_gb < 10.0 {
            issues.push(format!("⚠️ Critical: Only {:.1}GB temp space (need 10GB minimum)", self.temp_space_gb));
        } else if self.temp_space_gb < 20.0 {
            issues.push(format!("⚠️ Warning: Only {:.1}GB temp space (20GB recommended)", self.temp_space_gb));
        }

        if self.antivirus_active {
            issues.push("⚠️ Warning: Antivirus is active (may cause installation failures)".to_string());
        }

        for dll in &self.missing_dlls {
            issues.push(format!("⚠️ Missing DLL: {}", dll));
        }

        for dep in &self.missing_dependencies {
            issues.push(format!("⚠️ Missing dependency: {}", dep));
        }

        issues
    }

    pub fn get_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();

        if self.ram_available_gb < 8.0 {
            recommendations.push("Close unnecessary programs to free up RAM".to_string());
        }

        if self.temp_space_gb < 20.0 {
            recommendations.push("Free up disk space on your temp drive".to_string());
        }

        if self.antivirus_active {
            recommendations.push("Consider temporarily disabling antivirus during installation".to_string());
            recommendations.push("Add the installation folder to antivirus exclusions".to_string());
        }

        if !self.missing_dlls.is_empty() {
            recommendations.push("Install missing DLL files before proceeding".to_string());
        }

        if !self.missing_dependencies.is_empty() {
            recommendations.push("Install missing dependencies (DirectX, .NET, VC++ Redistributables)".to_string());
        }

        recommendations
    }
}

// ─── RAM Detection ───

fn get_total_ram_gb() -> f64 {
    #[cfg(target_os = "windows")]
    {
        use std::mem;
        use winapi::um::sysinfoapi::{GetPhysicallyInstalledSystemMemory, MEMORYSTATUSEX, GlobalMemoryStatusEx};

        unsafe {
            let mut mem_kb: u64 = 0;
            if GetPhysicallyInstalledSystemMemory(&mut mem_kb) != 0 {
                return mem_kb as f64 / 1024.0 / 1024.0; // KB to GB
            }

            // Fallback to GlobalMemoryStatusEx
            let mut mem_status: MEMORYSTATUSEX = mem::zeroed();
            mem_status.dwLength = mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut mem_status) != 0 {
                return mem_status.ullTotalPhys as f64 / 1024.0 / 1024.0 / 1024.0; // Bytes to GB
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Linux fallback
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb as f64 / 1024.0 / 1024.0; // KB to GB
                        }
                    }
                }
            }
        }
    }

    0.0 // Unknown
}

fn get_available_ram_gb() -> f64 {
    #[cfg(target_os = "windows")]
    {
        use std::mem;
        use winapi::um::sysinfoapi::{MEMORYSTATUSEX, GlobalMemoryStatusEx};

        unsafe {
            let mut mem_status: MEMORYSTATUSEX = mem::zeroed();
            mem_status.dwLength = mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut mem_status) != 0 {
                return mem_status.ullAvailPhys as f64 / 1024.0 / 1024.0 / 1024.0; // Bytes to GB
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Linux fallback
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemAvailable:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb as f64 / 1024.0 / 1024.0; // KB to GB
                        }
                    }
                }
            }
        }
    }

    0.0 // Unknown
}

// ─── Disk Space Detection ───

fn get_temp_space_gb() -> f64 {
    let temp_dir = std::env::temp_dir();

    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use winapi::um::fileapi::GetDiskFreeSpaceExW;

        // Get drive root (e.g., C:\ from C:\Users\...)
        let temp_str = temp_dir.to_string_lossy();
        let drive_root = if temp_str.len() >= 3 && temp_str.chars().nth(1) == Some(':') {
            format!("{}:\\", &temp_str[0..1])
        } else {
            return 0.0;
        };

        let wide: Vec<u16> = OsStr::new(&drive_root)
            .encode_wide()
            .chain(Some(0))
            .collect();

        unsafe {
            let mut free_bytes: u64 = 0;
            let mut total_bytes: u64 = 0;
            let mut total_free_bytes: u64 = 0;

            if GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_bytes as *mut u64 as *mut _,
                &mut total_bytes as *mut u64 as *mut _,
                &mut total_free_bytes as *mut u64 as *mut _,
            ) != 0
            {
                return free_bytes as f64 / 1024.0 / 1024.0 / 1024.0; // Bytes to GB
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Linux fallback using statvfs
        use std::os::unix::fs::MetadataExt;
        if let Ok(metadata) = std::fs::metadata(&temp_dir) {
            // Simplified - just return a placeholder
            // Real implementation would use statvfs
            return 50.0;
        }
    }

    0.0 // Unknown
}

// ─── CPU Detection ───

fn get_cpu_cores() -> i64 {
    num_cpus::get() as i64
}

// ─── Antivirus Detection ───

fn is_antivirus_active() -> bool {
    #[cfg(target_os = "windows")]
    {
        // Check Windows Defender status via PowerShell
        if let Ok(output) = std::process::Command::new("powershell")
            .args(["-Command", "Get-MpComputerStatus | Select-Object -ExpandProperty AntivirusEnabled"])
            .output()
        {
            if output.status.success() {
                let result = String::from_utf8_lossy(&output.stdout);
                return result.trim() == "True";
            }
        }

        // Fallback: check if Windows Defender service is running
        if let Ok(output) = std::process::Command::new("sc")
            .args(["query", "WinDefend"])
            .output()
        {
            if output.status.success() {
                let result = String::from_utf8_lossy(&output.stdout);
                return result.contains("RUNNING");
            }
        }
    }

    false // Unknown or not Windows
}

// ─── DLL Detection ───

fn check_missing_dlls() -> Vec<String> {
    let mut missing = Vec::new();

    // Critical DLLs for FitGirl installations
    let critical_dlls = [
        ("unarc.dll", vec!["C:\\Windows\\System32", "C:\\Windows\\SysWOW64"]),
        ("ISDone.dll", vec!["C:\\Windows\\System32", "C:\\Windows\\SysWOW64"]),
    ];

    for (dll_name, search_paths) in &critical_dlls {
        let mut found = false;

        for base_path in search_paths {
            let dll_path = Path::new(base_path).join(dll_name);
            if dll_path.exists() {
                found = true;
                break;
            }
        }

        if !found {
            missing.push(dll_name.to_string());
        }
    }

    missing
}

// ─── Dependency Detection ───

fn check_missing_dependencies() -> Vec<String> {
    let mut missing = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // Check DirectX
        if !is_directx_installed() {
            missing.push("DirectX Runtime".to_string());
        }

        // Check .NET Framework
        if !is_dotnet_installed() {
            missing.push(".NET Framework 4.8".to_string());
        }

        // Check Visual C++ Redistributables
        let vc_redist_missing = check_vcredist();
        missing.extend(vc_redist_missing);
    }

    missing
}

#[cfg(target_os = "windows")]
fn is_directx_installed() -> bool {
    // Check for d3dx9_43.dll (DirectX 9.0c)
    let dx_paths = [
        "C:\\Windows\\System32\\d3dx9_43.dll",
        "C:\\Windows\\SysWOW64\\d3dx9_43.dll",
    ];

    dx_paths.iter().any(|path| Path::new(path).exists())
}

#[cfg(target_os = "windows")]
fn is_dotnet_installed() -> bool {
    // Check registry for .NET 4.8
    if let Ok(output) = std::process::Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full", "/v", "Release"])
        .output()
    {
        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout);
            // .NET 4.8 = Release >= 528040
            if let Some(line) = result.lines().find(|l| l.contains("Release")) {
                if let Some(value_str) = line.split_whitespace().last() {
                    if let Ok(release) = value_str.trim_start_matches("0x").parse::<i32>() {
                        return release >= 528040;
                    }
                }
            }
        }
    }

    false
}

#[cfg(target_os = "windows")]
fn check_vcredist() -> Vec<String> {
    let mut missing = Vec::new();

    // Common VC++ Redistributables needed for games
    let required_versions = [
        ("2015-2022", r"SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64"),
        ("2013", r"SOFTWARE\Microsoft\VisualStudio\12.0\VC\Runtimes\x64"),
        ("2012", r"SOFTWARE\Microsoft\VisualStudio\11.0\VC\Runtimes\x64"),
        ("2010", r"SOFTWARE\Microsoft\VisualStudio\10.0\VC\VCRedist\x64"),
    ];

    for (version, reg_path) in &required_versions {
        let result = std::process::Command::new("reg")
            .args(["query", &format!("HKLM\\{}", reg_path)])
            .output();

        if let Ok(output) = result {
            if !output.status.success() {
                missing.push(format!("Visual C++ {} Redistributable", version));
            }
        } else {
            missing.push(format!("Visual C++ {} Redistributable", version));
        }
    }

    missing
}
