use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::tools::workspace::{relative_display, Workspace, WorkspaceError, WorkspaceResult};
use crate::util::fs::{atomic_write_bytes, lock_file as acquire_file_lock, FileLock};
use crate::util::hash::sha256_hex;

use super::markdown;
use super::model::{
    HistoryDocument, HistoryIndex, IndexEntry, ManifestEntry, MemoryManifest, MemoryReference,
    MemoryState, ScanReport,
};

pub const DEFAULT_HISTORY_DIR: &str = "docs/history-session";
const STATE_ITEM_LIMIT: usize = 12;
const STATE_TEXT_LIMIT: usize = 512;
const STATE_FOCUS_LIMIT: usize = 2_048;
const STATE_REFERENCE_LIMIT: usize = 8;

pub fn resolve_history_dir(
    workspace: &Workspace,
    workspace_root: Option<&str>,
    history_dir: Option<&str>,
) -> WorkspaceResult<PathBuf> {
    if let Some(requested_root) = workspace_root {
        let requested_path = Path::new(requested_root.trim());
        let candidate = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            workspace.root().join(requested_path)
        };
        let requested = candidate
            .canonicalize()
            .map_err(|_| WorkspaceError::invalid_argument("workspace_root does not exist"))?;
        if requested != workspace.root() {
            return Err(WorkspaceError::path_outside_workspace());
        }
    }

    let raw = history_dir.unwrap_or(DEFAULT_HISTORY_DIR).trim();
    if raw.is_empty() || workspace.reject_unsafe_text(raw).is_err() {
        return Err(WorkspaceError::path_outside_workspace());
    }
    let candidate = workspace
        .root()
        .join(raw.replace('/', std::path::MAIN_SEPARATOR_STR));
    ensure_safe_candidate(workspace, &candidate)?;
    if candidate.exists() && !candidate.is_dir() {
        return Err(WorkspaceError::not_a_directory(
            "history_dir must be a directory",
        ));
    }
    Ok(candidate)
}

fn ensure_safe_candidate(workspace: &Workspace, candidate: &Path) -> WorkspaceResult<()> {
    if candidate.exists() || candidate.is_symlink() {
        let resolved = candidate
            .canonicalize()
            .map_err(|_| WorkspaceError::path_outside_workspace())?;
        if !resolved.starts_with(workspace.root()) {
            return Err(WorkspaceError::path_outside_workspace());
        }
        return Ok(());
    }
    let mut ancestor = candidate.parent();
    while let Some(path) = ancestor {
        if path.exists() || path.is_symlink() {
            let resolved = path
                .canonicalize()
                .map_err(|_| WorkspaceError::path_outside_workspace())?;
            if !resolved.starts_with(workspace.root()) {
                return Err(WorkspaceError::path_outside_workspace());
            }
            return Ok(());
        }
        ancestor = path.parent();
    }
    Err(WorkspaceError::path_outside_workspace())
}

pub fn ensure_directory(path: &Path) -> WorkspaceResult<()> {
    fs::create_dir_all(path).map_err(|error| io_error("HISTORY_WRITE_FAILED", error, true))
}

pub fn lock_directory(path: &Path) -> WorkspaceResult<FileLock> {
    ensure_directory(path)?;
    let lock_path = path.join(".history.lock");
    acquire_file_lock(&lock_path).map_err(|error| io_error("HISTORY_LOCK_FAILED", error, true))
}

