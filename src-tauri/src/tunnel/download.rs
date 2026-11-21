use std::path::Path;
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult};
use crate::settings::AppSettings;

/// A GitHub download URL rewritten to honor the configured mirror, plus the
/// original URL as a fallback. Callers try `primary` first, then `fallback`.
pub struct DownloadPlan {
    pub primary: String,
    pub fallback: Option<String>,
}

/// Build the ordered list of URLs to attempt for a GitHub release asset.
///
/// When a mirror prefix is configured we try the mirrored URL first and keep
/// the original GitHub URL as a fallback. With no mirror we just hit GitHub.
pub fn plan_github_download(settings: &AppSettings, github_url: &str) -> DownloadPlan {
    let prefix = settings.download.github_mirror.trim();
    if prefix.is_empty() {
        return DownloadPlan {
            primary: github_url.to_string(),
            fallback: None,
        };
    }
    let mirrored = format!(
        "{}/{}",
        prefix.trim_end_matches('/'),
        github_url.trim_start_matches('/')
    );
    DownloadPlan {
        primary: mirrored,
        fallback: Some(github_url.to_string()),
    }
}

fn build_client(settings: &AppSettings) -> AppResult<reqwest::Client> {
    crate::util::http::build_download_client(&settings.download, Duration::from_secs(180))
}

/// Stream a GitHub release asset directly to `dest` via a sibling temporary
/// file, avoiding buffering multi-tens-of-MB archives in heap memory.
pub async fn download_release_asset_to_file(
    settings: &AppSettings,
    github_url: &str,
    label: &str,
    dest: &Path,
) -> AppResult<()> {
    let plan = plan_github_download(settings, github_url);
    let client = build_client(settings)?;

    let mut urls = vec![plan.primary];
    if let Some(fallback) = plan.fallback {
        urls.push(fallback);
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut last_err = String::new();
    for url in urls {
        match stream_to_file(&client, &url, dest).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = err;
            }
        }
    }
    Err(AppError::Message(format!("下载 {label} 失败: {last_err}")))
}

async fn stream_to_file(client: &reqwest::Client, url: &str, dest: &Path) -> Result<(), String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?;

    let tmp_path = dest.with_extension(format!("part-{}", uuid::Uuid::new_v4().simple()));
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|err| err.to_string())?;

    let write_result: Result<(), String> = async {
        while let Some(chunk) = response.chunk().await.map_err(|err| err.to_string())? {
            file.write_all(&chunk)
                .await
                .map_err(|err| err.to_string())?;
        }
        file.flush().await.map_err(|err| err.to_string())?;
        Ok(())
    }
    .await;

    if let Err(err) = write_result {
        drop(file);
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(err);
    }
    drop(file);

    if let Err(err) = tokio::fs::rename(&tmp_path, dest).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(err.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_proxy_mode_uses_proxy_url_and_rejects_empty_url() {
        let mut settings = AppSettings::default();
        settings.download.proxy_mode = "manual".into();
        settings.download.proxy_url.clear();
        assert!(build_client(&settings).is_err());

        settings.download.proxy_url = "http://127.0.0.1:7890".into();
        assert!(build_client(&settings).is_ok());
    }
}
