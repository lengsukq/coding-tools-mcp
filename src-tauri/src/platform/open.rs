use std::process::Command;

use crate::error::{AppError, AppResult};

pub fn open_path_in_file_manager(path: &std::path::Path) -> AppResult<()> {
    if !path.is_dir() {
        return Err(AppError::Message(format!(
            "路径不存在或不是目录: {}",
            path.display()
        )));
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|err| AppError::Message(format!("无法打开目录: {err}")))
            .map(|_| ())
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|err| AppError::Message(format!("无法打开目录: {err}")))
            .map(|_| ())
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|err| AppError::Message(format!("无法打开目录: {err}")))
            .map(|_| ())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        Err(AppError::Message("当前平台不支持打开目录。".into()))
    }
}

/// Open an http(s) URL in the system default browser.
pub fn open_url(url: &str) -> AppResult<()> {
    let url = url.trim();
    if !is_allowed_url(url) {
        return Err(AppError::Message(
            "仅支持打开 http:// 或 https:// 链接。".into(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        // `start` needs an empty window title when the URL may contain `&`.
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|err| AppError::Message(format!("无法打开链接: {err}")))
            .map(|_| ())
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|err| AppError::Message(format!("无法打开链接: {err}")))
            .map(|_| ())
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|err| AppError::Message(format!("无法打开链接: {err}")))
            .map(|_| ())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = url;
        Err(AppError::Message("当前平台不支持打开链接。".into()))
    }
}

pub fn is_allowed_url(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() || url.chars().any(|c| c.is_control()) {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

#[cfg(test)]
mod tests {
    use super::is_allowed_url;

    #[test]
    fn allows_http_and_https() {
        assert!(is_allowed_url("https://github.com/lengsukq/coding-tools-mcp"));
        assert!(is_allowed_url("http://example.com/path"));
        assert!(is_allowed_url("  HTTPS://Example.COM  "));
    }

    #[test]
    fn rejects_non_http_schemes_and_junk() {
        assert!(!is_allowed_url(""));
        assert!(!is_allowed_url("file:///tmp"));
        assert!(!is_allowed_url("javascript:alert(1)"));
        assert!(!is_allowed_url("ftp://example.com"));
        assert!(!is_allowed_url("https://example.com/\nevil"));
    }
}
