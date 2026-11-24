use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::{external_base_url, issue_scoped_access_token};
use crate::error::{AppError, AppResult};
use crate::mcp::stdio_bridge::{
    gemini_stdio_auth_file_path, gemini_stdio_bin_path, remove_auth_file_if_exists,
    write_private_auth_file, GeminiStdioAuthFile, GEMINI_STDIO_CLIENT_ID,
};
use crate::platform::platform;
use crate::secret::SecretStore;
use crate::settings::AppSettings;

use super::import::{display_home_path, local_json_client_sources, strip_toml_comment};
use super::model::{unix_time_ms, SelfLoopContext};

pub const DEFAULT_INJECTED_SERVER_KEY: &str = "coding-tools";
const MAX_KEPT_BACKUPS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TakeoverConnectMode {
    #[default]
    Auto,
    Http,
    Stdio,
}

impl TakeoverConnectMode {
    pub fn parse_str(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Ok(Self::Auto),
            "http" => Ok(Self::Http),
            "stdio" => Ok(Self::Stdio),
            other => Err(format!(
                "不支持的接管接入模式 `{other}`（可选 auto / http / stdio）"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedConnectMode {
    Http,
    Stdio,
}

impl ResolvedConnectMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Stdio => "stdio",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessTakeoverPlanItem {
    pub source_client: String,
    pub source_label: String,
    pub display_path: String,
    pub effective_mode: ResolvedConnectMode,
    pub removed_keys: Vec<String>,
    pub injected_key: String,
    pub already_has_coding_tools: bool,
    pub entry_preview: String,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessTakeoverBackupRecord {
    pub id: String,
    pub created_at_ms: u64,
    pub source_client: String,
    pub source_label: String,
    pub target_path: String,
    pub display_path: String,
    pub backup_file: String,
    pub removed_keys: Vec<String>,
    pub injected_key: String,
    pub connect_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessTakeoverAppliedDto {
    pub backup_id: String,
    pub source_client: String,
    pub source_label: String,
    pub display_path: String,
    pub effective_mode: ResolvedConnectMode,
    pub removed_keys: Vec<String>,
    pub injected_key: String,
}

/// Resolve the authoritative config path for a supported harness client.
pub fn resolve_client_config_path(
    home: &Path,
    source_client: &str,
) -> Option<(&'static str, PathBuf)> {
    if source_client == "codex" {
        return Some(("Codex", home.join(".codex").join("config.toml")));
    }
    local_json_client_sources(home)
        .into_iter()
        .find(|(id, _, _)| *id == source_client)
        .map(|(_, label, path)| (label, path))
}

/// Determine the effective connector mode (HTTP vs Stdio) for a given harness client,
/// along with any auth preflight warning if the requested mode cannot work safely.
pub fn resolve_client_connect_mode(
    source_client: &str,
    requested: TakeoverConnectMode,
    settings: &AppSettings,
) -> (ResolvedConnectMode, Option<String>) {
    let local_http_ok =
        settings.allow_local_noauth || settings.global_mcp_auth_type.trim() == "noauth";
    let local_stdio_ok =
        settings.global_mcp_auth_type.trim() != "bearer" || settings.allow_local_noauth;
    let client_prefers_http = matches!(
        source_client,
        "codex" | "cursor" | "claude_code" | "opencode"
    );

    match requested {
        TakeoverConnectMode::Auto => {
            if client_prefers_http && local_http_ok {
                (ResolvedConnectMode::Http, None)
            } else if local_stdio_ok {
                (ResolvedConnectMode::Stdio, None)
            } else {
                (
                    ResolvedConnectMode::Stdio,
                    Some(
                        "当前 Global MCP 为 Bearer 认证且关闭了「本机免授权直连」；建议开启「本机免授权直连」或改用 OAuth，以确保接管后客户端免明文密钥直连。"
                            .into(),
                    ),
                )
            }
        }
        TakeoverConnectMode::Http => {
            let warn = if !local_http_ok {
                Some(
                    "本机 HTTP 直连需要开启「服务名与访问控制 → 本机免授权直连」（或设为无认证模式），否则客户端连接 http://127.0.0.1:<port>/mcp 会返回 401。"
                        .into(),
                )
            } else if source_client == "claude_desktop" {
                Some(
                    "Claude Desktop 本地配置文件通常仅支持 Stdio 子进程，建议改用自动或 mcp-stdio 模式。"
                        .into(),
                )
            } else {
                None
            };
            (ResolvedConnectMode::Http, warn)
        }
        TakeoverConnectMode::Stdio => {
            let warn = if !local_stdio_ok {
                Some(
                    "Bearer 模式且未开启「本机免授权直连」时，mcp-stdio 不会自动把 Bearer Token 写入外部配置；请开启本机免授权或改用 OAuth。"
                        .into(),
                )
            } else {
                None
            };
            (ResolvedConnectMode::Stdio, warn)
        }
    }
}

/// Preview what files and entries will be rewritten if the selected scanned candidates
/// are aggregated and replaced. Pure read-only inspection.
pub fn preview_harness_takeover(
    home: &Path,
    grouped_keys: &BTreeMap<String, Vec<String>>,
    requested_mode: TakeoverConnectMode,
    settings: &AppSettings,
    stdio_bin: &Path,
) -> Vec<HarnessTakeoverPlanItem> {
    let endpoint = format!(
        "http://127.0.0.1:{}/mcp",
        settings.global_gateway.local_port
    );
    let self_loop = SelfLoopContext::from_settings(settings);
    let mut plans = Vec::new();

    for (client_id, keys) in grouped_keys {
        if keys.is_empty() {
            continue;
        }
        let Some((label, path)) = resolve_client_config_path(home, client_id) else {
            continue;
        };
        let (effective_mode, warning) =
            resolve_client_connect_mode(client_id, requested_mode, settings);
        let raw = fs::read_to_string(&path).unwrap_or_default();
        let existing_self_key = find_existing_self_key(client_id, &raw, &self_loop);
        let already_has_coding_tools = existing_self_key.is_some();
        let injected_key =
            existing_self_key.unwrap_or_else(|| DEFAULT_INJECTED_SERVER_KEY.to_string());
        let entry_preview = format_entry_preview(
            client_id,
            &injected_key,
            effective_mode,
            &endpoint,
            stdio_bin,
        );
        plans.push(HarnessTakeoverPlanItem {
            source_client: client_id.clone(),
            source_label: label.to_string(),
            display_path: display_home_path(home, &path),
            effective_mode,
            removed_keys: keys.clone(),
            injected_key,
            already_has_coding_tools,
            entry_preview,
            warning,
        });
    }

    plans
}

/// Ensure the local `mcp-stdio` hardlink entrypoint and (for OAuth) private `0600` credential
/// file are prepared before injecting stdio commands into external harness configs.
pub fn ensure_local_stdio_bridge_ready(settings: &AppSettings) -> Result<PathBuf, String> {
    let bin_path = gemini_stdio_bin_path()?;
    let auth_path = gemini_stdio_auth_file_path()?;
    let current_exe = resolve_cli_executable().map_err(|e| e.to_string())?;

    install_bridge_entrypoint_shared(&bin_path, &current_exe).map_err(|e| e.to_string())?;

    let endpoint = format!(
        "http://127.0.0.1:{}/mcp",
        settings.global_gateway.local_port
    );
    match settings.global_mcp_auth_type.trim() {
        "oauth" => {
            let oauth_secret = SecretStore::get_shared("oauth_token_secret")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            let server_url = external_base_url(
                &HeaderMap::new(),
                settings.global_gateway.local_port,
                &settings.global_gateway.public_url,
            );
            let (token, expires_at) =
                issue_scoped_access_token(&server_url, &oauth_secret, GEMINI_STDIO_CLIENT_ID)
                    .map_err(|()| "生成本地 OAuth 桥接凭据失败".to_string())?;
            let payload = GeminiStdioAuthFile::new_oauth(endpoint, server_url, token, expires_at);
            write_private_auth_file(&auth_path, &payload)?;
        }
        "noauth" => {
            remove_auth_file_if_exists(&auth_path)?;
        }
        "bearer" if settings.allow_local_noauth => {
            remove_auth_file_if_exists(&auth_path)?;
        }
        "bearer" => {
            return Err(
                "当前为 Bearer 模式且关闭了本机免授权直连，无法生成无明文密钥的 mcp-stdio 桥接配置"
                    .into(),
            );
        }
        _ => {}
    }

    Ok(bin_path)
}

fn resolve_cli_executable() -> AppResult<PathBuf> {
    let current = std::env::current_exe()?;
    // Inside macOS `.app/Contents/MacOS/`, prefer sibling `coding-tools-mcp` if present,
    // otherwise fall back to current_exe (which also dispatches `mcp-stdio` subcommand).
    if let Some(parent) = current.parent() {
        let sibling = parent.join(if cfg!(windows) {
            "coding-tools-mcp.exe"
        } else {
            "coding-tools-mcp"
        });
        if sibling.is_file() {
            return Ok(sibling);
        }
    }
    Ok(current)
}

fn install_bridge_entrypoint_shared(bin_path: &Path, target_exe: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        const EXDEV: i32 = 18;
        let parent = bin_path
            .parent()
            .ok_or_else(|| AppError::Message("无效的本地桥接入口路径。".into()))?;
        fs::create_dir_all(parent)?;
        if let Ok(meta) = fs::symlink_metadata(bin_path) {
            if meta.is_dir() {
                return Err(AppError::Message(format!(
                    "{} 是目录，无法覆盖为桥接入口。",
                    bin_path.display()
                )));
            }
            // Skip replacing if already pointing to the same inode.
            use std::os::unix::fs::MetadataExt;
            if !meta.file_type().is_symlink() {
                if let Ok(target_meta) = fs::metadata(target_exe) {
                    if meta.dev() == target_meta.dev() && meta.ino() == target_meta.ino() {
                        return Ok(());
                    }
                }
            }
            fs::remove_file(bin_path)?;
        }
        if let Err(err) = fs::hard_link(target_exe, bin_path) {
            if err.raw_os_error() == Some(EXDEV) {
                fs::copy(target_exe, bin_path)?;
            } else {
                return Err(err.into());
            }
        }
    }
    #[cfg(not(unix))]
    {
        let same_path = bin_path == target_exe
            || (bin_path.exists()
                && fs::canonicalize(bin_path).ok() == fs::canonicalize(target_exe).ok());
        if !same_path {
            if let Some(parent) = bin_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(target_exe, bin_path)?;
        }
    }
    Ok(())
}

/// Apply takeover (backup + lossless rewrite) to all target harness files for imported keys.
pub fn apply_harness_takeovers(
    home: &Path,
    backup_dir: &Path,
    grouped_keys: &BTreeMap<String, Vec<String>>,
    requested_mode: TakeoverConnectMode,
    settings: &AppSettings,
    stdio_bin: &Path,
) -> (Vec<HarnessTakeoverAppliedDto>, Vec<String>) {
    let endpoint = format!(
        "http://127.0.0.1:{}/mcp",
        settings.global_gateway.local_port
    );
    let self_loop = SelfLoopContext::from_settings(settings);
    let mut applied = Vec::new();
    let mut errors = Vec::new();

    for (client_id, keys) in grouped_keys {
        if keys.is_empty() {
            continue;
        }
        let Some((label, target_path)) = resolve_client_config_path(home, client_id) else {
            errors.push(format!("{client_id}: 不支持自动替换的客户端类型"));
            continue;
        };
        if !target_path.is_file() {
            errors.push(format!(
                "{label}: 配置文件不存在（{}）",
                target_path.display()
            ));
            continue;
        }
        let (effective_mode, warning) =
            resolve_client_connect_mode(client_id, requested_mode, settings);
        if effective_mode == ResolvedConnectMode::Http && warning.is_some() {
            errors.push(format!("{label}: {}", warning.unwrap_or_default()));
            continue;
        }

        let original_bytes = match fs::read(&target_path) {
            Ok(b) => b,
            Err(err) => {
                errors.push(format!("{label}: 读取配置文件失败: {err}"));
                continue;
            }
        };
        let original_text = match String::from_utf8(original_bytes.clone()) {
            Ok(s) => s,
            Err(err) => {
                errors.push(format!("{label}: 配置文件不是有效 UTF-8: {err}"));
                continue;
            }
        };

        let existing_self_key = find_existing_self_key(client_id, &original_text, &self_loop);
        let injected_key = existing_self_key
            .clone()
            .unwrap_or_else(|| DEFAULT_INJECTED_SERVER_KEY.to_string());

        let rewritten = if client_id == "codex" {
            rewrite_codex_toml(
                &original_text,
                keys,
                &injected_key,
                existing_self_key.is_some(),
                effective_mode,
                &endpoint,
                stdio_bin,
            )
        } else {
            rewrite_json_client_config(
                client_id,
                &original_text,
                keys,
                &injected_key,
                existing_self_key.is_some(),
                effective_mode,
                &endpoint,
                stdio_bin,
            )
        };

        let next_content = match rewritten {
            Ok(c) => c,
            Err(err) => {
                errors.push(format!("{label}: 改写配置失败: {err}"));
                continue;
            }
        };

        let record = match create_backup_snapshot(
            backup_dir,
            home,
            client_id,
            label,
            &target_path,
            &original_bytes,
            keys,
            &injected_key,
            effective_mode,
        ) {
            Ok(rec) => rec,
            Err(err) => {
                errors.push(format!("{label}: 创建备份快照失败（未修改原文件）: {err}"));
                continue;
            }
        };

        if let Err(err) = atomic_write_bytes(&target_path, next_content.as_bytes()) {
            errors.push(format!(
                "{label}: 写入新配置失败（备份已保存为 {}）: {err}",
                record.id
            ));
            continue;
        }

        applied.push(HarnessTakeoverAppliedDto {
            backup_id: record.id,
            source_client: client_id.clone(),
            source_label: label.to_string(),
            display_path: record.display_path,
            effective_mode,
            removed_keys: keys.clone(),
            injected_key,
        });
    }

    (applied, errors)
}

pub fn default_backup_dir() -> AppResult<PathBuf> {
    Ok(platform()
        .app_config_dir()?
        .join("data")
        .join("harness-backups"))
}

pub fn list_takeover_backups_at(backup_dir: &Path) -> Vec<HarnessTakeoverBackupRecord> {
    let index_path = backup_dir.join("index.json");
    let Ok(raw) = fs::read_to_string(&index_path) else {
        return Vec::new();
    };
    let mut list: Vec<HarnessTakeoverBackupRecord> = serde_json::from_str(&raw).unwrap_or_default();
    list.sort_by_key(|b| std::cmp::Reverse(b.created_at_ms));
    list
}

/// Restore the exact original bytes from a saved takeover backup back to the harness config file.
pub fn restore_takeover_backup_at(
    home: &Path,
    backup_dir: &Path,
    backup_id: &str,
) -> Result<HarnessTakeoverBackupRecord, String> {
    let mut records = list_takeover_backups_at(backup_dir);
    let Some(pos) = records.iter().position(|r| r.id == backup_id) else {
        return Err(format!("未找到备份记录 `{backup_id}`"));
    };
    let record = records[pos].clone();

    // Verify target path matches an expected supported harness config path for safety.
    let expected_path = resolve_client_config_path(home, &record.source_client)
        .map(|(_, p)| p)
        .ok_or_else(|| format!("不支持还原的客户端类型 `{}`", record.source_client))?;
    let target_path = PathBuf::from(&record.target_path);
    if target_path != expected_path {
        return Err(format!(
            "备份目标路径与预期客户端路径不匹配，已拒绝还原（{}）",
            target_path.display()
        ));
    }

    let backup_path = backup_dir.join(&record.backup_file);
    let bytes = fs::read(&backup_path)
        .map_err(|err| format!("读取备份文件失败（{}）: {err}", backup_path.display()))?;
    atomic_write_bytes(&target_path, &bytes)
        .map_err(|err| format!("写回原配置文件失败（{}）: {err}", target_path.display()))?;

    records[pos].restored_at_ms = Some(unix_time_ms());
    save_backup_index(backup_dir, &records).map_err(|e| e.to_string())?;
    Ok(records[pos].clone())
}

#[allow(clippy::too_many_arguments)]
fn create_backup_snapshot(
    backup_dir: &Path,
    home: &Path,
    source_client: &str,
    source_label: &str,
    target_path: &Path,
    original_bytes: &[u8],
    removed_keys: &[String],
    injected_key: &str,
    effective_mode: ResolvedConnectMode,
) -> AppResult<HarnessTakeoverBackupRecord> {
    fs::create_dir_all(backup_dir)?;
    let now_ms = unix_time_ms();
    let short_id = &uuid::Uuid::new_v4().to_string()[..8];
    let id = format!("bk-{now_ms}-{short_id}");
    let backup_file = format!("{source_client}-{id}.bak");
    let backup_path = backup_dir.join(&backup_file);
    fs::write(&backup_path, original_bytes)?;

    let record = HarnessTakeoverBackupRecord {
        id,
        created_at_ms: now_ms,
        source_client: source_client.to_string(),
        source_label: source_label.to_string(),
        target_path: target_path.to_string_lossy().into_owned(),
        display_path: display_home_path(home, target_path),
        backup_file,
        removed_keys: removed_keys.to_vec(),
        injected_key: injected_key.to_string(),
        connect_mode: effective_mode.as_str().to_string(),
        restored_at_ms: None,
    };

    let mut index = list_takeover_backups_at(backup_dir);
    index.insert(0, record.clone());
    if index.len() > MAX_KEPT_BACKUPS {
        for old in index.drain(MAX_KEPT_BACKUPS..) {
            let _ = fs::remove_file(backup_dir.join(old.backup_file));
        }
    }
    save_backup_index(backup_dir, &index)?;
    Ok(record)
}

fn save_backup_index(backup_dir: &Path, records: &[HarnessTakeoverBackupRecord]) -> AppResult<()> {
    fs::create_dir_all(backup_dir)?;
    let bytes = serde_json::to_vec_pretty(records)
        .map_err(|err| AppError::Message(format!("序列化备份索引失败: {err}")))?;
    atomic_write_bytes(&backup_dir.join("index.json"), &bytes)
}

fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_file_name(format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config"),
        uuid::Uuid::new_v4()
    ));
    fs::write(&tmp, bytes)?;
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(AppError::Io(err));
    }
    Ok(())
}

fn json_container_key(client_id: &str) -> &'static str {
    if client_id == "opencode" {
        "mcp"
    } else {
        "mcpServers"
    }
}

fn build_json_entry(
    client_id: &str,
    mode: ResolvedConnectMode,
    endpoint: &str,
    stdio_bin: &Path,
) -> Value {
    let bin = stdio_bin.to_string_lossy().to_string();
    match (client_id, mode) {
        ("opencode", ResolvedConnectMode::Http) => json!({
            "type": "remote",
            "url": endpoint,
            "enabled": true
        }),
        ("opencode", ResolvedConnectMode::Stdio) => json!({
            "type": "local",
            "command": [bin, "mcp-stdio", endpoint],
            "enabled": true
        }),
        ("claude_code", ResolvedConnectMode::Http) => json!({
            "type": "http",
            "url": endpoint
        }),
        ("claude_code", ResolvedConnectMode::Stdio) => json!({
            "type": "stdio",
            "command": bin,
            "args": ["mcp-stdio", endpoint]
        }),
        ("antigravity", ResolvedConnectMode::Http) => json!({
            "serverUrl": endpoint
        }),
        ("gemini_cli", ResolvedConnectMode::Http) => json!({
            "httpUrl": endpoint
        }),
        (_, ResolvedConnectMode::Http) => json!({
            "url": endpoint
        }),
        (_, ResolvedConnectMode::Stdio) => json!({
            "command": bin,
            "args": ["mcp-stdio", endpoint]
        }),
    }
}

fn format_codex_entry_toml(
    injected_key: &str,
    mode: ResolvedConnectMode,
    endpoint: &str,
    stdio_bin: &Path,
) -> String {
    let header_key = if injected_key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        injected_key.to_string()
    } else {
        serde_json::to_string(injected_key).unwrap_or_else(|_| "\"coding-tools\"".into())
    };
    match mode {
        ResolvedConnectMode::Http => {
            let url_lit =
                serde_json::to_string(endpoint).unwrap_or_else(|_| format!("\"{endpoint}\""));
            format!("[mcp_servers.{header_key}]\nurl = {url_lit}\n")
        }
        ResolvedConnectMode::Stdio => {
            let bin = stdio_bin.to_string_lossy();
            let cmd_lit =
                serde_json::to_string(bin.as_ref()).unwrap_or_else(|_| format!("\"{bin}\""));
            let ep_lit =
                serde_json::to_string(endpoint).unwrap_or_else(|_| format!("\"{endpoint}\""));
            format!(
                "[mcp_servers.{header_key}]\ncommand = {cmd_lit}\nargs = [\"mcp-stdio\", {ep_lit}]\n"
            )
        }
    }
}

