use std::path::PathBuf;
use std::sync::OnceLock;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

use crate::error::{AppError, AppResult};
use crate::platform::paths::{append_if_exists, resolve_from_path};

static ROAMING_APP_DATA: OnceLock<PathBuf> = OnceLock::new();

pub fn roaming_app_data() -> AppResult<PathBuf> {
    if let Some(cached) = ROAMING_APP_DATA.get() {
        return Ok(cached.clone());
    }
    let resolved = unsafe {
        let raw = SHGetKnownFolderPath(&FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, None)
            .map_err(|err| AppError::Message(format!("SHGetKnownFolderPath failed: {err}")))?;
        let text_result = raw
            .to_string()
            .map_err(|err| AppError::Message(err.to_string()));
        // SHGetKnownFolderPath allocates the PWSTR on the COM task allocator;
        // always free it regardless of UTF-16 conversion outcome.
        CoTaskMemFree(Some(raw.0.cast()));
        PathBuf::from(text_result?)
    };
    let _ = ROAMING_APP_DATA.set(resolved.clone());
    Ok(resolved)
}

pub fn cloudflared_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(found) = resolve_from_path("cloudflared") {
        paths.push(found);
    }
    append_if_exists(&mut paths, r"C:\Program Files\cloudflared\cloudflared.exe");
    append_if_exists(
        &mut paths,
        r"C:\Program Files (x86)\cloudflared\cloudflared.exe",
    );
    if let Some(home) = dirs::home_dir() {
        append_if_exists(
            &mut paths,
            home.join(".cloudflared").join("cloudflared.exe"),
        );
    }
    paths
}

pub fn frpc_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(found) = resolve_from_path("frpc") {
        paths.push(found);
    }
    append_if_exists(&mut paths, r"C:\Program Files\frp\frpc.exe");
    if let Some(home) = dirs::home_dir() {
        append_if_exists(&mut paths, home.join(".frp").join("frpc.exe"));
    }
    paths
}
