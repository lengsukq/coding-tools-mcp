use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::tunnel::log_dir_for_profile;

pub(crate) const DEFAULT_RETENTION_DAYS: u16 = 30;
const MAX_RETENTION_DAYS: u16 = 365;
const DAY_MS: u64 = 24 * 60 * 60 * 1000;
const MAX_PAGE_SIZE: usize = 200;
const PRUNE_INTERVAL_MS: u64 = 60 * 60 * 1000;

static RETENTION_DAYS: AtomicU16 = AtomicU16::new(DEFAULT_RETENTION_DAYS);
static STORE: OnceLock<Mutex<ToolAuditStore>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolAuditRecord {
    pub timestamp_ms: u64,
    pub duration_ms: u64,
    pub request_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub tool_name: String,
    pub wrapper_tool_name: Option<String>,
    pub auth_method: String,
    pub client_id: Option<String>,
    pub outcome: String,
    pub error_category: Option<String>,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub change_id: Option<String>,
    pub changed_file_count: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolAuditFilter {
    pub workspace_id: Option<String>,
    pub from_ms: Option<u64>,
    pub to_ms: Option<u64>,
    pub session_id: Option<String>,
    pub tool_name: Option<String>,
    pub outcome: Option<String>,
    pub search: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolAuditPage {
    pub records: Vec<ToolAuditRecord>,
    pub total: usize,
    pub error_count: usize,
    pub workspace_count: usize,
    pub offset: usize,
    pub limit: usize,
    pub retention_days: u16,
    pub health_message: Option<String>,
}

pub(crate) struct ToolAuditCall {
    timestamp_ms: u64,
    started: Instant,
    request_id: Option<String>,
    session_id: Option<String>,
    tool_name: String,
    wrapper_tool_name: Option<String>,
    auth_method: String,
    client_id: Option<String>,
    request_bytes: u64,
}

impl ToolAuditCall {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        request_id: &Value,
        tool_name: &str,
        session_id: Option<&str>,
        auth_method: &str,
        client_id: Option<&str>,
        request_bytes: usize,
        body: &Value,
    ) -> Self {
        let delegated_tool = if tool_name == "workspace_invoke" {
            body.get("params")
                .and_then(|params| params.get("arguments"))
                .and_then(|arguments| arguments.get("tool"))
                .and_then(Value::as_str)
                .map(safe_label)
        } else {
            None
        };

        Self {
            timestamp_ms: unix_time_ms(),
            started: Instant::now(),
            request_id: bounded_identifier(request_id, 128),
            session_id: session_id.map(|value| bounded_identifier_text(value, 256)),
            tool_name: delegated_tool
                .clone()
                .unwrap_or_else(|| safe_label(tool_name)),
            wrapper_tool_name: delegated_tool.map(|_| "workspace_invoke".to_string()),
            auth_method: safe_auth_method(auth_method),
            client_id: client_id.map(|value| bounded_visible_text(value, 128)),
            request_bytes: request_bytes as u64,
        }
    }

    fn finish(self, workspace_id: Option<String>, response: &Value) -> ToolAuditRecord {
        let response_bytes = serde_json::to_vec(response)
            .map(|bytes| bytes.len() as u64)
            .unwrap_or_default();
        let (outcome, error_category) = classify_outcome(response);
        let structured = response
            .get("result")
            .and_then(|result| result.get("structuredContent"));
        let change_id = structured
            .and_then(|content| content.get("change_id"))
            .and_then(Value::as_str)
            .filter(|value| safe_change_id(value))
            .map(str::to_string);
        let changed_file_count = structured
            .and_then(|content| content.get("affected_files"))
            .and_then(Value::as_array)
            .map(|files| files.len() as u64);

        ToolAuditRecord {
            timestamp_ms: self.timestamp_ms,
            duration_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            request_id: self.request_id,
            session_id: self.session_id,
            workspace_id,
            tool_name: self.tool_name,
            wrapper_tool_name: self.wrapper_tool_name,
            auth_method: self.auth_method,
            client_id: self.client_id,
            outcome,
            error_category,
            request_bytes: self.request_bytes,
            response_bytes,
            change_id,
            changed_file_count,
        }
    }
}

struct ToolAuditStore {
    root: PathBuf,
    last_write_error: Option<String>,
    last_prune_ms: u64,
}

impl ToolAuditStore {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            last_write_error: None,
            last_prune_ms: 0,
        }
    }

    fn append(&mut self, record: &ToolAuditRecord) -> std::io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let path = self.event_path(record);
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        serde_json::to_writer(&mut file, record).map_err(std::io::Error::other)?;
        file.write_all(b"\n")?;
        file.flush()?;
        self.last_write_error = None;
        Ok(())
    }

    fn list(
        &mut self,
        filter: &ToolAuditFilter,
        retention_days: u16,
    ) -> std::io::Result<ToolAuditPage> {
        let now = unix_time_ms();
        if self.prune_expired(now, retention_days).is_err() {
            self.last_write_error = Some(
                "自动清理过期审计记录失败；请检查应用日志目录权限和磁盘空间。".into(),
            );
        }

        let offset = filter.offset.unwrap_or(0).min(1_000_000);
        let limit = filter.limit.unwrap_or(50).clamp(1, MAX_PAGE_SIZE);
        let mut matching = self.read_records(filter)?;
        matching.sort_by(|left, right| {
            right
                .timestamp_ms
                .cmp(&left.timestamp_ms)
                .then_with(|| right.request_id.cmp(&left.request_id))
        });
        let total = matching.len();
        let error_count = matching
            .iter()
            .filter(|record| record.outcome == "error")
            .count();
        let workspace_count = matching
            .iter()
            .filter_map(|record| record.workspace_id.as_deref())
            .collect::<std::collections::HashSet<_>>()
            .len();
        let records = matching.into_iter().skip(offset).take(limit).collect();

        Ok(ToolAuditPage {
            records,
            total,
            error_count,
            workspace_count,
            offset,
            limit,
            retention_days,
            health_message: self.last_write_error.clone(),
        })
    }

    fn read_records(&mut self, filter: &ToolAuditFilter) -> std::io::Result<Vec<ToolAuditRecord>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let workspace_prefix = filter.workspace_id.as_deref().map(workspace_prefix);
        let search = filter.search.as_deref().map(|value| value.to_lowercase());
        let session_filter = filter
            .session_id
            .as_deref()
            .map(|value| value.to_lowercase());
        let tool_filter = filter
            .tool_name
            .as_deref()
            .map(|value| value.to_lowercase());
        let outcome_filter = filter.outcome.as_deref();
        let mut records = Vec::new();

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".jsonl")
                || workspace_prefix
                    .as_deref()
                    .is_some_and(|prefix| !name.starts_with(prefix))
                || !file_day_in_range(
                    &name,
                    filter.from_ms.unwrap_or(0),
                    filter.to_ms.unwrap_or(u64::MAX),
                )
            {
                continue;
            }

            let file = match std::fs::File::open(entry.path()) {
                Ok(file) => file,
                Err(_) => {
                    self.last_write_error =
                        Some("部分审计记录无法读取；请检查应用日志目录权限。".into());
                    continue;
                }
            };
            for line in BufReader::new(file).lines() {
                let Ok(line) = line else {
                    self.last_write_error =
                        Some("部分审计记录无法读取；请检查应用日志目录权限。".into());
                    continue;
                };
                let Ok(record) = serde_json::from_str::<ToolAuditRecord>(&line) else {
                    self.last_write_error =
                        Some("发现无法解析的审计记录；其他可读记录仍可查看。".into());
                    continue;
                };

                if filter
                    .from_ms
                    .is_some_and(|from| record.timestamp_ms < from)
                    || filter.to_ms.is_some_and(|to| record.timestamp_ms > to)
                    || filter
                        .workspace_id
                        .as_deref()
                        .is_some_and(|workspace| record.workspace_id.as_deref() != Some(workspace))
                    || session_filter.as_deref().is_some_and(|session| {
                        !record
                            .session_id
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(session)
                    })
                    || tool_filter
                        .as_deref()
                        .is_some_and(|tool| !record.tool_name.to_lowercase().contains(tool))
                    || outcome_filter.is_some_and(|outcome| record.outcome != outcome)
                    || search.as_deref().is_some_and(|needle| {
                        ![
                            record.request_id.as_deref().unwrap_or_default(),
                            record.session_id.as_deref().unwrap_or_default(),
                            record.tool_name.as_str(),
                            record.client_id.as_deref().unwrap_or_default(),
                        ]
                        .iter()
                        .any(|value| value.to_lowercase().contains(needle))
                    })
                {
                    continue;
                }
                records.push(record);
            }
        }
        Ok(records)
    }

    fn clear(&mut self, workspace_id: Option<&str>) -> std::io::Result<()> {
        if !self.root.exists() {
            self.last_write_error = None;
            return Ok(());
        }
        let prefix = workspace_id.map(workspace_prefix);
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_file()
                && name.ends_with(".jsonl")
                && prefix
                    .as_deref()
                    .is_none_or(|prefix| name.starts_with(prefix))
            {
                fs::remove_file(entry.path())?;
            }
        }
        self.last_write_error = None;
        Ok(())
    }

    fn prune_expired(&mut self, now_ms: u64, retention_days: u16) -> std::io::Result<()> {
        if !self.root.exists() {
            return Ok(());
        }
        let current_day = now_ms / DAY_MS;
        let first_kept_day = current_day.saturating_sub(retention_days.saturating_sub(1) as u64);
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(day) = file_day(&name) {
                if day < first_kept_day {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        self.last_prune_ms = now_ms;
        Ok(())
    }

    fn prune_if_due(&mut self, now_ms: u64, retention_days: u16) {
        if now_ms.saturating_sub(self.last_prune_ms) < PRUNE_INTERVAL_MS {
            return;
        }
        if self.prune_expired(now_ms, retention_days).is_err() {
            self.last_write_error =
                Some("自动清理过期审计记录失败；请检查应用日志目录权限和磁盘空间。".into());
        }
    }

    fn event_path(&self, record: &ToolAuditRecord) -> PathBuf {
        let day = record.timestamp_ms / DAY_MS;
        let scope = record
            .workspace_id
            .as_deref()
            .map(workspace_prefix)
            .unwrap_or_else(|| "global-".to_string());
        self.root.join(format!("{scope}{day}.jsonl"))
    }
}

fn store() -> &'static Mutex<ToolAuditStore> {
    STORE.get_or_init(|| {
        Mutex::new(ToolAuditStore::new(
            log_dir_for_profile("global-mcp").join("tool-audit"),
        ))
    })
}

