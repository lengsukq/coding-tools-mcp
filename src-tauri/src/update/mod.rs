use std::cmp::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::settings::AppSettings;

pub const REPO_URL: &str = "https://github.com/lengsukq/coding-tools-mcp";
pub const RELEASES_LATEST_URL: &str = "https://github.com/lengsukq/coding-tools-mcp/releases/latest";
pub const RELEASES_API_URL: &str =
    "https://api.github.com/repos/lengsukq/coding-tools-mcp/releases/latest";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub latest_tag: String,
    pub update_available: bool,
    pub release_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseLatest {
    tag_name: String,
    html_url: Option<String>,
}

pub fn current_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn normalize_tag(tag: &str) -> String {
    let trimmed = tag.trim();
    let without_v = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    without_v.trim().to_string()
}

/// Compare two semver-like strings (major.minor.patch[+pre]).
/// Returns None when either side cannot be parsed as numeric major.minor.patch.
pub fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    let left_parts = parse_version_tuple(&normalize_tag(left))?;
    let right_parts = parse_version_tuple(&normalize_tag(right))?;
    Some(left_parts.cmp(&right_parts))
}

fn parse_version_tuple(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next().unwrap_or(version).trim();
    if core.is_empty() {
        return None;
    }
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    if parts.next().is_some() {
        // Extra numeric segments are ignored for comparison stability.
    }
    Some((major, minor, patch))
}

pub fn parse_latest_release(body: &str, current_version: &str) -> AppResult<UpdateCheckResult> {
    let release: GithubReleaseLatest = serde_json::from_str(body)
        .map_err(|err| AppError::Message(format!("无法解析 GitHub Release 响应: {err}")))?;
    if release.tag_name.trim().is_empty() {
        return Err(AppError::Message(
            "GitHub Release 响应缺少 tag_name。".into(),
        ));
    }
    let latest_tag = release.tag_name.trim().to_string();
    let latest_version = normalize_tag(&latest_tag);
    if compare_versions(&latest_version, "0.0.0").is_none() {
        return Err(AppError::Message(format!(
            "无法解析最新版本号: {latest_tag}"
        )));
    }
    let current = normalize_tag(current_version);
    let ordering = compare_versions(&latest_version, &current).ok_or_else(|| {
        AppError::Message(format!("无法比较版本: 当前={current} 最新={latest_version}"))
    })?;
    let release_url = release
        .html_url
        .filter(|url| crate::platform::is_allowed_url(url))
        .unwrap_or_else(|| RELEASES_LATEST_URL.to_string());

    Ok(UpdateCheckResult {
        current_version: current,
        latest_version,
        latest_tag,
        update_available: ordering == Ordering::Greater,
        release_url,
    })
}

fn build_update_client(settings: &AppSettings) -> AppResult<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(format!(
            "CodingToolsMCP/{} (+{})",
            current_app_version(),
            REPO_URL
        ));

    let mode = settings.download.proxy_mode.trim();
    match mode {
        "" | "none" => {
            builder = builder.no_proxy();
        }
        "system" => {}
        "manual" => {
            let proxy_url = settings.download.proxy_url.trim();
            if proxy_url.is_empty() {
                return Err(AppError::Message(
                    "下载代理模式为手动，但未填写代理地址。".into(),
                ));
            }
            let proxy = reqwest::Proxy::all(proxy_url)
                .map_err(|err| AppError::Message(format!("代理地址无效: {err}")))?;
            builder = builder.proxy(proxy);
        }
        url => {
            let proxy = reqwest::Proxy::all(url)
                .map_err(|err| AppError::Message(format!("代理地址无效: {err}")))?;
            builder = builder.proxy(proxy);
        }
    }

    builder
        .build()
        .map_err(|err| AppError::Message(err.to_string()))
}

pub async fn check_app_update(settings: &AppSettings) -> AppResult<UpdateCheckResult> {
    let client = build_update_client(settings)?;
    let response = client
        .get(RELEASES_API_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| AppError::Message(format!("检查更新失败: {err}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| AppError::Message(format!("读取更新响应失败: {err}")))?;

    if !status.is_success() {
        return Err(AppError::Message(format!(
            "检查更新失败（HTTP {status}）。可稍后重试，或手动打开 Releases 页面。"
        )));
    }

    parse_latest_release(&body, current_app_version())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn normalize_strips_v_prefix() {
        assert_eq!(normalize_tag("v0.1.23"), "0.1.23");
        assert_eq!(normalize_tag("V1.2.3"), "1.2.3");
        assert_eq!(normalize_tag(" 0.1.0 "), "0.1.0");
    }

    #[test]
    fn compare_versions_orders_semver() {
        assert_eq!(
            compare_versions("0.1.23", "0.1.22"),
            Some(Ordering::Greater)
        );
        assert_eq!(compare_versions("v0.1.23", "0.1.23"), Some(Ordering::Equal));
        assert_eq!(
            compare_versions("0.1.20", "v0.1.23"),
            Some(Ordering::Less)
        );
        assert_eq!(compare_versions("1.0.0", "0.9.9"), Some(Ordering::Greater));
        assert!(compare_versions("latest", "0.1.0").is_none());
    }

    #[test]
    fn parse_latest_detects_newer_release() {
        let body = r#"{
            "tag_name": "v0.1.99",
            "html_url": "https://github.com/lengsukq/coding-tools-mcp/releases/tag/v0.1.99"
        }"#;
        let result = parse_latest_release(body, "0.1.23").expect("parse");
        assert!(result.update_available);
        assert_eq!(result.latest_version, "0.1.99");
        assert_eq!(result.latest_tag, "v0.1.99");
        assert!(result.release_url.contains("v0.1.99"));
    }

    #[test]
    fn parse_latest_reports_up_to_date() {
        let body = r#"{
            "tag_name": "v0.1.23",
            "html_url": "https://github.com/lengsukq/coding-tools-mcp/releases/tag/v0.1.23"
        }"#;
        let result = parse_latest_release(body, "0.1.23").expect("parse");
        assert!(!result.update_available);
        assert_eq!(result.current_version, "0.1.23");
    }

    #[test]
    fn parse_latest_rejects_bad_tag() {
        let body = r#"{"tag_name":"nightly","html_url":"https://example.com"}"#;
        assert!(parse_latest_release(body, "0.1.23").is_err());
    }
}
