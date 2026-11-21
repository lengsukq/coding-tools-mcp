use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::commands::runtime::restart_global_mcp;
use crate::error::{AppError, AppResult};
use crate::lan_peer::{
    self, DiscoveredPeerDto, LocalDeviceDto, PairedPeerStatusDto, PairingPinDto,
};
use crate::settings::LanFederationConfig;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanOverviewDto {
    pub local_device: LocalDeviceDto,
    pub config: LanFederationConfig,
    pub active_pin: Option<PairingPinDto>,
    pub discovered_peers: Vec<DiscoveredPeerDto>,
    pub paired_peers: Vec<PairedPeerStatusDto>,
    pub mcp_running: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanSettingsUpdateDto {
    pub enabled: bool,
    pub device_name: String,
    pub share_mode: String,
    pub shared_workspace_ids: Vec<String>,
    pub trust_level: String,
    pub allow_computer_use: bool,
    #[serde(default)]
    pub extra_subnets: Vec<String>,
    #[serde(default)]
    pub allow_local_noauth: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanSettingsSaveResultDto {
    pub overview: LanOverviewDto,
    pub listener_restarted: bool,
}

async fn build_lan_overview(app: &AppHandle, force_refresh: bool) -> AppResult<LanOverviewDto> {
    let app_handle = app.clone();
    let (local_device, config, active_pin, discovered_peers, mcp_running) =
        tauri::async_runtime::spawn_blocking(move || -> AppResult<_> {
            let state = app_handle.state::<AppState>();
            let _ = lan_peer::ensure_local_identity().map_err(AppError::Message)?;
            lan_peer::sync_discovery_daemon();

            let workspace_count = state
                .gateway
                .registry
                .local_descriptors()
                .map(|items| items.len())
                .unwrap_or_default();
            let mcp_running = state.with_runtime(|rt| {
                rt.refresh_mcp();
                Ok(rt.is_running())
            })?;
            let settings = state.with_settings(|store| Ok(store.settings()))?;
            let local_device = lan_peer::local_device_summary(Some(&settings), workspace_count);
            let active_pin = lan_peer::current_pairing_pin();
            let discovered_peers = lan_peer::list_discovered_peers();
            Ok((
                local_device,
                settings.lan_federation,
                active_pin,
                discovered_peers,
                mcp_running,
            ))
        })
        .await
        .map_err(|err| AppError::Message(format!("加载局域网概览任务失败: {err}")))??;

    let paired_peers = lan_peer::list_paired_peer_statuses_async(None, force_refresh).await;

    Ok(LanOverviewDto {
        local_device,
        config,
        active_pin,
        discovered_peers,
        paired_peers,
        mcp_running,
    })
}

#[tauri::command]
pub async fn get_lan_peers_overview(
    app: AppHandle,
    force_refresh: Option<bool>,
) -> AppResult<LanOverviewDto> {
    build_lan_overview(&app, force_refresh.unwrap_or(false)).await
}

#[tauri::command]
pub async fn save_lan_federation_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    input: LanSettingsUpdateDto,
) -> AppResult<LanSettingsSaveResultDto> {
    let share_mode = input.share_mode.trim().to_string();
    if !matches!(share_mode.as_str(), "all" | "selected") {
        return Err(AppError::Message(format!(
            "不支持的工作区共享模式: {share_mode}"
        )));
    }
    let trust_level = input.trust_level.trim().to_string();
    if !matches!(trust_level.as_str(), "full" | "readonly") {
        return Err(AppError::Message(format!(
            "不支持的局域网信任级别: {trust_level}"
        )));
    }
    let device_name: String = input
        .device_name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    if device_name.is_empty() {
        return Err(AppError::Message("本机设备名称不能为空".into()));
    }
    let normalized_subnets = crate::local_network::normalize_extra_subnets(&input.extra_subnets)
        .map_err(AppError::Message)?;

    let app_for_save = app.clone();
    let enabled = input.enabled;
    let allow_local_noauth = input.allow_local_noauth;
    let shared_workspace_ids = input.shared_workspace_ids;
    let allow_computer_use = input.allow_computer_use;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        let _ = lan_peer::ensure_local_identity().map_err(AppError::Message)?;
        app_for_save.state::<AppState>().with_settings(|store| {
            let mut settings = store.settings();
            if let Some(local_noauth) = allow_local_noauth {
                settings.allow_local_noauth = local_noauth;
            }
            settings.lan_federation.enabled = enabled;
            settings.lan_federation.device_name = device_name;
            settings.lan_federation.share_mode = share_mode;
            settings.lan_federation.shared_workspace_ids = shared_workspace_ids;
            settings.lan_federation.trust_level = trust_level;
            settings.lan_federation.allow_computer_use = allow_computer_use;
            settings.lan_federation.extra_subnets = normalized_subnets;
            store.update_settings(settings)
        })?;
        if !enabled {
            lan_peer::cancel_pairing_pin();
        }
        lan_peer::sync_discovery_daemon();
        lan_peer::invalidate_peer_cache("");
        Ok(())
    })
    .await
    .map_err(|err| AppError::Message(format!("保存局域网设置任务失败: {err}")))??;

    let listener_restarted = crate::commands::runtime::apply_live_runtime_changes(&state, false)
        .await
        .map_err(|error| {
            eprintln!("Global MCP live reload after LAN settings save failed: {error}");
            AppError::Message(format!(
                "局域网设置已保存，但热重载 Global MCP 失败，请手动重启服务：{error}"
            ))
        })?;

    let overview = build_lan_overview(&app, true).await?;
    Ok(LanSettingsSaveResultDto {
        overview,
        listener_restarted,
    })
}