pub fn scan(workspace: &Workspace, history_dir: &Path) -> WorkspaceResult<ScanReport> {
    if !history_dir.exists() {
        return Ok(ScanReport::default());
    }
    ensure_safe_candidate(workspace, history_dir)?;
    let mut report = ScanReport::default();
    let entries =
        fs::read_dir(history_dir).map_err(|error| io_error("HISTORY_READ_FAILED", error, true))?;
    for entry in entries {
        let entry = entry.map_err(|error| io_error("HISTORY_READ_FAILED", error, true))?;
        let file_type = entry
            .file_type()
            .map_err(|error| io_error("HISTORY_READ_FAILED", error, true))?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(name.as_str(), "README.md" | "index.json" | ".history.lock")
            || name.starts_with(".history-tmp-")
            || name.starts_with(".tmp-")
        {
            continue;
        }
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            report.invalid_files.push(name);
            continue;
        };
        let is_markdown = path.extension().and_then(|value| value.to_str()) == Some("md");
        let number = stem.parse::<u64>().ok();
        if !is_markdown
            || number.is_none()
            || number == Some(0)
            || number.map(|value| value.to_string()) != Some(stem.to_string())
        {
            report.invalid_files.push(name);
            continue;
        }
        let number = number.expect("validated number");
        let bytes =
            fs::read(&path).map_err(|error| io_error("HISTORY_READ_FAILED", error, true))?;
        let content = String::from_utf8(bytes).map_err(|error| WorkspaceError::ToolDetails {
            code: "HISTORY_INVALID_UTF8",
            message: "History Markdown must be UTF-8.".into(),
            category: "validation",
            retryable: false,
            details: serde_json::json!({"file": name, "error": error.to_string()}),
        })?;
        if content.trim().is_empty() {
            report.empty_files.push(name.clone());
        }
        report.documents.push(HistoryDocument {
            number,
            path: relative_display(workspace.root(), &path),
            session_key: markdown::metadata(&content, "Session key"),
            created_at: markdown::metadata(&content, "Created"),
            updated_at: markdown::metadata(&content, "Updated"),
            content,
        });
    }
    report.documents.sort_by_key(|document| document.number);
    report.invalid_files.sort();
    report.empty_files.sort();
    report.numbers = report
        .documents
        .iter()
        .map(|document| document.number)
        .collect();
    if let Some(latest) = report.latest_number() {
        let present = report.numbers.iter().copied().collect::<BTreeSet<_>>();
        report.missing_numbers = (1..=latest)
            .filter(|number| !present.contains(number))
            .collect();
    }
    let mut keys = BTreeMap::<String, usize>::new();
    for key in report
        .documents
        .iter()
        .filter_map(|document| document.session_key.as_ref())
    {
        *keys.entry(key.clone()).or_default() += 1;
    }
    report.duplicate_session_keys = keys
        .into_iter()
        .filter_map(|(key, count)| (count > 1).then_some(key))
        .collect();
    Ok(report)
}

pub fn rebuild_index(report: &ScanReport) -> HistoryIndex {
    let duplicates = report
        .duplicate_session_keys
        .iter()
        .collect::<BTreeSet<_>>();
    let mut index = HistoryIndex {
        latest_number: report.latest_number().unwrap_or(0),
        ..HistoryIndex::default()
    };
    for document in &report.documents {
        let Some(session_key) = document.session_key.as_ref() else {
            continue;
        };
        if duplicates.contains(session_key) {
            continue;
        }
        index.sessions.insert(
            session_key.clone(),
            IndexEntry {
                number: document.number,
                path: document.path.clone(),
                created_at: document.created_at.clone().unwrap_or_default(),
                updated_at: document.updated_at.clone().unwrap_or_default(),
            },
        );
    }
    index
}

pub fn read_index(history_dir: &Path) -> WorkspaceResult<Option<HistoryIndex>> {
    read_json(
        &history_dir.join("index.json"),
        "HISTORY_INDEX_INVALID",
        "History index",
    )
}

pub fn write_index(history_dir: &Path, index: &HistoryIndex) -> WorkspaceResult<()> {
    write_json(&history_dir.join("index.json"), index, "history index")
}

pub fn memory_dir(history_dir: &Path) -> PathBuf {
    history_dir.join("memory")
}

pub fn read_manifest(history_dir: &Path) -> WorkspaceResult<Option<MemoryManifest>> {
    read_json(
        &memory_dir(history_dir).join("manifest.json"),
        "HISTORY_MANIFEST_INVALID",
        "History manifest",
    )
}

pub fn write_manifest(history_dir: &Path, manifest: &MemoryManifest) -> WorkspaceResult<()> {
    write_json(
        &memory_dir(history_dir).join("manifest.json"),
        manifest,
        "history manifest",
    )
}

pub fn read_state(history_dir: &Path) -> WorkspaceResult<Option<MemoryState>> {
    read_json(
        &memory_dir(history_dir).join("state.json"),
        "HISTORY_STATE_INVALID",
        "History state",
    )
}

