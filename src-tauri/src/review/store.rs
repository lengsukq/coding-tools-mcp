use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::util::fs::{atomic_write_bytes, lock_file as acquire_file_lock, FileLock};
use crate::util::hash::sha256_hex;

pub(super) const REVIEW_DIR: &str = ".coding-tools/reviews";
const REVIEW_LOCK_FILE: &str = ".reviews.lock";
const MAX_REISSUED_TOKENS: usize = 31;
const REVIEW_TTL_SECS: u64 = 30 * 24 * 60 * 60;

fn default_review_scope() -> String {
    "operation".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewStats {
    pub files: usize,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeFile {
    pub path: String,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
    pub patch: String,
    pub redacted: bool,
}

impl ChangeFile {
    pub(super) fn redacted(path: &str, status: &str, reason: &str) -> Self {
        Self {
            path: path.into(),
            status: status.into(),
            additions: 0,
            deletions: 0,
            patch: reason.into(),
            redacted: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSet {
    pub id: String,
    /// Original bearer token hash. Kept for backwards compatibility and so the
    /// first URL remains valid for the full Review lifetime.
    pub token_hash: String,
    /// Hashes of subsequently issued links. Plaintext tokens are never stored.
    #[serde(default)]
    pub token_hashes: Vec<String>,
    pub workspace_key: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default = "default_review_scope")]
    pub scope: String,
    pub operation_ids: Vec<String>,
    pub base_revision: Option<String>,
    pub created_at: u64,
    pub expires_at: u64,
    pub summary: String,
    pub stats: ReviewStats,
    pub files: Vec<ChangeFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewLink {
    pub change_id: String,
    pub token: String,
}

pub fn review_path(root: &Path, id: &str) -> PathBuf {
    root.join(REVIEW_DIR).join(format!("{id}.json"))
}

fn lock_reviews(workspace_root: &Path) -> Result<FileLock, String> {
    let lock_path = workspace_root.join(REVIEW_DIR).join(REVIEW_LOCK_FILE);
    acquire_file_lock(&lock_path).map_err(|e| e.to_string())
}

fn is_review_json_file(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("json")
}

fn valid_change_id(change_id: &str) -> bool {
    crate::util::text::is_safe_identifier(change_id)
}

fn read_change_set(path: &Path) -> Result<ChangeSet, String> {
    let body = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&body).map_err(|e| e.to_string())
}

fn write_change_set(path: &Path, review: &ChangeSet) -> Result<(), String> {
    let payload = serde_json::to_vec_pretty(review).map_err(|e| e.to_string())?;
    atomic_write_bytes(path, &payload).map_err(|e| e.to_string())
}

pub fn attach_session(
    workspace_root: &Path,
    change_id: &str,
    session_id: &str,
) -> Result<(), String> {
    if !valid_change_id(change_id) {
        return Err("invalid review id".into());
    }
    let _lock = lock_reviews(workspace_root)?;
    let path = review_path(workspace_root, change_id);
    let mut review = read_change_set(&path)?;
    if review.session_id.as_deref() == Some(session_id) {
        return Ok(());
    }
    review.session_id = Some(session_id.to_string());
    write_change_set(&path, &review)
}

pub fn attach_operation(
    workspace_root: &Path,
    change_id: &str,
    operation_id: &str,
) -> Result<(), String> {
    if !valid_change_id(change_id) {
        return Err("invalid review id".into());
    }
    let _lock = lock_reviews(workspace_root)?;
    let path = review_path(workspace_root, change_id);
    let mut review = read_change_set(&path)?;
    if !review.operation_ids.iter().any(|id| id == operation_id) {
        review.operation_ids.push(operation_id.to_string());
        write_change_set(&path, &review)?;
    }
    Ok(())
}

pub fn issue_token(workspace_root: &Path, change_id: &str) -> Result<ReviewLink, String> {
    if !valid_change_id(change_id) {
        return Err("invalid review id".into());
    }
    let _lock = lock_reviews(workspace_root)?;
    let path = review_path(workspace_root, change_id);
    let mut review = read_change_set(&path)?;
    let (token, token_hash) = fresh_token();
    if !review
        .token_hashes
        .iter()
        .any(|existing| existing == &token_hash)
    {
        review.token_hashes.push(token_hash);
    }
    // Keep a bounded number of re-issued links without invalidating the
    // original URL that may already have been sent to Chat/mobile.
    if review.token_hashes.len() > MAX_REISSUED_TOKENS {
        review
            .token_hashes
            .drain(..review.token_hashes.len() - MAX_REISSUED_TOKENS);
    }
    write_change_set(&path, &review)?;
    Ok(ReviewLink {
        change_id: change_id.to_string(),
        token,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSetSummary {
    pub id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default = "default_review_scope")]
    pub scope: String,
    pub operation_ids: Vec<String>,
    pub created_at: u64,
    pub expires_at: u64,
    pub summary: String,
    pub stats: ReviewStats,
}

#[derive(Debug, Deserialize)]
struct ChangeSetExpiry {
    expires_at: u64,
}

pub(crate) fn review_summary(workspace_root: &Path, change_id: &str) -> Option<ChangeSetSummary> {
    if !valid_change_id(change_id) {
        return None;
    }
    let file = fs::File::open(review_path(workspace_root, change_id)).ok()?;
    serde_json::from_reader::<_, ChangeSetSummary>(BufReader::new(file)).ok()
}

pub fn list_review_summaries(workspace_root: &Path) -> Vec<ChangeSetSummary> {
    let Ok(entries) = fs::read_dir(workspace_root.join(REVIEW_DIR)) else {
        return Vec::new();
    };
    let mut reviews = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_review_json_file(path))
        .filter_map(|path| fs::File::open(path).ok())
        .filter_map(|file| {
            serde_json::from_reader::<_, ChangeSetSummary>(BufReader::new(file)).ok()
        })
        .collect::<Vec<_>>();
    reviews.sort_by_key(|review| std::cmp::Reverse(review.created_at));
    reviews
}

pub fn read_change_set_by_id(workspace_root: &Path, id: &str) -> Result<ChangeSet, String> {
    if !valid_change_id(id) {
        return Err("invalid review id".into());
    }
    read_change_set(&review_path(workspace_root, id))
}

pub fn cleanup_expired(workspace_root: &Path) -> usize {
    let Ok(entries) = fs::read_dir(workspace_root.join(REVIEW_DIR)) else {
        return 0;
    };
    let now = now_secs();
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_review_json_file(&path) {
            continue;
        }
        let Ok(file) = fs::File::open(&path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_reader::<_, ChangeSetExpiry>(BufReader::new(file)) else {
            continue;
        };
        if meta.expires_at < now && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

pub fn load_review(
    workspace_root: &Path,
    change_id: &str,
    token: &str,
) -> Result<ChangeSet, String> {
    if !valid_change_id(change_id) {
        return Err("invalid review id".into());
    }
    let body = fs::read(review_path(workspace_root, change_id))
        .map_err(|_| "review not found".to_string())?;
    let review: ChangeSet = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if review.expires_at < now_secs() {
        return Err("review expired".into());
    }
    let candidate = sha256_hex(token.as_bytes());
    if candidate != review.token_hash && !review.token_hashes.iter().any(|hash| hash == &candidate)
    {
        return Err("invalid review token".into());
    }
    Ok(review)
}

pub fn delete_review(workspace_root: &Path, change_id: &str) -> Result<(), String> {
    if !valid_change_id(change_id) {
        return Err("invalid review id".into());
    }
    fs::remove_file(review_path(workspace_root, change_id)).map_err(|e| e.to_string())
}

pub fn storage_bytes(workspace_root: &Path) -> u64 {
    fs::read_dir(workspace_root.join(REVIEW_DIR))
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| is_review_json_file(&entry.path()))
        .filter_map(|e| e.metadata().ok().map(|m| m.len()))
        .sum()
}

pub fn find_review(
    workspace_roots: impl IntoIterator<Item = PathBuf>,
    id: &str,
    token: &str,
) -> Result<ChangeSet, String> {
    if !valid_change_id(id) {
        return Err("invalid review id".into());
    }
    for root in workspace_roots {
        if review_path(&root, id).is_file() {
            return load_review(&root, id, token);
        }
    }
    Err("review not found".into())
}

pub(super) fn fresh_token() -> (String, String) {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let token_hash = sha256_hex(token.as_bytes());
    (token, token_hash)
}

pub(super) fn workspace_key(workspace_root: &Path) -> String {
    sha256_hex(workspace_root.to_string_lossy().as_bytes())
}

fn now_secs() -> u64 {
    crate::util::time::unix_now_secs()
}

pub(super) struct PersistChangeSetInput<'a> {
    pub(super) workspace_root: &'a Path,
    pub(super) change_id: &'a str,
    pub(super) token_hash: String,
    pub(super) workspace_key: String,
    pub(super) scope: &'a str,
    pub(super) session_id: Option<String>,
    pub(super) operation_ids: Vec<String>,
    pub(super) base_revision: Option<String>,
    pub(super) summary: &'a str,
    pub(super) files: Vec<ChangeFile>,
}

pub(super) fn persist_change_set(input: PersistChangeSetInput<'_>) -> Result<(), String> {
    if !valid_change_id(input.change_id) {
        return Err("invalid review id".into());
    }
    let stats = ReviewStats {
        files: input.files.len(),
        additions: input.files.iter().map(|file| file.additions).sum(),
        deletions: input.files.iter().map(|file| file.deletions).sum(),
    };
    let created_at = now_secs();
    let change = ChangeSet {
        id: input.change_id.into(),
        token_hash: input.token_hash,
        token_hashes: Vec::new(),
        workspace_key: input.workspace_key,
        session_id: input.session_id,
        scope: input.scope.into(),
        operation_ids: input.operation_ids,
        base_revision: input.base_revision,
        created_at,
        expires_at: created_at + REVIEW_TTL_SECS,
        summary: input.summary.into(),
        stats,
        files: input.files,
    };
    let _lock = lock_reviews(input.workspace_root)?;
    write_change_set(&review_path(input.workspace_root, input.change_id), &change)
}