fn with_store<R>(f: impl FnOnce(&mut ToolAuditStore) -> R) -> R {
    let mut store = store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut store)
}

pub(crate) fn initialize(retention_days: u16) {
    let retention_days = normalize_retention_days(retention_days);
    RETENTION_DAYS.store(retention_days, Ordering::Relaxed);
    with_store(|store| {
        if store.prune_expired(unix_time_ms(), retention_days).is_err() {
            store.last_write_error =
                Some("启动时清理过期审计记录失败；请检查应用日志目录权限和磁盘空间。".into());
        }
    });
}

pub(crate) fn normalize_retention_days(days: u16) -> u16 {
    days.clamp(1, MAX_RETENTION_DAYS)
}

pub(crate) fn is_valid_retention_days(days: u16) -> bool {
    (1..=MAX_RETENTION_DAYS).contains(&days)
}

pub(crate) fn set_retention_days(days: u16) {
    let days = normalize_retention_days(days);
    RETENTION_DAYS.store(days, Ordering::Relaxed);
    with_store(|store| {
        if store.prune_expired(unix_time_ms(), days).is_err() {
            store.last_write_error =
                Some("更新保留期限后清理旧记录失败；请检查应用日志目录权限和磁盘空间。".into());
        }
    });
}

pub(crate) fn record_tool_call(
    call: ToolAuditCall,
    workspace_id: Option<String>,
    response: &Value,
) {
    let record = call.finish(workspace_id, response);
    with_store(|store| {
        if store.append(&record).is_err() {
            store.last_write_error = Some(
                "最近一次 MCP 调用未能写入审计记录；请检查磁盘空间和应用日志目录权限。".into(),
            );
        } else {
            store.prune_if_due(unix_time_ms(), RETENTION_DAYS.load(Ordering::Relaxed));
        }
    });
}