#[tauri::command]
pub async fn create_lan_pairing_pin() -> AppResult<PairingPinDto> {
    tauri::async_runtime::spawn_blocking(|| {
        let _ = lan_peer::ensure_local_identity().map_err(AppError::Message)?;
        Ok(lan_peer::generate_pairing_pin())
    })
    .await
    .map_err(|err| AppError::Message(format!("生成配对码任务失败: {err}")))?
}

#[tauri::command]
pub async fn cancel_lan_pairing_pin() -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(|| {
        lan_peer::cancel_pairing_pin();
        Ok(())
    })
    .await
    .map_err(|err| AppError::Message(format!("取消配对码任务失败: {err}")))?
}

#[tauri::command]
pub async fn scan_lan_peers(app: AppHandle) -> AppResult<LanOverviewDto> {
    tauri::async_runtime::spawn_blocking(lan_peer::trigger_discovery_scan)
        .await
        .map_err(|err| AppError::Message(format!("触发局域网扫描任务失败: {err}")))?;
    lan_peer::probe_configured_overlay_targets().await;
    tokio::time::sleep(std::time::Duration::from_millis(220)).await;
    build_lan_overview(&app, true).await
}

#[tauri::command]
pub async fn probe_lan_peer_address(host: String, port: u16) -> AppResult<DiscoveredPeerDto> {
    lan_peer::probe_peer_hello(&host, port)
        .await
        .map_err(AppError::Message)
}

#[tauri::command]
pub async fn pair_lan_peer(
    app: AppHandle,
    state: State<'_, AppState>,
    host: String,
    port: u16,
    pin: String,
) -> AppResult<LanOverviewDto> {
    let was_bind_all = state.with_settings(|store| {
        let s = store.settings();
        Ok(s.allow_lan_access || s.lan_federation.enabled)
    })?;
    let _paired = lan_peer::pair_with_peer(&host, port, &pin)
        .await
        .map_err(AppError::Message)?;
    lan_peer::sync_discovery_daemon();

    let was_running = state.with_runtime(|rt| Ok(rt.is_running()))?;
    if !was_bind_all && was_running {
        if let Err(error) = restart_global_mcp(&state).await {
            eprintln!("Global MCP restart after LAN pairing failed: {error}");
            return Err(AppError::Message(format!(
                "已完成设备配对，但重启 Global MCP 以开放局域网监听失败，请手动重启服务：{error}"
            )));
        }
    }
    build_lan_overview(&app, true).await
}

#[tauri::command]
pub async fn remove_lan_paired_peer(
    app: AppHandle,
    device_id: String,
) -> AppResult<LanOverviewDto> {
    let (removed, secret) = tauri::async_runtime::spawn_blocking(move || -> AppResult<_> {
        let secret = lan_peer::get_peer_secret(&device_id, None);
        let removed = lan_peer::remove_local_paired_peer(&device_id).map_err(AppError::Message)?;
        Ok((removed, secret))
    })
    .await
    .map_err(|err| AppError::Message(format!("移除配对设备任务失败: {err}")))??;

    if let (Some(peer), Some(secret)) = (removed, secret) {
        lan_peer::notify_peer_unpair(&peer, &secret).await;
    }
    build_lan_overview(&app, false).await
}

#[tauri::command]
pub async fn set_lan_paired_peer_enabled(
    app: AppHandle,
    device_id: String,
    enabled: bool,
) -> AppResult<LanOverviewDto> {
    tauri::async_runtime::spawn_blocking(move || {
        lan_peer::update_paired_peer_enabled(&device_id, enabled).map_err(AppError::Message)
    })
    .await
    .map_err(|err| AppError::Message(format!("更新配对设备状态任务失败: {err}")))??;
    build_lan_overview(&app, true).await
}

#[tauri::command]
pub async fn rename_lan_paired_peer(
    app: AppHandle,
    device_id: String,
    device_name: String,
) -> AppResult<LanOverviewDto> {
    tauri::async_runtime::spawn_blocking(move || {
        lan_peer::rename_paired_peer(&device_id, &device_name).map_err(AppError::Message)
    })
    .await
    .map_err(|err| AppError::Message(format!("重命名配对设备任务失败: {err}")))??;
    build_lan_overview(&app, false).await
}
