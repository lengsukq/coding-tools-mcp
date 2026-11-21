use super::*;
use crate::util::time::unix_now_secs;

const ACTIVITY_WINDOW_SECS: u64 = 8 * 24 * 60 * 60;
const REVIEW_LIST_LIMIT: usize = 20;

fn configured_public_review_base() -> AppResult<String> {
    let settings = crate::settings::AppSettings::load_or_default();
    let base = settings
        .global_gateway
        .public_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    if base.is_empty() {
        return Err(AppError::Message(
            "请先配置 Global Gateway Public URL".into(),
        ));
    }
    Ok(base)
}

#[tauri::command]
pub async fn issue_workspace_review_url(
    state: State<'_, AppState>,
    workspace_id: String,
    review_id: String,
) -> AppResult<String> {
    let root = state.workspace_path(&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let base = configured_public_review_base()?;
        let link = crate::review::issue_token(&root, &review_id).map_err(AppError::Message)?;
        Ok(format!("{base}/review/{}?t={}", link.change_id, link.token))
    })
    .await
    .map_err(|err| AppError::Message(format!("签发变更审查链接失败: {err}")))?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewLaunch {
    change_id: String,
    url: String,
    scope: String,
    files: usize,
    additions: usize,
    deletions: usize,
}

fn review_launch(
    root: &std::path::Path,
    link: crate::review::ReviewLink,
    scope: &str,
) -> AppResult<ReviewLaunch> {
    let base = configured_public_review_base()?;
    let review = crate::review::load_review(root, &link.change_id, &link.token)
        .map_err(AppError::Message)?;
    Ok(ReviewLaunch {
        change_id: link.change_id.clone(),
        url: format!("{base}/review/{}?t={}", link.change_id, link.token),
        scope: scope.into(),
        files: review.stats.files,
        additions: review.stats.additions,
        deletions: review.stats.deletions,
    })
}

#[tauri::command]
pub async fn create_workspace_review_url(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<Option<ReviewLaunch>> {
    let root = state.workspace_path(&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(link) = crate::review::create_workspace_review(&root, "Workspace changes vs HEAD")
            .map_err(AppError::Message)?
        else {
            return Ok(None);
        };
        review_launch(&root, link, "workspace").map(Some)
    })
    .await
    .map_err(|err| AppError::Message(format!("生成变更审查失败: {err}")))?
}

#[tauri::command]
pub async fn create_session_review_url(
    state: State<'_, AppState>,
    workspace_id: String,
    session_id: String,
) -> AppResult<Option<ReviewLaunch>> {
    let root = state.workspace_path(&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(link) =
            crate::review::create_session_review(&root, &session_id, "Session aggregate changes")
                .map_err(AppError::Message)?
        else {
            return Ok(None);
        };
        review_launch(&root, link, "session").map(Some)
    })
    .await
    .map_err(|err| AppError::Message(format!("生成会话变更审查失败: {err}")))?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReviewSummary {
    id: String,
    created_at: u64,
    expires_at: u64,
    summary: String,
    files: usize,
    additions: usize,
    deletions: usize,
    operation_ids: Vec<String>,
    storage_bytes: u64,
    session_id: Option<String>,
    scope: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceActivityEvent {
    created_at: u64,
    operations: usize,
    files: usize,
    additions: usize,
    deletions: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceActivityMetrics {
    events: Vec<WorkspaceActivityEvent>,
    total_operations: usize,
    total_files: usize,
    total_additions: usize,
    total_deletions: usize,
}

#[tauri::command]
pub async fn get_workspace_activity_metrics(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<WorkspaceActivityMetrics> {
    let root = state.workspace_path(&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::review::cleanup_expired(&root);
        let since = unix_now_secs().saturating_sub(ACTIVITY_WINDOW_SECS);
        let mut events = crate::review::list_review_summaries(&root)
            .into_iter()
            .filter(|review| review.scope == "operation" && review.created_at >= since)
            .map(|review| WorkspaceActivityEvent {
                created_at: review.created_at,
                operations: review.operation_ids.len().max(1),
                files: review.stats.files,
                additions: review.stats.additions,
                deletions: review.stats.deletions,
            })
            .collect::<Vec<_>>();
        events.sort_by_key(|event| event.created_at);
        Ok(WorkspaceActivityMetrics {
            total_operations: events.iter().map(|event| event.operations).sum(),
            total_files: events.iter().map(|event| event.files).sum(),
            total_additions: events.iter().map(|event| event.additions).sum(),
            total_deletions: events.iter().map(|event| event.deletions).sum(),
            events,
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("读取工作区活动指标失败: {err}")))?
}

#[tauri::command]
pub async fn list_workspace_reviews(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<Vec<WorkspaceReviewSummary>> {
    let root = state.workspace_path(&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::review::cleanup_expired(&root);
        let storage_bytes = crate::review::storage_bytes(&root);
        Ok(crate::review::list_review_summaries(&root)
            .into_iter()
            .take(REVIEW_LIST_LIMIT)
            .map(|review| WorkspaceReviewSummary {
                id: review.id,
                created_at: review.created_at,
                expires_at: review.expires_at,
                summary: review.summary,
                files: review.stats.files,
                additions: review.stats.additions,
                deletions: review.stats.deletions,
                operation_ids: review.operation_ids,
                storage_bytes,
                session_id: review.session_id,
                scope: review.scope,
            })
            .collect())
    })
    .await
    .map_err(|err| AppError::Message(format!("读取变更审查列表失败: {err}")))?
}

#[tauri::command]
pub async fn delete_workspace_review(
    state: State<'_, AppState>,
    workspace_id: String,
    review_id: String,
) -> AppResult<()> {
    let root = state.workspace_path(&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::review::delete_review(&root, &review_id).map_err(AppError::Message)
    })
    .await
    .map_err(|err| AppError::Message(format!("删除变更审查失败: {err}")))?
}
