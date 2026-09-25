use crate::data::DataStore;
use crate::error::{AppError, AppResult};
use crate::tool_audit::{self, ToolAuditFilter, ToolAuditPage, DEFAULT_RETENTION_DAYS};

#[tauri::command]
pub async fn list_tool_audit_records(filter: ToolAuditFilter) -> AppResult<ToolAuditPage> {
    tauri::async_runtime::spawn_blocking(move || {
        let retention_days = DataStore::read_file(|data| {
            Ok(data
                .tool_audit_retention_days
                .unwrap_or(DEFAULT_RETENTION_DAYS))
        })?;
        tool_audit::set_retention_days(retention_days);
        tool_audit::list_tool_calls(&filter).map_err(AppError::from)
    })
    .await
    .map_err(|error| AppError::Message(format!("审计记录查询任务失败: {error}")))?
}

#[tauri::command]
pub async fn set_tool_audit_retention_days(days: u16) -> AppResult<u16> {
    if !tool_audit::is_valid_retention_days(days) {
        return Err(AppError::Message("保留期限必须在 1 到 365 天之间。".into()));
    }
    tauri::async_runtime::spawn_blocking(move || {
        DataStore::update_file(|data| {
            data.tool_audit_retention_days = Some(days);
            Ok(())
        })?;
        tool_audit::set_retention_days(days);
        Ok(days)
    })
    .await
    .map_err(|error| AppError::Message(format!("审计保留期限更新任务失败: {error}")))?
}

#[tauri::command]
pub async fn clear_tool_audit_records(workspace_id: Option<String>) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        tool_audit::clear_tool_calls(workspace_id.as_deref()).map_err(AppError::from)
    })
    .await
    .map_err(|error| AppError::Message(format!("审计记录清理任务失败: {error}")))?
}
