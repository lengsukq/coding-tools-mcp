use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::settings::{FrpProfile, ProxyConfig};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrpProfileDto {
    pub id: String,
    pub name: String,
    pub server: String,
    pub server_port: u16,
    pub has_token: bool,
}

#[tauri::command]
pub async fn list_frp_profiles(app: AppHandle) -> AppResult<Vec<FrpProfileDto>> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_settings(|store| {
            Ok(store
                .data()
                .frp_profiles
                .iter()
                .map(|profile| {
                    let has_token = store
                        .get_app_secret("frp_profile_token", &profile.id)
                        .is_some_and(|value| !value.trim().is_empty());

                    FrpProfileDto {
                        id: profile.id.clone(),
                        name: profile.name.clone(),
                        server: profile.server.clone(),
                        server_port: profile.server_port,
                        has_token,
                    }
                })
                .collect())
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("读取 FRP 配置失败: {err}")))?
}

#[tauri::command]
pub async fn save_frp_profile(
    app: AppHandle,
    profile: FrpProfile,
    token: Option<String>,
) -> AppResult<FrpProfileDto> {
    if profile.name.trim().is_empty() || profile.server.trim().is_empty() {
        return Err(AppError::Message("FRP 配置名称和服务器不能为空。".into()));
    }

    let mut saved = profile;
    saved.name = saved.name.trim().to_string();
    saved.server = saved.server.trim().to_string();

    if saved.id.trim().is_empty() {
        saved.id = uuid::Uuid::new_v4().to_string().replace('-', "");
    }

    tauri::async_runtime::spawn_blocking(move || {
        let has_token = app
            .state::<AppState>()
            .with_settings(|store| store.save_frp_profile(saved.clone(), token.as_deref()))?;
        Ok(FrpProfileDto {
            id: saved.id,
            name: saved.name,
            server: saved.server,
            server_port: saved.server_port,
            has_token,
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("保存 FRP 配置任务失败: {err}")))?
}

#[tauri::command]
pub async fn delete_frp_profile(app: AppHandle, id: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .with_settings(|store| store.delete_frp_profile(&id))
    })
    .await
    .map_err(|err| AppError::Message(format!("删除 FRP 配置任务失败: {err}")))?
}

#[tauri::command]
pub async fn get_proxy(app: AppHandle) -> AppResult<ProxyConfig> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .with_settings(|store| Ok(store.settings().proxy))
    })
    .await
    .map_err(|err| AppError::Message(format!("读取代理配置失败: {err}")))?
}

#[tauri::command]
pub async fn set_proxy(app: AppHandle, proxy: ProxyConfig) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_settings(|store| {
            let mut settings = store.settings();
            settings.proxy = proxy;
            store.update_settings(settings)
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("保存代理配置失败: {err}")))?
}

#[tauri::command]
pub async fn set_last_workspace(app: AppHandle, id: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_settings(|store| {
            if store.data().last_workspace_id == id {
                return Ok(());
            }
            let mut settings = store.settings();
            settings.last_workspace_id = id;
            store.update_settings(settings)
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("更新最近工作区失败: {err}")))?
}

#[tauri::command]
pub async fn get_last_workspace_id(app: AppHandle) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .with_settings(|store| Ok(store.data().last_workspace_id.clone()))
    })
    .await
    .map_err(|err| AppError::Message(format!("读取最近工作区失败: {err}")))?
}