fn format_entry_preview(
    client_id: &str,
    injected_key: &str,
    mode: ResolvedConnectMode,
    endpoint: &str,
    stdio_bin: &Path,
) -> String {
    if client_id == "codex" {
        return format_codex_entry_toml(injected_key, mode, endpoint, stdio_bin)
            .trim_end()
            .to_string();
    }
    let entry = build_json_entry(client_id, mode, endpoint, stdio_bin);
    let container = json_container_key(client_id);
    serde_json::to_string_pretty(&json!({
        container: {
            injected_key: entry
        }
    }))
    .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn rewrite_json_client_config(
    client_id: &str,
    raw: &str,
    removed_keys: &[String],
    injected_key: &str,
    already_has_self: bool,
    mode: ResolvedConnectMode,
    endpoint: &str,
    stdio_bin: &Path,
) -> Result<String, String> {
    let mut root: Value =
        serde_json::from_str(raw.trim()).map_err(|err| format!("原 JSON 解析失败: {err}"))?;
    let Some(root_obj) = root.as_object_mut() else {
        return Err("原配置文件顶层不是 JSON 对象".into());
    };
    let container_key = json_container_key(client_id);
    let container = root_obj
        .entry(container_key.to_string())
        .or_insert_with(|| json!({}));
    let Some(servers_map) = container.as_object_mut() else {
        return Err(format!("原配置文件的 `{container_key}` 字段不是 JSON 对象"));
    };

    for key in removed_keys {
        if key != injected_key {
            servers_map.remove(key);
        }
    }
    if !already_has_self {
        servers_map.insert(
            injected_key.to_string(),
            build_json_entry(client_id, mode, endpoint, stdio_bin),
        );
    }

    let mut out = serde_json::to_string_pretty(&root)
        .map_err(|err| format!("序列化更新后 JSON 失败: {err}"))?;
    out.push('\n');
    Ok(out)
}

/// Losslessly remove `[mcp_servers.<key>]` and `[mcp_servers.<key>.*]` sections from
/// Codex `config.toml` while preserving all comments, formatting, and unrelated settings,
/// then append `[mcp_servers.coding-tools]` if not already present.
fn rewrite_codex_toml(
    raw: &str,
    removed_keys: &[String],
    injected_key: &str,
    already_has_self: bool,
    mode: ResolvedConnectMode,
    endpoint: &str,
    stdio_bin: &Path,
) -> Result<String, String> {
    let remove_set: std::collections::HashSet<&str> = removed_keys
        .iter()
        .map(String::as_str)
        .filter(|k| *k != injected_key)
        .collect();

    let mut kept_lines: Vec<&str> = Vec::new();
    let mut skipping_section = false;

    for line in raw.lines() {
        let cleaned = strip_toml_comment(line).trim();
        if cleaned.starts_with('[') && cleaned.ends_with(']') {
            let header = cleaned[1..cleaned.len() - 1].trim();
            if let Some(server_name) = parse_codex_mcp_section_server(header) {
                skipping_section = remove_set.contains(server_name.as_str());
                if skipping_section {
                    // Trim one redundant blank line directly preceding a removed section.
                    while kept_lines.last().is_some_and(|l| l.trim().is_empty()) {
                        kept_lines.pop();
                    }
                    continue;
                }
            } else {
                skipping_section = false;
            }
        }
        if !skipping_section {
            kept_lines.push(line);
        }
    }

    while kept_lines.last().is_some_and(|l| l.trim().is_empty()) {
        kept_lines.pop();
    }

    let mut output = kept_lines.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }

    if !already_has_self {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("# Managed by Coding Tools MCP Hub (aggregate & replace)\n");
        output.push_str(&format_codex_entry_toml(
            injected_key,
            mode,
            endpoint,
            stdio_bin,
        ));
    }

    Ok(output)
}

