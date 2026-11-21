use std::time::Duration;

use crate::error::{AppError, AppResult};
use crate::settings::DownloadConfig;

/// Build a `reqwest::Client` honoring the configured download proxy mode and timeout.
///
/// Supported `proxy_mode` values:
/// - `""` / `"none"`: disable proxy lookup
/// - `"system"`: use reqwest default system proxy detection
/// - `"manual"`: require and apply `config.proxy_url`
/// - any other non-empty string: legacy compatibility treating the value as an explicit proxy URL
pub fn build_download_client(
    config: &DownloadConfig,
    timeout: Duration,
) -> AppResult<reqwest::Client> {
    build_download_client_with_user_agent(config, timeout, None)
}

/// Same as [`build_download_client`] with an optional custom `User-Agent` header.
pub fn build_download_client_with_user_agent(
    config: &DownloadConfig,
    timeout: Duration,
    user_agent: Option<&str>,
) -> AppResult<reqwest::Client> {
    let mut builder = reqwest::Client::builder().timeout(timeout);
    if let Some(ua) = user_agent.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder.user_agent(ua);
    }

    match config.proxy_mode.trim() {
        "" | "none" => {
            builder = builder.no_proxy();
        }
        "system" => {}
        "manual" => {
            let proxy_url = config.proxy_url.trim();
            if proxy_url.is_empty() {
                return Err(AppError::Message(
                    "下载代理模式为手动，但未填写代理地址。".into(),
                ));
            }
            let proxy = reqwest::Proxy::all(proxy_url)
                .map_err(|err| AppError::Message(format!("代理地址无效: {err}")))?;
            builder = builder.proxy(proxy);
        }
        legacy_url => {
            let proxy = reqwest::Proxy::all(legacy_url)
                .map_err(|err| AppError::Message(format!("代理地址无效: {err}")))?;
            builder = builder.proxy(proxy);
        }
    }

    builder
        .build()
        .map_err(|err| AppError::Message(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_download_client_handles_none_system_and_manual_modes() {
        let none_cfg = DownloadConfig {
            github_mirror: String::new(),
            proxy_mode: "none".into(),
            proxy_url: String::new(),
        };
        assert!(build_download_client(&none_cfg, Duration::from_secs(10)).is_ok());

        let system_cfg = DownloadConfig {
            github_mirror: String::new(),
            proxy_mode: "system".into(),
            proxy_url: String::new(),
        };
        assert!(build_download_client(&system_cfg, Duration::from_secs(10)).is_ok());

        let manual_valid = DownloadConfig {
            github_mirror: String::new(),
            proxy_mode: "manual".into(),
            proxy_url: "http://127.0.0.1:7890".into(),
        };
        assert!(build_download_client(&manual_valid, Duration::from_secs(10)).is_ok());
    }

    #[test]
    fn build_download_client_rejects_manual_mode_without_url_or_invalid_url() {
        let missing_url = DownloadConfig {
            github_mirror: String::new(),
            proxy_mode: "manual".into(),
            proxy_url: "   ".into(),
        };
        let err = build_download_client(&missing_url, Duration::from_secs(10))
            .expect_err("manual mode without proxy url must fail");
        assert!(err.to_string().contains("未填写代理地址"));

        let invalid_url = DownloadConfig {
            github_mirror: String::new(),
            proxy_mode: "manual".into(),
            proxy_url: "://not a valid proxy".into(),
        };
        let err = build_download_client(&invalid_url, Duration::from_secs(10))
            .expect_err("invalid manual proxy url must fail");
        assert!(err.to_string().contains("代理地址无效"));
    }
}
