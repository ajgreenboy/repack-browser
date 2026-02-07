use serde::{Deserialize, Serialize};
use std::path::Path;
use sysinfo::{System, SystemExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub client_id: String,
    pub client_name: String,
    pub ram_total_gb: f64,
    pub ram_available_gb: f64,
    pub disk_space_gb: f64,
    pub cpu_cores: usize,
    pub missing_dlls: Vec<String>,
    pub os_version: String,
}

pub fn gather_system_info(client_id: &str, client_name: &str) -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let ram_total_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let ram_available_gb = sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0;

    // Get disk space for C: drive
    let disk_space_gb = get_disk_space_gb("C:\\");

    let cpu_cores = sys.cpus().len();

    let missing_dlls = check_missing_dlls();

    let os_version = format!(
        "{} {}",
        System::name().unwrap_or_else(|| "Windows".to_string()),
        System::os_version().unwrap_or_else(|| "Unknown".to_string())
    );

    SystemInfo {
        client_id: client_id.to_string(),
        client_name: client_name.to_string(),
        ram_total_gb,
        ram_available_gb,
        disk_space_gb,
        cpu_cores,
        missing_dlls,
        os_version,
    }
}

#[cfg(target_os = "windows")]
fn get_disk_space_gb(drive: &str) -> f64 {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::fileapi::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = OsStr::new(drive)
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
            return free_bytes as f64 / 1024.0 / 1024.0 / 1024.0;
        }
    }

    0.0
}

#[cfg(not(target_os = "windows"))]
fn get_disk_space_gb(_drive: &str) -> f64 {
    0.0
}

fn check_missing_dlls() -> Vec<String> {
    let mut missing = Vec::new();

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