pub fn write_state(history_dir: &Path, state: &MemoryState) -> WorkspaceResult<()> {
    write_json(
        &memory_dir(history_dir).join("state.json"),
        state,
        "history state",
    )
}

pub fn upsert_document(report: &mut ScanReport, document: HistoryDocument) {
    let file_name = format!("{}.md", document.number);
    if !document.content.trim().is_empty() {
        report.empty_files.retain(|name| name != &file_name);
    }
    match report
        .documents
        .binary_search_by_key(&document.number, |existing| existing.number)
    {
        Ok(index) => {
            report.documents[index] = document;
        }
        Err(index) => {
            let number = document.number;
            report.documents.insert(index, document);
            report.numbers.insert(index, number);
            report.missing_numbers.retain(|missing| *missing != number);
        }
    }
}

pub fn archive_revision(report: &ScanReport) -> String {
    let mut digest = Sha256::new();
    for document in &report.documents {
        digest.update(document.number.to_le_bytes());
        digest.update(sha256(document.content.as_bytes()).as_bytes());
    }
    format!("sha256:{:x}", digest.finalize())
}

pub fn build_manifest(report: &ScanReport) -> MemoryManifest {
    let entries = report
        .documents
        .iter()
        .map(|document| ManifestEntry {
            number: document.number,
            path: document.path.clone(),
            title: markdown::document_title(&document.content, document.number),
            created_at: document.created_at.clone().unwrap_or_default(),
            updated_at: document.updated_at.clone().unwrap_or_default(),
            bytes: document.content.len() as u64,
            sha256: sha256(document.content.as_bytes()),
            keywords: keywords(document),
        })
        .collect::<Vec<_>>();
    let mut digest = Sha256::new();
    for entry in &entries {
        digest.update(entry.number.to_le_bytes());
        digest.update(entry.sha256.as_bytes());
    }
    MemoryManifest {
        version: 2,
        archive_revision: format!("sha256:{:x}", digest.finalize()),
        entries,
    }
}

pub fn build_state(
    report: &ScanReport,
    manifest: &MemoryManifest,
    current_number: Option<u64>,
    timestamp: &str,
    state_revision: u64,
) -> MemoryState {
    let current_document = current_number.and_then(|number| {
        report
            .documents
            .iter()
            .find(|document| document.number == number)
    });
    let current_focus = current_document
        .and_then(|document| latest_user_focus(&document.content))
        .or_else(|| {
            current_document
                .map(|document| markdown::document_title(&document.content, document.number))
        })
        .unwrap_or_else(|| "尚未记录当前焦点".to_string());
    let mut recent_changes = Vec::new();
    let mut open_items = Vec::new();
    for document in report.documents.iter().rev() {
        let records = markdown::parse_checkpoint_records(&document.content);
        for record in latest_revisions(&records).into_iter().rev() {
            push_bounded(
                &mut recent_changes,
                record.files_changed.iter().map(String::as_str),
            );
            push_bounded(
                &mut recent_changes,
                record.decisions.iter().map(String::as_str),
            );
            push_bounded(
                &mut open_items,
                record.remaining_issues.iter().map(String::as_str),
            );
            push_bounded(
                &mut open_items,
                record.next_actions.iter().map(String::as_str),
            );
        }
    }
    let references = manifest
        .entries
        .iter()
        .rev()
        .filter(|entry| Some(entry.number) != current_number)
        .take(STATE_REFERENCE_LIMIT)
        .map(|entry| MemoryReference {
            number: entry.number,
            path: entry.path.clone(),
            reason: "最近历史档案；可按需读取原文".into(),
        })
        .collect();
    MemoryState {
        version: 2,
        state_revision,
        archive_revision: manifest.archive_revision.clone(),
        generated_at: timestamp.to_string(),
        current_session: current_number.and_then(|number| {
            manifest
                .entries
                .iter()
                .find(|entry| entry.number == number)
                .map(|entry| MemoryReference {
                    number: entry.number,
                    path: entry.path.clone(),
                    reason: "当前 ChatGPT 会话档案".into(),
                })
        }),
        current_focus: truncate_text(&current_focus, STATE_FOCUS_LIMIT),
        recent_changes,
        open_items,
        references,
    }
}

