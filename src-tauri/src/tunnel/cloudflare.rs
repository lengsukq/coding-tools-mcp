use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use tokio::time;

use crate::error::{AppError, AppResult};
use crate::platform::platform;
use crate::settings::ProxyConfig;
use crate::tunnel::helpers::{configure_tunnel_command, set_executable_permissions};
use crate::tunnel::rotate_log_if_needed;

const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// Handle to a supervised `cloudflared` child process.
pub struct CloudflareTunnelHandle {
    pub child: Child,
    pub public_url: String,
    pub pid: Option<u32>,
}

pub fn resolve_cloudflared() -> AppResult<PathBuf> {
    platform()
        .cloudflared_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| cached_cloudflared_path().filter(|path| path.is_file()))
        .ok_or_else(|| {
            AppError::Message(
                "未找到 cloudflared。请到「软件管理」安装，或自行安装 Cloudflare Tunnel CLI。\n\
                 Windows 可执行：winget install Cloudflare.cloudflared"
                    .into(),
            )
        })
}

/// Path where the app caches a self-managed cloudflared binary.
pub(crate) fn cached_cloudflared_path() -> Option<PathBuf> {
    platform()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("bin").join(cloudflared_binary_name()))
}

pub(crate) fn cloudflared_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "cloudflared.exe"
    }
    #[cfg(not(windows))]
    {
        "cloudflared"
    }
}

/// GitHub release asset name for the current platform.
fn cloudflared_release_asset() -> AppResult<&'static str> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Ok("cloudflared-windows-amd64.exe")
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        Ok("cloudflared-windows-arm64.exe")
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Ok("cloudflared-linux-amd64")
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        Ok("cloudflared-linux-arm64")
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Ok("cloudflared-darwin-amd64.tgz")
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Ok("cloudflared-darwin-arm64.tgz")
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
    )))]
    {
        Err(AppError::Message(
            "当前平台暂不支持自动下载 cloudflared。".into(),
        ))
    }
}

/// Latest cloudflared release. Pinned for reproducibility; bump as needed.
const CLOUDFLARED_VERSION: &str = "2025.6.1";

/// Download cloudflared into the app cache `bin/` directory, honoring the
/// configured mirror + proxy. Windows/Linux assets are raw binaries; macOS
/// assets are `.tgz` archives that need extraction.
pub(crate) async fn download_cloudflared_to_cache() -> AppResult<PathBuf> {
    let settings = crate::settings::AppSettings::load_or_default();
    let asset = cloudflared_release_asset()?;
    let url = format!(
        "https://github.com/cloudflare/cloudflared/releases/download/{CLOUDFLARED_VERSION}/{asset}"
    );
    let dest =
        cached_cloudflared_path().ok_or_else(|| AppError::Message("无法解析缓存目录。".into()))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if asset.ends_with(".tgz") {
        let archive_path = dest.with_extension(format!("tgz-{}", uuid::Uuid::new_v4().simple()));
        crate::tunnel::download::download_release_asset_to_file(
            &settings,
            &url,
            "cloudflared",
            &archive_path,
        )
        .await?;
        let archive_for_extract = archive_path.clone();
        let dest_for_extract = dest.clone();
        let extract_result = tokio::task::spawn_blocking(move || {
            let result = extract_cloudflared_from_tar_gz(&archive_for_extract, &dest_for_extract);
            let _ = std::fs::remove_file(&archive_for_extract);
            result
        })
        .await
        .map_err(|err| AppError::Message(format!("解压 cloudflared 任务异常中断: {err}")))?;
        extract_result?;
    } else {
        crate::tunnel::download::download_release_asset_to_file(
            &settings,
            &url,
            "cloudflared",
            &dest,
        )
        .await?;
    }

    set_executable_permissions(&dest, "cloudflared")?;

    if dest.is_file() {
        Ok(dest)
    } else {
        Err(AppError::Message("cloudflared 自动安装失败。".into()))
    }
}

#[cfg(target_os = "macos")]
fn extract_cloudflared_from_tar_gz(archive_path: &Path, dest: &Path) -> AppResult<()> {
    crate::tunnel::helpers::extract_binary_from_tar_gz(archive_path, dest, "cloudflared", |path| {
        path.ends_with("cloudflared")
    })
}

