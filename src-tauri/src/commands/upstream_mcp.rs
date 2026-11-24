use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::mcp::stdio_bridge::gemini_stdio_bin_path;
use crate::upstream_mcp::{
    apply_harness_takeovers, default_backup_dir, ensure_local_stdio_bridge_ready,
    list_takeover_backups_at, parse_mcp_servers_json, preview_harness_takeover,
    restore_takeover_backup_at, scan_local_mcp_clients, validate_and_normalize_server,
    HarnessTakeoverAppliedDto, HarnessTakeoverBackupRecord, HarnessTakeoverPlanItem,
    ImportSelectionItem, ImportedUpstreamCandidate, ResolvedConnectMode, SelfLoopContext,
    TakeoverConnectMode, UpstreamExposeMode, UpstreamMcpOverviewDto, UpstreamSecretPatch,
    UpstreamServerConfig, UpstreamTestSyncResultDto,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamImportResultDto {
    pub imported_count: usize,
    pub skipped: Vec<String>,
    pub replaced_files: Vec<HarnessTakeoverAppliedDto>,
    pub replace_errors: Vec<String>,
    pub overview: UpstreamMcpOverviewDto,
}

async fn load_overview_blocking(app: &AppHandle) -> AppResult<UpstreamMcpOverviewDto> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        Ok(state.gateway.upstream.overview(None))
    })
    .await
    .map_err(|err| AppError::Message(format!("加载 MCP 聚合概览失败: {err}")))?
}

#[tauri::command]
pub async fn get_upstream_mcp_overview(app: AppHandle) -> AppResult<UpstreamMcpOverviewDto> {
    load_overview_blocking(&app).await
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
    load_overview_blocking(&app).await
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
    load_overview_blocking(&app).await
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
    load_overview_blocking(&app).await
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
    load_overview_blocking(&app).await
}

#[tauri::command]
pub async fn set_upstream_mcp_server_mode(
    app: AppHandle,
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

    load_overview_blocking(&app).await
}

#[tauri::command]
pub async fn update_upstream_mcp_disabled_tools(
    app: AppHandle,
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

    load_overview_blocking(&app).await
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
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Vec<UpstreamTestSyncResultDto>> {
    let app_for_read = app.clone();
    let ids: Vec<String> = tauri::async_runtime::spawn_blocking(move || {
        app_for_read.state::<AppState>().with_settings(|store| {
            Ok(store
                .settings()
                .upstream_mcp
                .servers
                .into_iter()
                .filter(|s| s.enabled)
                .map(|s| s.id)
                .collect())
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("读取上游 MCP 列表失败: {err}")))??;
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

#[tauri::command]
pub async fn preview_upstream_mcp_takeover(
    app: AppHandle,
    selections: Vec<ImportSelectionItem>,
    connect_mode: Option<TakeoverConnectMode>,
) -> AppResult<Vec<HarnessTakeoverPlanItem>> {
    let app_handle = app.clone();
    let mode = connect_mode.unwrap_or_default();
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
        let scanned = scan_local_mcp_clients(&existing_aliases, &loop_ctx);
        let by_id: HashMap<String, ImportedUpstreamCandidate> = scanned
            .into_iter()
            .map(|item| (item.candidate_id.clone(), item))
            .collect();

        let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for sel in selections {
            if let Some(cand) = by_id.get(&sel.candidate_id) {
                if cand.blocked_reason.is_none() {
                    grouped
                        .entry(cand.source_client.clone())
                        .or_default()
                        .push(cand.original_key.clone());
                }
            }
        }
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Message("无法定位用户主目录以预览配置文件替换".into()))?;
        let stdio_bin = gemini_stdio_bin_path().map_err(AppError::Message)?;
        Ok(preview_harness_takeover(
            &home, &grouped, mode, &settings, &stdio_bin,
        ))
    })
    .await
    .map_err(|err| AppError::Message(format!("生成接管预览失败: {err}")))?
}

#[tauri::command]
pub async fn list_upstream_mcp_takeover_backups() -> AppResult<Vec<HarnessTakeoverBackupRecord>> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = default_backup_dir()?;
        Ok(list_takeover_backups_at(&dir))
    })
    .await
    .map_err(|err| AppError::Message(format!("读取接管备份记录失败: {err}")))?
}

