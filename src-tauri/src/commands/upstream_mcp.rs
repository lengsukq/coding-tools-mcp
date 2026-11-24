use std::collections::{HashMap, HashSet};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::upstream_mcp::{
    parse_mcp_servers_json, scan_local_mcp_clients, validate_and_normalize_server,
    ImportSelectionItem, ImportedUpstreamCandidate, SelfLoopContext, UpstreamExposeMode,
    UpstreamMcpOverviewDto, UpstreamSecretPatch, UpstreamServerConfig, UpstreamTestSyncResultDto,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamImportResultDto {
    pub imported_count: usize,
    pub skipped: Vec<String>,
    pub overview: UpstreamMcpOverviewDto,
}

#[tauri::command]
pub async fn get_upstream_mcp_overview(app: AppHandle) -> AppResult<UpstreamMcpOverviewDto> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        Ok(state.gateway.upstream.overview(None))
    })
    .await
    .map_err(|err| AppError::Message(format!("加载 MCP 聚合概览失败: {err}")))?
}

#[tauri::command]
pub async fn set_upstream_mcp_global_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
    always_expose_proxy_tools: bool,
) -> AppResult<UpstreamMcpOverviewDto> {
    let app_for_save = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app_for_save.state::<AppState>().with_settings(|store| {
            store.set_upstream_global_settings(enabled, always_expose_proxy_tools)
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("保存 MCP 聚合开关失败: {err}")))??;

    state.gateway.upstream.reload_config(None).await;
    Ok(state.gateway.upstream.overview(None))
}

#[tauri::command]
pub async fn save_upstream_mcp_server(
    app: AppHandle,
    state: State<'_, AppState>,
    server: UpstreamServerConfig,
    secret_patch: Option<UpstreamSecretPatch>,
) -> AppResult<UpstreamMcpOverviewDto> {
    let app_for_save = app.clone();
    let patch = secret_patch.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        app_for_save.state::<AppState>().with_settings(|store| {
            let settings = store.settings();
            let mut next = server;
            validate_and_normalize_server(
                &mut next,
                &settings.upstream_mcp.servers,
                &SelfLoopContext::from_settings(&settings),
            )
            .map_err(AppError::Message)?;
            store.save_upstream_server(next, patch)?;
            Ok(())
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("保存上游 MCP 配置失败: {err}")))??;

    state.gateway.upstream.reload_config(None).await;
    Ok(state.gateway.upstream.overview(None))
}

#[tauri::command]
pub async fn delete_upstream_mcp_server(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
) -> AppResult<UpstreamMcpOverviewDto> {
    let id_for_save = server_id.clone();
    let app_for_save = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app_for_save
            .state::<AppState>()
            .with_settings(|store| store.delete_upstream_server(&id_for_save))
    })
    .await
    .map_err(|err| AppError::Message(format!("删除上游 MCP 配置失败: {err}")))??;

    state
        .gateway
        .upstream
        .disconnect_server(&server_id, true)
        .await;
    state.gateway.upstream.reload_config(None).await;
    Ok(state.gateway.upstream.overview(None))
}

#[tauri::command]
pub async fn set_upstream_mcp_server_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    enabled: bool,
) -> AppResult<UpstreamMcpOverviewDto> {
    let app_for_save = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app_for_save.state::<AppState>().with_settings(|store| {
            let settings = store.settings();
            let mut target = settings
                .upstream_mcp
                .servers
                .into_iter()
                .find(|s| s.id == server_id)
                .ok_or_else(|| AppError::Message(format!("未找到上游 MCP 服务 `{server_id}`")))?;
            target.enabled = enabled;
            store.save_upstream_server(target, UpstreamSecretPatch::default())?;
            Ok(())
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("切换上游 MCP 状态失败: {err}")))??;

    state.gateway.upstream.reload_config(None).await;
    Ok(state.gateway.upstream.overview(None))
}