#[cfg(not(target_os = "macos"))]
fn extract_cloudflared_from_tar_gz(_archive_path: &Path, _dest: &Path) -> AppResult<()> {
    Err(AppError::Message(
        "当前平台的 cloudflared 无需解压。".into(),
    ))
}

pub fn extract_trycloudflare_url(line: &str) -> Option<String> {
    const PREFIX: &str = "https://";
    const SUFFIX: &str = ".trycloudflare.com";
    let lower = line.to_ascii_lowercase();
    let mut search_from = 0;

    while let Some(rel) = lower[search_from..].find(PREFIX) {
        let start = search_from + rel;
        let Some(suffix_rel) = lower[start..].find(SUFFIX) else {
            break;
        };
        let end = start + suffix_rel + SUFFIX.len();
        let host = &line[start + PREFIX.len()..end - SUFFIX.len()];
        if host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && !host.is_empty() {
            return Some(line[start..end].trim_end_matches('/').to_string());
        }
        search_from = start + PREFIX.len();
    }
    None
}

/// Apply the global proxy to a tunnel child process environment.
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

/// Directory used as cwd and PID storage for managed `cloudflared` processes.
pub(crate) fn managed_cloudflared_dir(scope_id: &str) -> AppResult<PathBuf> {
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
    let folder = if sanitized.is_empty() {
        "global-mcp".to_string()
    } else {
        sanitized
    };
    Ok(platform()
        .app_config_dir()?
        .join("cloudflared")
        .join(folder))
}

pub(crate) fn managed_cloudflared_pid_path(scope_id: &str) -> AppResult<PathBuf> {
    Ok(managed_cloudflared_dir(scope_id)?.join("cloudflared.pid"))
}

