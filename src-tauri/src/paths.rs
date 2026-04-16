use std::path::{Path, PathBuf};

/// Detects whether the app is running in "installed" or "portable" mode.
/// Returns "installed" if the app has an MSI/NSIS uninstall entry in the registry,
/// or if the exe path is under a standard Program Files directory.
/// Returns "portable" otherwise (data stays in %APPDATA% / Roaming).
pub fn get_app_mode() -> &'static str {
    if is_installed() {
        "installed"
    } else {
        "portable"
    }
}

fn is_installed() -> bool {
    // Check 1: registry uninstall key (reliable for MSI/NSIS installs)
    if is_registered_install() {
        return true;
    }

    // Check 2: exe is under a standard Program Files directory
    if let Ok(exe_path) = std::env::current_exe() {
        let exe_str = exe_path.to_string_lossy().to_lowercase();
        if exe_str.contains("program files") {
            return true;
        }
    }

    false
}

#[cfg(windows)]
fn is_registered_install() -> bool {
    use std::process::Command;

    // Check the NSIS uninstall location (uses productName from tauri.conf.json)
    let nsis_check = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\SnipVault",
            "/v",
            "UninstallString",
        ])
        .output();

    if let Ok(output) = nsis_check {
        if output.status.success() {
            return true;
        }
    }

    // Check HKCU as well (NSIS sometimes installs per-user here)
    let nsis_hkcu = Command::new("reg")
        .args([
            "query",
            r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\SnipVault",
            "/v",
            "UninstallString",
        ])
        .output();

    if let Ok(output) = nsis_hkcu {
        if output.status.success() {
            return true;
        }
    }

    // Also check by bundle identifier from tauri.conf.json
    let bundle_check = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\cn.rainss.snipvault",
            "/v",
            "UninstallString",
        ])
        .output();

    if let Ok(output) = bundle_check {
        if output.status.success() {
            return true;
        }
    }

    false
}

#[cfg(not(windows))]
fn is_registered_install() -> bool {
    false
}

fn roaming_app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("SnipVault")
}

fn can_write_dir(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }

    let probe = dir.join(format!(
        ".snipvault-write-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Returns the data directory for the current app mode.
/// - Installed: prefer <exe_dir>/data, fallback to %APPDATA%/SnipVault/ (Roaming) when not writable
/// - Portable: %APPDATA%/SnipVault/ (Roaming)
pub fn get_data_dir() -> PathBuf {
    let mode = get_app_mode();
    let app_data_dir = roaming_app_data_dir();

    log::info!("get_data_dir: app_mode = {}", mode);

    if mode == "installed" {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let installed_data_dir = exe_dir.join("data");
                if can_write_dir(&installed_data_dir) {
                    log::info!("get_data_dir: resolved installed path = {:?}", installed_data_dir);
                    return installed_data_dir;
                }

                log::warn!(
                    "get_data_dir: installed path not writable, fallback to appdata: {:?}",
                    installed_data_dir
                );
            }
        }
    }

    log::info!("get_data_dir: resolved appdata path = {:?}", app_data_dir);
    app_data_dir
}

pub fn get_db_path() -> PathBuf {
    get_data_dir().join("snippets.db")
}

pub fn get_settings_dir() -> PathBuf {
    // Settings always go to the data dir
    let dir = get_data_dir();
    log::info!("get_settings_dir() = {:?}", dir);
    dir
}

pub fn get_settings_path() -> PathBuf {
    let path = get_settings_dir().join("settings.json");
    log::info!("get_settings_path() = {:?}", path);
    path
}