#[tauri::command]
pub async fn set_upstream_mcp_server_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    expose_mode: UpstreamExposeMode,
) -> AppResult<UpstreamMcpOverviewDto> {
    let app_for_save = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app_for_save.state::<AppState>().with_settings(|store| {
            let settings = store.settings();
            let mut target = settings
                .upstream_mcp
                .servers
                .into_iter()
                .find(|s| s.id == server_id)
                .ok_or_else(|| AppError::Message(format!("未找到上游 MCP 服务 `{server_id}`")))?;
            target.expose_mode = expose_mode;
            store.save_upstream_server(target, UpstreamSecretPatch::default())?;
            Ok(())
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("切换暴露模式失败: {err}")))??;

    Ok(state.gateway.upstream.overview(None))
}

#[tauri::command]
pub async fn update_upstream_mcp_disabled_tools(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    disabled_tools: Vec<String>,
) -> AppResult<UpstreamMcpOverviewDto> {
    let app_for_save = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app_for_save.state::<AppState>().with_settings(|store| {
            let settings = store.settings();
            let mut target = settings
                .upstream_mcp
                .servers
                .into_iter()
                .find(|s| s.id == server_id)
                .ok_or_else(|| AppError::Message(format!("未找到上游 MCP 服务 `{server_id}`")))?;
            let mut deduped = Vec::new();
            for tool in disabled_tools {
                let clean = tool.trim().to_string();
                if !clean.is_empty() && !deduped.contains(&clean) {
                    deduped.push(clean);
                }
            }
            target.disabled_tools = deduped;
            store.save_upstream_server(target, UpstreamSecretPatch::default())?;
            Ok(())
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("保存工具过滤设置失败: {err}")))??;

    Ok(state.gateway.upstream.overview(None))
}

#[tauri::command]
pub async fn test_and_sync_upstream_mcp_server(
    state: State<'_, AppState>,
    server_id: String,
) -> AppResult<UpstreamTestSyncResultDto> {
    Ok(state
        .gateway
        .upstream
        .test_and_sync_server(&server_id, None)
        .await)
}

#[tauri::command]
pub async fn sync_all_upstream_mcp_servers(
    state: State<'_, AppState>,
) -> AppResult<Vec<UpstreamTestSyncResultDto>> {
    let ids: Vec<String> = state.with_settings(|store| {
        Ok(store
            .settings()
            .upstream_mcp
            .servers
            .into_iter()
            .filter(|s| s.enabled)
            .map(|s| s.id)
            .collect())
    })?;
    let mut join_set = tokio::task::JoinSet::new();
    for id in ids {
        let hub = std::sync::Arc::clone(&state.gateway.upstream);
        join_set.spawn(async move { hub.test_and_sync_server(&id, None).await });
    }
    let mut results = Vec::new();
    while let Some(res) = join_set.join_next().await {
        if let Ok(item) = res {
            results.push(item);
        }
    }
    results.sort_by(|a, b| a.alias.cmp(&b.alias));
    Ok(results)
}

#[tauri::command]
pub async fn scan_local_upstream_mcp_clients(
    app: AppHandle,
) -> AppResult<Vec<ImportedUpstreamCandidate>> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = app_handle
            .state::<AppState>()
            .with_settings(|store| Ok(store.settings()))?;
        let existing_aliases: HashSet<String> = settings
            .upstream_mcp
            .servers
            .iter()
            .map(|s| s.alias.to_ascii_lowercase())
            .collect();
        let loop_ctx = SelfLoopContext::from_settings(&settings);
        Ok(scan_local_mcp_clients(&existing_aliases, &loop_ctx))
    })
    .await
    .map_err(|err| AppError::Message(format!("扫描本机 MCP 配置失败: {err}")))?
}