fn write_managed_cloudflared_pid(scope_id: &str, pid: u32, image_path: &Path) -> AppResult<()> {
    let path = managed_cloudflared_pid_path(scope_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{pid}\n{}\n", image_path.to_string_lossy()))?;
    Ok(())
}

pub(crate) fn clear_managed_cloudflared_pid(scope_id: &str) {
    if let Ok(path) = managed_cloudflared_pid_path(scope_id) {
        let _ = std::fs::remove_file(path);
    }
}

pub(crate) async fn stop_recorded_cloudflared_instance(scope_id: &str) -> AppResult<bool> {
    let path = managed_cloudflared_pid_path(scope_id)?;
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let mut lines = contents.lines();
    let pid = lines
        .next()
        .and_then(|value| value.trim().parse::<u32>().ok());
    let recorded_image = lines
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let (Some(pid), Some(recorded_image)) = (pid, recorded_image) else {
        clear_managed_cloudflared_pid(scope_id);
        return Ok(false);
    };

    if !platform().is_process_alive(pid) {
        clear_managed_cloudflared_pid(scope_id);
        return Ok(false);
    }
    let Some(actual_image) = platform().process_image_path(pid)? else {
        clear_managed_cloudflared_pid(scope_id);
        return Ok(false);
    };
    let left =
        std::fs::canonicalize(recorded_image).unwrap_or_else(|_| PathBuf::from(recorded_image));
    let right =
        std::fs::canonicalize(&actual_image).unwrap_or_else(|_| PathBuf::from(&actual_image));
    let same = {
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
    };
    if !same {
        clear_managed_cloudflared_pid(scope_id);
        return Ok(false);
    }

    let _ = platform().terminate_process_tree(pid);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while platform().is_process_alive(pid) && tokio::time::Instant::now() < deadline {
        time::sleep(Duration::from_millis(50)).await;
    }
    clear_managed_cloudflared_pid(scope_id);
    Ok(true)
}

pub(crate) fn cloudflared_command_args(port: u16, http2: bool) -> Vec<String> {
    let mut args = vec!["tunnel".to_string()];
    if http2 {
        // Prefer HTTP/2 over QUIC to avoid frequent 1033/502 drops on restrictive networks.
        args.push("--protocol".to_string());
        args.push("http2".to_string());
        args.push("--post-quantum=false".to_string());
    }
    args.push("--url".to_string());
    args.push(format!("http://127.0.0.1:{port}"));
    args
}

/// Spawn a Cloudflare Quick Tunnel (`cloudflared tunnel [--protocol http2 ...] --url http://127.0.0.1:{port}`).
pub async fn spawn_cloudflare_tunnel(
    port: u16,
    cwd: &Path,
    log_path: &Path,
    use_proxy: bool,
    http2: bool,
) -> AppResult<CloudflareTunnelHandle> {
    let cloudflared = resolve_cloudflared()?;
    std::fs::create_dir_all(cwd)?;

    let mut cmd = Command::new(&cloudflared);
    cmd.current_dir(cwd);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    configure_tunnel_command(&mut cmd);

    let settings = crate::settings::AppSettings::load_or_default();
    if use_proxy {
        apply_proxy_env(&mut cmd, &settings.proxy);
    }

    cmd.args(cloudflared_command_args(port, http2));

    let mut child = cmd
        .spawn()
        .map_err(|err| AppError::Message(format!("启动 cloudflared 失败: {err}")))?;
    let pid = child.id();
    if let Some(pid) = pid {
        let _ = write_managed_cloudflared_pid("global-mcp", pid, &cloudflared);
    }

    if let Some(parent) = log_path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            return Err(cleanup_spawned_child(child, pid, error.into()).await);
        }
    }

    let (ready_tx, ready_rx) = oneshot::channel();
    let log_path = log_path.to_path_buf();
    let log_path_for_error = log_path.clone();

    if let Some(stdout) = child.stdout.take() {
        let stderr = child.stderr.take();
        tokio::spawn(async move {
            stream_cloudflare_output(stdout, stderr, &log_path, ready_tx).await;
        });
    } else {
        let _ = ready_tx.send(QuickTunnelReady { public_url: None });
    }

    let ready_result = match time::timeout(READY_TIMEOUT, ready_rx).await {
        Err(_) => Err(AppError::Message(format!(
            "cloudflared 已启动，但在 {} 秒内没有返回 trycloudflare.com 公网地址。\n\
             请检查：1) MCP 服务是否已在本机端口 {port} 运行；2) 设置 → 通用 → 网络代理 是否配置为手动代理（如 http://127.0.0.1:7890）；\
             3) 查看日志 {log_hint}",
            READY_TIMEOUT.as_secs(),
            log_hint = log_path_for_error.display()
        ))),
        Ok(Err(_)) => Err(AppError::Message("cloudflared 输出流意外结束。".into())),
        Ok(Ok(ready)) => Ok(ready),
    };
    let ready = match ready_result {
        Ok(ready) => ready,
        Err(error) => return Err(cleanup_spawned_child(child, pid, error).await),
    };

    let Some(public_url) = ready.public_url else {
        let error = AppError::Message(format!(
            "cloudflared 已启动，但没有解析到 trycloudflare.com 地址。请查看日志：{}",
            log_path_for_error.display()
        ));
        return Err(cleanup_spawned_child(child, pid, error).await);
    };

    Ok(CloudflareTunnelHandle {
        child,
        public_url,
        pid,
    })
}

async fn cleanup_spawned_child(child: Child, pid: Option<u32>, start_error: AppError) -> AppError {
    let stop_result = force_stop_child(child, pid).await;
    clear_managed_cloudflared_pid("global-mcp");
    match stop_result {
        Ok(()) => start_error,
        Err(cleanup_error) => AppError::Message(format!(
            "{start_error}; 清理 cloudflared 子进程失败：{cleanup_error}"
        )),
    }
}

struct QuickTunnelReady {
    public_url: Option<String>,
}