fn latest_user_focus(content: &str) -> Option<String> {
    markdown::parse_checkpoint_records(content)
        .into_iter()
        .max_by_key(|record| record.revision)
        .and_then(|record| {
            if record.raw_user_input.trim().is_empty() {
                (!record.user_intent.trim().is_empty()).then_some(record.user_intent)
            } else {
                Some(record.raw_user_input)
            }
        })
        .or_else(|| {
            markdown::parse_initial_input_records(content)
                .into_iter()
                .max_by_key(|record| record.revision)
                .map(|record| record.raw_user_input)
        })
}

fn latest_revisions(
    records: &[super::model::CheckpointRecord],
) -> Vec<&super::model::CheckpointRecord> {
    let mut latest = BTreeMap::new();
    for record in records {
        let should_replace = latest
            .get(record.turn_id.as_str())
            .map(|existing: &&super::model::CheckpointRecord| record.revision >= existing.revision)
            .unwrap_or(true);
        if should_replace {
            latest.insert(record.turn_id.as_str(), record);
        }
    }
    latest.into_values().collect()
}

fn push_bounded<'a>(target: &mut Vec<String>, values: impl Iterator<Item = &'a str>) {
    for value in values {
        let value = value.trim();
        if value.is_empty() || target.iter().any(|existing| existing == value) {
            continue;
        }
        if target.len() >= STATE_ITEM_LIMIT {
            return;
        }
        target.push(truncate_text(value, STATE_TEXT_LIMIT));
    }
}

fn keywords(document: &HistoryDocument) -> Vec<String> {
    let mut values = Vec::new();
    values.push(markdown::document_title(&document.content, document.number));
    values.extend(
        markdown::parse_initial_input_records(&document.content)
            .into_iter()
            .map(|record| record.raw_user_input),
    );
    values.extend(
        markdown::parse_checkpoint_records(&document.content)
            .into_iter()
            .flat_map(|record| [record.user_intent, record.raw_user_input]),
    );
    tokenize(&values.join(" ")).into_iter().take(32).collect()
}

pub fn tokenize(value: &str) -> Vec<String> {
    let mut tokens = value
        .split(|character: char| !character.is_alphanumeric())
        .map(|token| token.trim().to_lowercase())
        .filter(|token| token.chars().count() >= 2)
        .collect::<Vec<_>>();
    tokens.sort();
    tokens.dedup();
    tokens
}

pub fn truncate_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut text = value.chars().take(max_chars).collect::<String>();
    text.push_str("...");
    text
}

pub fn write_markdown(path: &Path, content: &str) -> WorkspaceResult<()> {
    atomic_write(path, content.as_bytes())
}

pub fn sha256(content: &[u8]) -> String {
    sha256_hex(content)
}

fn read_json<T>(path: &Path, invalid_code: &'static str, label: &str) -> WorkspaceResult<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(path).map_err(|error| io_error("HISTORY_READ_FAILED", error, true))?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|error| WorkspaceError::ToolDetails {
            code: invalid_code,
            message: format!("{label} is not valid JSON."),
            category: "validation",
            retryable: true,
            details: serde_json::json!({"error": error.to_string()}),
        })
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T, label: &str) -> WorkspaceResult<()> {
    let content =
        serde_json::to_vec_pretty(value).map_err(|error| WorkspaceError::ToolDetails {
            code: "HISTORY_WRITE_FAILED",
            message: format!("Unable to serialize {label}."),
            category: "internal",
            retryable: true,
            details: serde_json::json!({"error": error.to_string()}),
        })?;
    atomic_write(path, &content)
}

fn atomic_write(target: &Path, content: &[u8]) -> WorkspaceResult<()> {
    if target.parent().is_none() {
        return Err(WorkspaceError::invalid_argument(
            "History target has no parent",
        ));
    }
    atomic_write_bytes(target, content)
        .map_err(|error| io_error("HISTORY_WRITE_FAILED", error, true))
}

fn io_error(code: &'static str, error: io::Error, retryable: bool) -> WorkspaceError {
    WorkspaceError::ToolDetails {
        code,
        message: error.to_string(),
        category: "filesystem",
        retryable,
        details: serde_json::json!({"kind": format!("{:?}", error.kind())}),
    }
}