#[tauri::command]
pub async fn parse_upstream_mcp_json(
    app: AppHandle,
    raw_json: String,
) -> AppResult<Vec<ImportedUpstreamCandidate>> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = app_handle
            .state::<AppState>()
            .with_settings(|store| Ok(store.settings()))?;
        let existing_aliases: HashSet<String> = settings
            .upstream_mcp
            .servers
            .iter()
            .map(|s| s.alias.to_ascii_lowercase())
            .collect();
        let loop_ctx = SelfLoopContext::from_settings(&settings);
        parse_mcp_servers_json(
            &raw_json,
            "json",
            "粘贴 JSON",
            "",
            &existing_aliases,
            &loop_ctx,
        )
        .map_err(AppError::Message)
    })
    .await
    .map_err(|err| AppError::Message(format!("解析 JSON 配置失败: {err}")))?
}

/// Confirm and persist selected candidates from either a local client scan (`raw_json == None`)
/// or a pasted JSON payload (`raw_json == Some(...)`). Secrets are re-extracted in Rust and
/// written directly to `app_secrets` without ever transiting raw values through WebView state.
#[tauri::command]
pub async fn import_upstream_mcp_candidates(
    app: AppHandle,
    state: State<'_, AppState>,
    raw_json: Option<String>,
    selections: Vec<ImportSelectionItem>,
) -> AppResult<UpstreamImportResultDto> {
    if selections.is_empty() {
        return Err(AppError::Message(
            "请至少勾选一个要导入的上游 MCP 服务".into(),
        ));
    }
    let app_for_save = app.clone();
    let (imported_count, skipped) =
        tauri::async_runtime::spawn_blocking(move || -> AppResult<(usize, Vec<String>)> {
            app_for_save.state::<AppState>().with_settings(|store| {
                let settings = store.settings();
                let existing_aliases: HashSet<String> = settings
                    .upstream_mcp
                    .servers
                    .iter()
                    .map(|s| s.alias.to_ascii_lowercase())
                    .collect();
                let loop_ctx = SelfLoopContext::from_settings(&settings);
                let all_candidates =
                    match raw_json.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                        Some(raw) => parse_mcp_servers_json(
                            raw,
                            "json",
                            "粘贴 JSON",
                            "",
                            &existing_aliases,
                            &loop_ctx,
                        )
                        .map_err(AppError::Message)?,
                        None => scan_local_mcp_clients(&existing_aliases, &loop_ctx),
                    };

                let mut by_id: HashMap<String, ImportedUpstreamCandidate> = all_candidates
                    .into_iter()
                    .map(|item| (item.candidate_id.clone(), item))
                    .collect();
                let mut current_servers = settings.upstream_mcp.servers.clone();
                let mut imported = 0usize;
                let mut skipped = Vec::new();

                for sel in selections {
                    let Some(mut cand) = by_id.remove(&sel.candidate_id) else {
                        skipped.push(format!("{}: 候选条目已失效", sel.candidate_id));
                        continue;
                    };
                    if let Some(reason) = cand.blocked_reason {
                        skipped.push(format!("{}: {reason}", cand.original_key));
                        continue;
                    }
                    if let Some(alias) = sel.alias_override.filter(|v| !v.trim().is_empty()) {
                        cand.config.alias = alias;
                    }
                    if let Some(mode) = sel.expose_mode {
                        cand.config.expose_mode = mode;
                    }
                    if let Some(enabled) = sel.enabled {
                        cand.config.enabled = enabled;
                    }
                    if let Err(err) =
                        validate_and_normalize_server(&mut cand.config, &current_servers, &loop_ctx)
                    {
                        skipped.push(format!("{}: {err}", cand.original_key));
                        continue;
                    }
                    let saved = store.save_upstream_server(cand.config, cand.secret_patch)?;
                    current_servers.push(saved);
                    imported += 1;
                }
                Ok((imported, skipped))
            })
        })
        .await
        .map_err(|err| AppError::Message(format!("导入上游 MCP 失败: {err}")))??;

    state.gateway.upstream.reload_config(None).await;
    let overview = state.gateway.upstream.overview(None);
    Ok(UpstreamImportResultDto {
        imported_count,
        skipped,
        overview,
    })
}