pub(crate) fn list_tool_calls(filter: &ToolAuditFilter) -> std::io::Result<ToolAuditPage> {
    with_store(|store| store.list(filter, RETENTION_DAYS.load(Ordering::Relaxed)))
}

pub(crate) fn clear_tool_calls(workspace_id: Option<&str>) -> std::io::Result<()> {
    with_store(|store| store.clear(workspace_id))
}

fn classify_outcome(response: &Value) -> (String, Option<String>) {
    if let Some(error) = response.get("error") {
        let data = error.get("data");
        let category = if data
            .and_then(|value| value.get("stage"))
            .and_then(Value::as_str)
            == Some("rpc_worker")
        {
            "runtime"
        } else if data
            .and_then(|value| value.get("reason").or_else(|| value.get("code")))
            .and_then(Value::as_str)
            .is_some_and(|code| {
                let code = code.to_ascii_uppercase();
                code.contains("POLICY") || code.contains("PERMISSION") || code.contains("DENIED")
            })
        {
            "policy"
        } else {
            "protocol"
        };
        return ("error".into(), Some(category.into()));
    }

    let result = response.get("result");
    if result
        .and_then(|value| value.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let raw = result
            .and_then(|value| value.get("structuredContent"))
            .and_then(|value| value.get("error"))
            .and_then(|value| value.get("category"))
            .and_then(Value::as_str)
            .unwrap_or("tool");
        return ("error".into(), Some(safe_error_category(raw)));
    }

    ("success".into(), None)
}

