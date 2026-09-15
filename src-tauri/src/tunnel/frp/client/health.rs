use super::*;

pub(crate) fn frpc_log_name() -> &'static str {
    "frpc-mcp.log"
}

/// Detect a stuck reconnect loop: recent log ends with repeated connect failures
/// and no newer login/proxy success. This is the state that leaves local MCP up
/// while the public FRP vhost returns frp's own 404 page.
#[cfg(test)]
pub(crate) fn frpc_reconnect_loop_detected(content: &str) -> bool {
    let cleaned = strip_ansi(content);
    let lines: Vec<&str> = cleaned
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return false;
    }
    let window = &lines[lines.len().saturating_sub(40)..];
    let mut trailing_errors = 0usize;
    for line in window.iter().rev() {
        let lower = line.to_ascii_lowercase();
        if is_frpc_reconnect_noise_line(&lower) {
            continue;
        }
        if is_frpc_reconnect_error_line(&lower) {
            trailing_errors += 1;
            continue;
        }
        if is_frpc_healthy_line(&lower) {
            break;
        }
        if trailing_errors > 0 {
            break;
        }
    }
    trailing_errors >= 3
}

#[cfg(test)]
fn is_frpc_reconnect_noise_line(lower: &str) -> bool {
    lower.contains("try to connect to server")
}

#[cfg(test)]
fn is_frpc_reconnect_error_line(lower: &str) -> bool {
    lower.contains("connect to server error")
        || lower.contains("i/o deadline reached")
        || lower.contains("session shutdown")
        || (lower.contains("login to the server failed") && lower.contains("timeout"))
}

#[cfg(test)]
fn is_frpc_healthy_line(lower: &str) -> bool {
    lower.contains("login to server success")
        || lower.contains("start proxy success")
        || lower.contains("proxy start success")
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicMcpProbe {
    Healthy,
    /// frps answered with its own Not Found HTML — proxy not registered.
    FrpNotRouted,
    Unexpected,
}

#[cfg(test)]
pub(crate) fn classify_public_mcp_body(status: u16, body: &str) -> PublicMcpProbe {
    let lower = body.to_ascii_lowercase();
    if is_frp_not_found_page(&lower) {
        return PublicMcpProbe::FrpNotRouted;
    }
    if status == 200
        && (lower.contains("coding-tools-mcp")
            || lower.contains("protocolversion")
            || lower.contains("\"name\""))
    {
        return PublicMcpProbe::Healthy;
    }
    // Route is live but auth may challenge; still means the proxy works.
    if matches!(status, 200 | 401 | 405) {
        return PublicMcpProbe::Healthy;
    }
    PublicMcpProbe::Unexpected
}

#[cfg(test)]
pub(crate) fn is_frp_not_found_page(lower_body: &str) -> bool {
    (lower_body.contains("powered by") && lower_body.contains("frp"))
        || (lower_body.contains("the page you requested was not found")
            && lower_body.contains("frp"))
}

pub(super) async fn wait_for_frpc_ready(
    child: &mut Child,
    log_path: &Path,
    log_offset: u64,
    expected_proxy_count: usize,
) -> AppResult<bool> {
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| AppError::Message(err.to_string()))?
        {
            sleep(Duration::from_millis(300)).await;
            return Err(frpc_exit_error(status, log_path));
        }
        if let Some(error) = detect_frpc_log_error(log_path, log_offset) {
            return Err(error);
        }
        let content = read_log_since(log_path, log_offset);
        if successful_proxy_names(&content).len() >= expected_proxy_count {
            return Ok(true);
        }
        sleep(Duration::from_millis(200)).await;
    }
    if child.try_wait().ok().flatten().is_none() {
        let detail = read_log_since(log_path, log_offset);
        let detail = if detail.trim().is_empty() {
            "尚未收到 frpc 登录或代理建立日志".to_string()
        } else {
            frpc_log_summary_from_text(&detail)
        };
        return Err(AppError::Message(format!(
            "frpc 启动超时，隧道尚未就绪：{detail}"
        )));
    }
    Ok(false)
}

fn frpc_exit_error(status: std::process::ExitStatus, log_path: &Path) -> AppError {
    let detail = frpc_log_summary(log_path);
    if detail.is_empty() {
        return AppError::Message(format!(
            "frpc 退出，状态码 {status}。请检查 FRP 服务器地址、端口、Token 与子域名；\
             若使用全局 FRP 配置，请在工作区隧道里选择对应配置。"
        ));
    }
    AppError::Message(format!("frpc 退出，状态码 {status}。{detail}"))
}

fn detect_frpc_log_error(log_path: &Path, log_offset: u64) -> Option<AppError> {
    let content = read_log_since(log_path, log_offset);
    if content.is_empty() {
        return None;
    }
    let lowered = strip_ansi(&content).to_ascii_lowercase();
    if lowered.contains("authorization failed")
        || lowered.contains("token in login doesn't match")
        || lowered.contains("login to the server failed")
        || lowered.contains("start error: proxy")
        || lowered.contains("proxy already exists")
    {
        return Some(AppError::Message(format!(
            "frpc 连接失败：{}",
            frpc_log_summary_from_text(&content)
        )));
    }
    None
}

fn frpc_log_summary(log_path: &Path) -> String {
    let content = std::fs::read_to_string(log_path).unwrap_or_default();
    frpc_log_summary_from_text(&content)
}

fn frpc_log_summary_from_text(content: &str) -> String {
    let cleaned = strip_ansi(content);
    cleaned
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("请检查 FRP 服务器、端口与 Token")
        .to_string()
}

pub(super) fn log_file_len(path: &Path) -> u64 {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn read_log_since(path: &Path, offset: u64) -> String {
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let current_len = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let start = if current_len >= offset { offset } else { 0 };
    if file.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

pub(super) async fn stream_frpc_logs<R>(stderr: R, log_paths: Vec<PathBuf>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let mut files = Vec::new();
    for log_path in log_paths {
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(file) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .await
        {
            files.push(file);
        }
    }
    if files.is_empty() {
        return;
    }

    let mut reader = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        use tokio::io::AsyncWriteExt;
        for file in &mut files {
            let _ = file.write_all(line.as_bytes()).await;
            let _ = file.write_all(b"\n").await;
            let _ = file.flush().await;
        }
    }
}

pub(super) fn successful_proxy_names(content: &str) -> HashSet<String> {
    let cleaned = strip_ansi(content);
    cleaned
        .lines()
        .filter_map(|line| {
            let lower = line.to_ascii_lowercase();
            let marker = if lower.contains("start proxy success") {
                "start proxy success"
            } else if lower.contains("proxy start success") {
                "proxy start success"
            } else {
                return None;
            };

            let marker_index = lower.find(marker)?;
            line[..marker_index]
                .rsplit_once('[')
                .map(|(_, name)| name.trim().trim_end_matches(']').trim().to_string())
                .filter(|name| !name.is_empty())
        })
        .collect()
}
