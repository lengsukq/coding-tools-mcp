use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::settings::DownloadConfig;
use crate::tunnel::{
    install_software as install_binary, list_software as list_binaries,
    uninstall_software as uninstall_binary, SoftwareStatus,
};

/// List install status for frpc and cloudflared.
#[tauri::command]
pub async fn list_software() -> AppResult<Vec<SoftwareStatus>> {
    tauri::async_runtime::spawn_blocking(list_binaries)
        .await
        .map_err(|err| AppError::Message(format!("检测软件状态失败: {err}")))
}

/// Download-install the requested binary ("frpc" | "cloudflared").
#[tauri::command]
pub async fn install_software(kind: String) -> AppResult<SoftwareStatus> {
    install_binary(&kind).await
}

/// Remove a cache-managed binary ("frpc" | "cloudflared").
#[tauri::command]
pub async fn uninstall_software(kind: String) -> AppResult<SoftwareStatus> {
    tauri::async_runtime::spawn_blocking(move || uninstall_binary(&kind))
        .await
        .map_err(|err| AppError::Message(format!("卸载软件任务失败: {err}")))?
}

/// Read the download config (mirror + proxy).
#[tauri::command]
pub async fn get_download_config(app: AppHandle) -> AppResult<DownloadConfig> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .with_settings(|store| Ok(store.data().download.clone()))
    })
    .await
    .map_err(|err| AppError::Message(format!("读取下载配置失败: {err}")))?
}

/// Persist the download config (mirror + proxy).
#[tauri::command]
pub async fn set_download_config(app: AppHandle, config: DownloadConfig) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_settings(|store| {
            let mut settings = store.settings();
            settings.download = config;
            store.update_settings(settings)
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("保存下载配置失败: {err}")))?
}
