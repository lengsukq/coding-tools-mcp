use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use super::sanitize::{file_day, file_day_in_range, unix_time_ms, workspace_prefix};
use super::{ToolAuditFilter, ToolAuditPage, ToolAuditRecord, DAY_MS};

const MAX_PAGE_SIZE: usize = 200;
const PRUNE_INTERVAL_MS: u64 = 60 * 60 * 1000;

pub(super) struct ToolAuditStore {
    root: PathBuf,
    pub(super) last_write_error: Option<String>,
    last_prune_ms: u64,
}

impl ToolAuditStore {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            last_write_error: None,
            last_prune_ms: 0,
        }
    }

    pub(super) fn append(&mut self, record: &ToolAuditRecord) -> std::io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let path = self.event_path(record);
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        serde_json::to_writer(&mut file, record).map_err(std::io::Error::other)?;
        file.write_all(b"\n")?;
        file.flush()?;
        self.last_write_error = None;
        Ok(())
    }

    pub(super) fn list(
        &mut self,
        filter: &ToolAuditFilter,
        retention_days: u16,
    ) -> std::io::Result<ToolAuditPage> {
        let now = unix_time_ms();
        if self.prune_expired(now, retention_days).is_err() {
            self.last_write_error =
                Some("自动清理过期审计记录失败；请检查应用日志目录权限和磁盘空间。".into());
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

                if matches_filter(
                    &record,
                    filter,
                    session_filter.as_deref(),
                    tool_filter.as_deref(),
                    outcome_filter,
                    search.as_deref(),
                ) {
                    records.push(record);
                }
            }
        }
        Ok(records)
    }

    pub(super) fn clear(&mut self, workspace_id: Option<&str>) -> std::io::Result<()> {
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

    pub(super) fn prune_expired(&mut self, now_ms: u64, retention_days: u16) -> std::io::Result<()> {
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

    pub(super) fn prune_if_due(&mut self, now_ms: u64, retention_days: u16) {
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

fn contains_ignore_case(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if haystack.is_ascii() && needle_lower.is_ascii() {
        let needle = needle_lower.as_bytes();
        return haystack
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle));
    }
    haystack.to_lowercase().contains(needle_lower)
}

fn matches_filter(
    record: &ToolAuditRecord,
    filter: &ToolAuditFilter,
    session_filter: Option<&str>,
    tool_filter: Option<&str>,
    outcome_filter: Option<&str>,
    search: Option<&str>,
) -> bool {
    !(filter
        .from_ms
        .is_some_and(|from| record.timestamp_ms < from)
        || filter.to_ms.is_some_and(|to| record.timestamp_ms > to)
        || filter
            .workspace_id
            .as_deref()
            .is_some_and(|workspace| record.workspace_id.as_deref() != Some(workspace))
        || session_filter.is_some_and(|session| {
            !contains_ignore_case(record.session_id.as_deref().unwrap_or_default(), session)
        })
        || tool_filter.is_some_and(|tool| !contains_ignore_case(&record.tool_name, tool))
        || outcome_filter.is_some_and(|outcome| record.outcome != outcome)
        || search.is_some_and(|needle| {
            ![
                record.request_id.as_deref().unwrap_or_default(),
                record.session_id.as_deref().unwrap_or_default(),
                record.tool_name.as_str(),
                record.client_id.as_deref().unwrap_or_default(),
                record.execution_host.as_deref().unwrap_or_default(),
                record.source_peer.as_deref().unwrap_or_default(),
            ]
            .iter()
            .any(|value| contains_ignore_case(value, needle))
        }))
}