async fn stream_cloudflare_output<R, E>(
    stdout: R,
    stderr: Option<E>,
    log_path: &Path,
    ready_tx: oneshot::Sender<QuickTunnelReady>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    E: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let mut ready_tx = Some(ready_tx);
    let mut public_url: Option<String> = None;

    rotate_log_if_needed(log_path);
    let mut log = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .await
    {
        Ok(file) => file,
        Err(_) => {
            if let Some(tx) = ready_tx.take() {
                let _ = tx.send(QuickTunnelReady { public_url: None });
            }
            return;
        }
    };

    let send_ready = |tx: &mut Option<oneshot::Sender<QuickTunnelReady>>, url: Option<String>| {
        if let Some(sender) = tx.take() {
            let _ = sender.send(QuickTunnelReady { public_url: url });
        }
    };

    let handle_line =
        |line: &str,
         public_url: &mut Option<String>,
         ready_tx: &mut Option<oneshot::Sender<QuickTunnelReady>>| {
            if public_url.is_none() {
                if let Some(url) = extract_trycloudflare_url(line) {
                    *public_url = Some(url.clone());
                    send_ready(ready_tx, Some(url));
                }
            }
        };

    // cloudflared logs primarily to stderr; read stdout and stderr concurrently.
    let (line_tx, mut line_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    if let Some(stderr) = stderr {
        let stderr_line_tx = line_tx.clone();
        tokio::spawn(async move {
            let mut stderr = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = stderr.next_line().await {
                if stderr_line_tx.send(line).is_err() {
                    break;
                }
            }
        });
    }

    tokio::spawn(async move {
        let mut stdout = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = stdout.next_line().await {
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });

    while let Some(line) = line_rx.recv().await {
        let _ = log.write_all(line.as_bytes()).await;
        let _ = log.write_all(b"\n").await;
        let _ = log.flush().await;
        handle_line(&line, &mut public_url, &mut ready_tx);
    }

    send_ready(&mut ready_tx, public_url);
}

pub async fn stop_child(mut child: Child, pid: Option<u32>) -> AppResult<()> {
    if let Some(pid) = pid {
        let _ = platform().terminate_process_tree(pid);
    }

    let _ = child.kill().await;
    let _ = time::timeout(Duration::from_secs(3), child.wait()).await;
    Ok(())
}

pub async fn force_stop_child(mut child: Child, pid: Option<u32>) -> AppResult<()> {
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
        .map_err(|_| AppError::Message("等待 cloudflared 退出超时。".into()))??;
    process_tree_result
        .map_err(|error| AppError::Message(format!("终止 cloudflared 进程树失败：{error}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::extract_trycloudflare_url;

    #[test]
    fn extracts_trycloudflare_url_from_log_line() {
        let line = "INF | https://abc-def.trycloudflare.com is your tunnel URL";
        assert_eq!(
            extract_trycloudflare_url(line).as_deref(),
            Some("https://abc-def.trycloudflare.com")
        );
    }

    #[test]
    fn ignores_invalid_hosts() {
        let line = "https://bad_host.trycloudflare.com";
        assert!(extract_trycloudflare_url(line).is_none());
    }

    #[test]
    fn command_args_include_http2_flags_when_enabled() {
        assert_eq!(
            super::cloudflared_command_args(28765, true),
            vec![
                "tunnel",
                "--protocol",
                "http2",
                "--post-quantum=false",
                "--url",
                "http://127.0.0.1:28765",
            ]
        );
        assert_eq!(
            super::cloudflared_command_args(28765, false),
            vec!["tunnel", "--url", "http://127.0.0.1:28765"]
        );
    }

    #[test]
    fn managed_cloudflared_dir_lives_outside_workspace() {
        let dir = super::managed_cloudflared_dir("global-mcp").expect("dir");
        assert!(dir.is_absolute());
        assert!(dir.ends_with(std::path::Path::new("cloudflared").join("global-mcp")));
    }

    #[tokio::test]
    async fn stream_cloudflare_output_closes_cleanly_when_stderr_is_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log_path = dir.path().join("cf.log");
        let (tx, rx) = super::oneshot::channel();
        let stdout = std::io::Cursor::new(b"INF no tunnel url here\n".to_vec());
        let stderr: Option<std::io::Cursor<Vec<u8>>> = None;

        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            super::stream_cloudflare_output(stdout, stderr, &log_path, tx),
        )
        .await
        .expect("stream must not deadlock when stderr is None");

        let ready = rx.await.expect("ready signal");
        assert!(ready.public_url.is_none());
    }
}