fn safe_error_category(value: &str) -> String {
    match value {
        "policy" | "permission" | "security" => "policy",
        "validation" | "not_found" | "runtime" | "storage" | "filesystem" | "internal" => value,
        _ => "tool",
    }
    .to_string()
}

fn safe_auth_method(value: &str) -> String {
    match value {
        "oauth" | "shared_bearer" => value,
        _ => "none",
    }
    .to_string()
}

fn safe_label(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return "unknown".into();
    }
    let value: String = value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/')
        })
        .take(128)
        .collect();
    if value.is_empty() {
        "unknown".into()
    } else {
        value
    }
}

fn bounded_identifier(value: &Value, max_chars: usize) -> Option<String> {
    if value.is_null() {
        return None;
    }
    Some(
        value
            .to_string()
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':')
            })
            .take(max_chars)
            .collect(),
    )
}

fn bounded_identifier_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':')
        })
        .take(max_chars)
        .collect()
}

fn bounded_visible_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect()
}

fn safe_change_id(value: &str) -> bool {
    (8..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn workspace_prefix(workspace_id: &str) -> String {
    format!("w-{}-", workspace_prefix_key(workspace_id))
}

fn workspace_prefix_key(workspace_id: &str) -> String {
    format!("{:x}", Sha256::digest(workspace_id.as_bytes()))
}

fn file_day_in_range(name: &str, from_ms: u64, to_ms: u64) -> bool {
    file_day(name).is_some_and(|day| {
        let from_day = from_ms / DAY_MS;
        let to_day = to_ms / DAY_MS;
        day >= from_day && day <= to_day
    })
}

fn file_day(name: &str) -> Option<u64> {
    let stem = name.strip_suffix(".jsonl")?;
    stem.rsplit('-').next()?.parse().ok()
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use std::thread;

    fn test_store() -> (tempfile::TempDir, ToolAuditStore) {
        let dir = tempfile::tempdir().expect("audit temp dir");
        let store = ToolAuditStore::new(dir.path().to_path_buf());
        (dir, store)
    }

    fn record(workspace_id: Option<&str>, timestamp_ms: u64, outcome: &str) -> ToolAuditRecord {
        ToolAuditRecord {
            timestamp_ms,
            duration_ms: 12,
            request_id: Some("42".into()),
            session_id: Some("session-a".into()),
            workspace_id: workspace_id.map(str::to_string),
            tool_name: "read_file".into(),
            wrapper_tool_name: None,
            auth_method: "shared_bearer".into(),
            client_id: None,
            outcome: outcome.into(),
            error_category: (outcome == "error").then_some("runtime".into()),
            request_bytes: 100,
            response_bytes: 80,
            change_id: None,
            changed_file_count: None,
        }
    }

    #[test]
    fn writes_only_safe_metadata_and_reads_across_store_restarts() {
        let (dir, mut store) = test_store();
        let call = ToolAuditCall::new(
            &json!(7),
            "patch",
            Some("session-a"),
            "shared_bearer",
            None,
            420,
            &json!({"params":{"arguments":{"patch":"SECRET_PAYLOAD"}}}),
        );
        let response = json!({
            "result": {
                "isError": false,
                "structuredContent": {
                    "change_id": "1234567890abcdef1234567890abcdef",
                    "affected_files": [{"path": "/private/project/file.rs"}],
                    "review_url": "https://example.test/review?t=SECRET_TOKEN"
                }
            }
        });
        store
            .append(&call.finish(Some("workspace-a".into()), &response))
            .unwrap();
        let contents = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
            .fold(String::new(), |mut combined, file| {
                combined.push_str(&file);
                combined
            });
        assert!(!contents.contains("SECRET_PAYLOAD"));
        assert!(!contents.contains("SECRET_TOKEN"));
        assert!(!contents.contains("/private/project/file.rs"));
        assert!(contents.contains("1234567890abcdef1234567890abcdef"));
        assert!(contents.contains("\"changedFileCount\":1"));

        let mut restarted = ToolAuditStore::new(dir.path().to_path_buf());
        let page = restarted
            .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.records[0].tool_name, "patch");
        assert_eq!(page.records[0].workspace_id.as_deref(), Some("workspace-a"));
    }

    #[test]
    fn filters_and_clears_by_workspace_without_touching_global_events() {
        let (dir, mut store) = test_store();
        let timestamp = unix_time_ms();
        store
            .append(&record(Some("workspace-a"), timestamp, "success"))
            .unwrap();
        store
            .append(&record(Some("workspace-b"), timestamp + 1, "error"))
            .unwrap();
        store
            .append(&record(None, timestamp + 2, "success"))
            .unwrap();

        let filter = ToolAuditFilter {
            workspace_id: Some("workspace-b".into()),
            outcome: Some("error".into()),
            from_ms: Some(timestamp + 1),
            to_ms: Some(timestamp + 1),
            session_id: Some("SESSION-A".into()),
            tool_name: Some("READ_FILE".into()),
            ..ToolAuditFilter::default()
        };
        let page = store.list(&filter, DEFAULT_RETENTION_DAYS).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.error_count, 1);

        store.clear(Some("workspace-a")).unwrap();
        let page = store
            .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
            .unwrap();
        assert_eq!(page.total, 2);
        assert!(page
            .records
            .iter()
            .any(|record| record.workspace_id.is_none()));

        drop(store);
        let mut store = ToolAuditStore::new(dir.path().to_path_buf());
        store.clear(None).unwrap();
        assert_eq!(
            store
                .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
                .unwrap()
                .total,
            0
        );
    }

    #[test]
    fn retention_removes_old_day_partitions_and_keeps_recent_records() {
        let (dir, mut store) = test_store();
        let now = unix_time_ms();
        store
            .append(&record(
                Some("workspace-a"),
                now.saturating_sub(3 * DAY_MS),
                "success",
            ))
            .unwrap();
        store
            .append(&record(Some("workspace-a"), now, "success"))
            .unwrap();

        store.prune_expired(now, 2).unwrap();
        let page = store.list(&ToolAuditFilter::default(), 2).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.records[0].timestamp_ms, now);
        assert!(!fs::read_dir(dir.path()).unwrap().any(|entry| {
            file_day(&entry.unwrap().file_name().to_string_lossy())
                .is_some_and(|day| day < now / DAY_MS - 1)
        }));
    }

    #[test]
    fn concurrent_appends_keep_every_record() {
        let dir = tempfile::tempdir().expect("audit temp dir");
        let store = Arc::new(Mutex::new(ToolAuditStore::new(dir.path().to_path_buf())));
        let mut workers = Vec::new();
        for index in 0..8 {
            let store = store.clone();
            workers.push(thread::spawn(move || {
                for offset in 0..25 {
                    let mut record = record(Some("workspace-a"), unix_time_ms(), "success");
                    record.request_id = Some(format!("{}-{offset}", index));
                    store.lock().unwrap().append(&record).unwrap();
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        let page = store
            .lock()
            .unwrap()
            .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
            .unwrap();
        assert_eq!(page.total, 200);
    }

    #[test]
    fn classifies_policy_tool_and_worker_failures_without_storing_messages() {
        assert_eq!(
            classify_outcome(&json!({"error":{"data":{"code":"POLICY_DENIED"}}})),
            ("error".into(), Some("policy".into()))
        );
        assert_eq!(
            classify_outcome(&json!({"error":{"data":{"stage":"rpc_worker"}}})),
            ("error".into(), Some("runtime".into()))
        );
        assert_eq!(
            classify_outcome(
                &json!({"result":{"isError":true,"structuredContent":{"error":{"category":"permission","message":"private path"}}}})
            ),
            ("error".into(), Some("policy".into()))
        );
    }

    #[test]
    fn records_success_tool_policy_runtime_and_global_outcomes() {
        let (dir, mut store) = test_store();
        let outcomes = [
            (
                json!({"result":{"isError":false,"structuredContent":{"message":"private result"}}}),
                "success",
                None,
                Some("workspace-a"),
            ),
            (
                json!({"result":{"isError":true,"structuredContent":{"error":{"category":"validation","message":"private tool error"}}}}),
                "error",
                Some("validation"),
                Some("workspace-a"),
            ),
            (
                json!({"error":{"code":-32602,"message":"private policy detail","data":{"reason":"POLICY_DENIED"}}}),
                "error",
                Some("policy"),
                None,
            ),
            (
                json!({"error":{"code":-32603,"message":"private runtime detail","data":{"stage":"rpc_worker","reason":"worker_failed"}}}),
                "error",
                Some("runtime"),
                None,
            ),
        ];

        for (index, (response, expected_outcome, expected_category, workspace_id)) in
            outcomes.into_iter().enumerate()
        {
            let call = ToolAuditCall::new(
                &json!(index + 1),
                "read_file",
                Some("session-a"),
                "shared_bearer",
                None,
                200,
                &json!({"params":{"arguments":{"path":"private request"}}}),
            );
            let record = call.finish(workspace_id.map(str::to_string), &response);
            assert_eq!(record.outcome, expected_outcome);
            assert_eq!(record.error_category.as_deref(), expected_category);
            store.append(&record).unwrap();
        }

        let contents = fs::read_to_string(
            fs::read_dir(dir.path())
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        )
        .unwrap();
        assert!(!contents.contains("private request"));
        assert!(!contents.contains("private result"));
        assert!(!contents.contains("private policy detail"));
        assert!(!contents.contains("private runtime detail"));

        let page = store
            .list(&ToolAuditFilter::default(), DEFAULT_RETENTION_DAYS)
            .unwrap();
        assert_eq!(page.total, 4);
        assert_eq!(page.error_count, 3);
        assert!(page
            .records
            .iter()
            .any(|record| record.workspace_id.is_none()));
    }
}
