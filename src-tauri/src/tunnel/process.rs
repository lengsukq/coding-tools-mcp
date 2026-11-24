//! Shared managed child-process helpers for tunnel providers (`cloudflared`, `frpc`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio::time;

use crate::error::{AppError, AppResult};
use crate::platform::platform;
use crate::settings::ProxyConfig;

/// Apply the global proxy settings to a tunnel child process environment.
pub(crate) fn apply_proxy_env(cmd: &mut Command, proxy: &ProxyConfig) {
    let url = match proxy.mode.as_str() {
        "manual" if !proxy.url.trim().is_empty() => Some(proxy.url.trim().to_string()),
        "system" => std::env::var("HTTPS_PROXY")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("HTTP_PROXY").ok().filter(|s| !s.is_empty()))
            .or_else(|| std::env::var("ALL_PROXY").ok().filter(|s| !s.is_empty())),
        _ => None,
    };
    if let Some(url) = url {
        for key in [
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "https_proxy",
            "http_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            cmd.env(key, &url);
        }
        // Some cloudflared builds consult this dedicated variable.
        cmd.env("TUNNEL_HTTP_PROXY", &url);
    }
}

pub(crate) fn sanitize_scope_id(scope_id: &str, fallback: &str) -> String {
    let sanitized: String = scope_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

pub(crate) fn managed_service_dir(
    service: &str,
    scope_id: &str,
    fallback: &str,
) -> AppResult<PathBuf> {
    Ok(platform()
        .app_config_dir()?
        .join(service)
        .join(sanitize_scope_id(scope_id, fallback)))
}

pub(crate) fn write_managed_pid(path: &Path, pid: u32, image_path: &Path) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{pid}\n{}\n", image_path.to_string_lossy()))?;
    Ok(())
}

pub(crate) fn clear_managed_pid(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Read a managed PID file (`<pid>\n<image_path>\n`) and verify that the process
/// is still alive and matches the recorded executable image. Stale or mismatched
/// PID files are removed automatically and return `Ok(None)`.
pub(crate) fn verify_recorded_managed_pid(path: &Path) -> AppResult<Option<(u32, String)>> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut lines = contents.lines();
    let pid = lines
        .next()
        .and_then(|value| value.trim().parse::<u32>().ok());
    let recorded_image = lines
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let (Some(pid), Some(recorded_image)) = (pid, recorded_image) else {
        clear_managed_pid(path);
        return Ok(None);
    };

    if !platform().is_process_alive(pid) {
        clear_managed_pid(path);
        return Ok(None);
    }
    let Some(actual_image) = platform().process_image_path(pid)? else {
        clear_managed_pid(path);
        return Ok(None);
    };
    if !same_process_image(Path::new(&recorded_image), Path::new(&actual_image)) {
        clear_managed_pid(path);
        return Ok(None);
    }
    Ok(Some((pid, recorded_image)))
}

pub(crate) fn same_process_image(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .eq_ignore_ascii_case(right.to_string_lossy().trim_start_matches("\\\\?\\"))
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

pub async fn stop_child(mut child: Child, pid: Option<u32>) -> AppResult<()> {
    if let Some(pid) = pid {
        let _ = platform().terminate_process_tree(pid);
    }
    let _ = child.kill().await;
    let _ = time::timeout(Duration::from_secs(3), child.wait()).await;
    Ok(())
}

pub async fn force_stop_child(mut child: Child, pid: Option<u32>, label: &str) -> AppResult<()> {
    let already_exited = child.try_wait().ok().flatten().is_some();
    let process_tree_result = if already_exited {
        Ok(())
    } else if let Some(pid) = pid {
        platform().terminate_process_tree(pid)
    } else {
        Ok(())
    };

    if !already_exited {
        let _ = child.kill().await;
    }
    time::timeout(Duration::from_secs(3), child.wait())
        .await
        .map_err(|_| AppError::Message(format!("等待 {label} 退出超时。")))??;
    process_tree_result
        .map_err(|error| AppError::Message(format!("终止 {label} 进程树失败：{error}")))?;
    Ok(())
}