#[tauri::command]
pub async fn restore_upstream_mcp_takeover_backup(
    backup_id: String,
) -> AppResult<HarnessTakeoverBackupRecord> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Message("无法定位用户主目录以执行还原".into()))?;
        let dir = default_backup_dir()?;
        restore_takeover_backup_at(&home, &dir, &backup_id).map_err(AppError::Message)
    })
    .await
    .map_err(|err| AppError::Message(format!("还原备份失败: {err}")))?
}

/// Confirm and persist selected candidates from either a local client scan (`raw_json == None`)
/// or a pasted JSON payload (`raw_json == Some(...)`). When `replace_source_configs` is true
/// (scan mode), atomically backs up each source harness config and replaces only the imported
/// entries with a single Coding Tools MCP entry.
#[tauri::command]
pub async fn import_upstream_mcp_candidates(
    app: AppHandle,
    state: State<'_, AppState>,
    raw_json: Option<String>,
    selections: Vec<ImportSelectionItem>,
    replace_source_configs: Option<bool>,
    connect_mode: Option<TakeoverConnectMode>,
) -> AppResult<UpstreamImportResultDto> {
    if selections.is_empty() {
        return Err(AppError::Message(
            "请至少勾选一个要导入的上游 MCP 服务".into(),
        ));
    }
    let do_replace = replace_source_configs.unwrap_or(false)
        && raw_json
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .is_none();
    let mode = connect_mode.unwrap_or_default();
    let app_for_save = app.clone();
    let (imported_count, skipped, replaced_files, replace_errors) =
        tauri::async_runtime::spawn_blocking(move || -> AppResult<_> {
            let (imported, skipped, imported_by_client, settings_after) =
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
                    let mut imported_by_client: BTreeMap<String, Vec<String>> = BTreeMap::new();

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
                        if let Err(err) = validate_and_normalize_server(
                            &mut cand.config,
                            &current_servers,
                            &loop_ctx,
                        ) {
                            skipped.push(format!("{}: {err}", cand.original_key));
                            continue;
                        }
                        let source_client = cand.source_client.clone();
                        let original_key = cand.original_key.clone();
                        let saved = store.save_upstream_server(cand.config, cand.secret_patch)?;
                        current_servers.push(saved);
                        imported_by_client
                            .entry(source_client)
                            .or_default()
                            .push(original_key);
                        imported += 1;
                    }
                    Ok((imported, skipped, imported_by_client, store.settings()))
                })?;

            let mut replaced_files = Vec::new();
            let mut replace_errors = Vec::new();
            if do_replace && !imported_by_client.is_empty() {
                let home = dirs::home_dir().ok_or_else(|| {
                    AppError::Message("无法定位用户主目录，跳过替换源客户端配置".into())
                })?;
                let backup_dir = default_backup_dir()?;
                let needs_stdio = imported_by_client.keys().any(|client| {
                    let (eff, _) = crate::upstream_mcp::takeover::resolve_client_connect_mode(
                        client,
                        mode,
                        &settings_after,
                    );
                    eff == ResolvedConnectMode::Stdio
                });
                let stdio_bin = if needs_stdio {
                    match ensure_local_stdio_bridge_ready(&settings_after) {
                        Ok(bin) => bin,
                        Err(err) => {
                            replace_errors.push(format!("准备 mcp-stdio 桥接入口失败: {err}"));
                            gemini_stdio_bin_path().map_err(AppError::Message)?
                        }
                    }
                } else {
                    gemini_stdio_bin_path().map_err(AppError::Message)?
                };
                if replace_errors.is_empty() {
                    let (applied, errs) = apply_harness_takeovers(
                        &home,
                        &backup_dir,
                        &imported_by_client,
                        mode,
                        &settings_after,
                        &stdio_bin,
                    );
                    replaced_files = applied;
                    replace_errors = errs;
                }
            }

            Ok((imported, skipped, replaced_files, replace_errors))
        })
        .await
        .map_err(|err| AppError::Message(format!("导入上游 MCP 失败: {err}")))??;

    state.gateway.upstream.reload_config(None).await;
    let overview = load_overview_blocking(&app).await?;
    Ok(UpstreamImportResultDto {
        imported_count,
        skipped,
        replaced_files,
        replace_errors,
        overview,
    })
}