fn parse_codex_mcp_section_server(header: &str) -> Option<String> {
    let rest = header.strip_prefix("mcp_servers.")?;
    let first = rest.split('.').next()?.trim().trim_matches(['"', '\'']);
    if first.is_empty() {
        None
    } else {
        Some(first.to_string())
    }
}

fn find_existing_self_key(
    client_id: &str,
    raw: &str,
    self_loop: &SelfLoopContext,
) -> Option<String> {
    if raw.trim().is_empty() {
        return None;
    }
    if client_id == "codex" {
        let mut active: Option<String> = None;
        let mut by_server: HashMap<String, Vec<String>> = HashMap::new();
        for line in raw.lines() {
            let clean = strip_toml_comment(line).trim();
            if clean.starts_with('[') && clean.ends_with(']') {
                let header = clean[1..clean.len() - 1].trim();
                active = parse_codex_mcp_section_server(header);
                continue;
            }
            if let Some(srv) = active.as_deref() {
                by_server
                    .entry(srv.to_string())
                    .or_default()
                    .push(clean.to_string());
            }
        }
        let port_needle = format!(":{}", self_loop.local_port);
        for (key, lines) in by_server {
            let joined = lines.join("\n");
            let mentions_self = (joined.contains("127.0.0.1") || joined.contains("localhost"))
                && joined.contains(&port_needle)
                && joined.contains("/mcp");
            if mentions_self || joined.contains("mcp-stdio") && joined.contains("coding-tools-mcp")
            {
                return Some(key);
            }
        }
        return None;
    }

    let parsed: Value = serde_json::from_str(raw.trim()).ok()?;
    let container = parsed.get(json_container_key(client_id))?.as_object()?;
    let port_needle = format!(":{}", self_loop.local_port);
    for (key, entry) in container {
        let text = entry.to_string();
        let is_self_http = (text.contains("127.0.0.1") || text.contains("localhost"))
            && text.contains(&port_needle)
            && text.contains("/mcp");
        let is_self_stdio = text.contains("coding-tools-mcp") && text.contains("mcp-stdio");
        if is_self_http || is_self_stdio {
            return Some(key.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_codex_toml_losslessly_and_restores_exact_original() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let backup_dir = temp.path().join("backups");
        fs::create_dir_all(home.join(".codex")).unwrap();

        let original = r#"# User custom Codex config
model = "gpt-5.4"
approval_policy = "on-request"

[mcp_servers.playwright]
command = "npx"
args = ["@playwright/mcp@latest"]

[mcp_servers.playwright.env]
HEADLESS = "true"

[mcp_servers.keep_me]
command = "uvx"
args = ["mcp-keep"]
"#;
        let toml_path = home.join(".codex/config.toml");
        fs::write(&toml_path, original).unwrap();

        let mut settings = AppSettings::default();
        settings.global_gateway.local_port = 28765;
        settings.allow_local_noauth = true;

        let mut grouped = BTreeMap::new();
        grouped.insert("codex".to_string(), vec!["playwright".to_string()]);
        let (applied, errors) = apply_harness_takeovers(
            &home,
            &backup_dir,
            &grouped,
            TakeoverConnectMode::Auto,
            &settings,
            Path::new("/Users/test/.local/bin/coding-tools-mcp"),
        );

        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].effective_mode, ResolvedConnectMode::Http);

        let updated = fs::read_to_string(&toml_path).unwrap();
        assert!(updated.contains("# User custom Codex config"));
        assert!(updated.contains("model = \"gpt-5.4\""));
        assert!(updated.contains("[mcp_servers.keep_me]"));
        assert!(!updated.contains("[mcp_servers.playwright]"));
        assert!(!updated.contains("HEADLESS"));
        assert!(updated.contains("[mcp_servers.coding-tools]"));
        assert!(updated.contains("url = \"http://127.0.0.1:28765/mcp\""));

        // Restore must bring back the exact byte-for-byte original file.
        let restored =
            restore_takeover_backup_at(&home, &backup_dir, &applied[0].backup_id).unwrap();
        assert!(restored.restored_at_ms.is_some());
        assert_eq!(fs::read_to_string(&toml_path).unwrap(), original);
    }

    #[test]
    fn rewrites_cursor_and_claude_desktop_preserving_unrelated_keys() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let backup_dir = temp.path().join("backups");
        fs::create_dir_all(home.join(".cursor")).unwrap();
        fs::create_dir_all(home.join(".gemini")).unwrap();

        let cursor_path = home.join(".cursor/mcp.json");
        fs::write(
            &cursor_path,
            r#"{
  "otherSetting": true,
  "mcpServers": {
    "github": { "command": "npx", "args": ["-y", "gh-mcp"] },
    "local_keep": { "command": "node", "args": ["keep.js"] }
  }
}"#,
        )
        .unwrap();

        let gemini_path = home.join(".gemini/settings.json");
        fs::write(
            &gemini_path,
            r#"{
  "theme": "dark",
  "mcpServers": {
    "context7": { "command": "npx", "args": ["-y", "@upstash/context7-mcp"] }
  }
}"#,
        )
        .unwrap();

        let mut settings = AppSettings::default();
        settings.global_gateway.local_port = 28765;
        settings.allow_local_noauth = true;

        let mut grouped = BTreeMap::new();
        grouped.insert("cursor".to_string(), vec!["github".to_string()]);
        grouped.insert("gemini_cli".to_string(), vec!["context7".to_string()]);

        let (applied, errors) = apply_harness_takeovers(
            &home,
            &backup_dir,
            &grouped,
            TakeoverConnectMode::Auto,
            &settings,
            Path::new("/Users/test/.local/bin/coding-tools-mcp"),
        );

        assert!(errors.is_empty(), "errors: {errors:?}");
        assert_eq!(applied.len(), 2);

        let cursor_json: Value =
            serde_json::from_str(&fs::read_to_string(&cursor_path).unwrap()).unwrap();
        assert_eq!(cursor_json["otherSetting"], true);
        assert!(cursor_json["mcpServers"].get("github").is_none());
        assert!(cursor_json["mcpServers"].get("local_keep").is_some());
        assert_eq!(
            cursor_json["mcpServers"]["coding-tools"]["url"],
            "http://127.0.0.1:28765/mcp"
        );

        let gemini_json: Value =
            serde_json::from_str(&fs::read_to_string(&gemini_path).unwrap()).unwrap();
        assert_eq!(gemini_json["theme"], "dark");
        assert!(gemini_json["mcpServers"].get("context7").is_none());
        assert_eq!(
            gemini_json["mcpServers"]["coding-tools"]["command"],
            "/Users/test/.local/bin/coding-tools-mcp"
        );
        assert_eq!(
            gemini_json["mcpServers"]["coding-tools"]["args"],
            json!(["mcp-stdio", "http://127.0.0.1:28765/mcp"])
        );
    }

    #[test]
    fn avoids_duplicating_existing_coding_tools_entry() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let backup_dir = temp.path().join("backups");
        fs::create_dir_all(home.join(".cursor")).unwrap();

        let cursor_path = home.join(".cursor/mcp.json");
        fs::write(
            &cursor_path,
            r#"{
  "mcpServers": {
    "my-ctm": { "url": "http://127.0.0.1:28765/mcp" },
    "github": { "command": "npx", "args": ["-y", "gh"] }
  }
}"#,
        )
        .unwrap();

        let mut settings = AppSettings::default();
        settings.global_gateway.local_port = 28765;
        settings.allow_local_noauth = true;

        let mut grouped = BTreeMap::new();
        grouped.insert("cursor".to_string(), vec!["github".to_string()]);
        let (applied, errors) = apply_harness_takeovers(
            &home,
            &backup_dir,
            &grouped,
            TakeoverConnectMode::Auto,
            &settings,
            Path::new("/Users/test/.local/bin/coding-tools-mcp"),
        );

        assert!(errors.is_empty());
        assert_eq!(applied[0].injected_key, "my-ctm");
        let after: Value =
            serde_json::from_str(&fs::read_to_string(&cursor_path).unwrap()).unwrap();
        assert!(after["mcpServers"].get("github").is_none());
        assert!(after["mcpServers"].get("my-ctm").is_some());
        assert!(after["mcpServers"].get("coding-tools").is_none());
    }
}
