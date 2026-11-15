use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(super) const REVIEW_DIR: &str = ".coding-tools/reviews";

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

fn valid_change_id(change_id: &str) -> bool {
    !change_id.is_empty()
        && change_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn read_change_set(path: &Path) -> Result<ChangeSet, String> {
    let body = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&body).map_err(|e| e.to_string())
}

fn write_change_set(path: &Path, review: &ChangeSet) -> Result<(), String> {
    let payload = serde_json::to_vec_pretty(review).map_err(|e| e.to_string())?;
    fs::write(path, payload).map_err(|e| e.to_string())
}

pub fn attach_session(
    workspace_root: &Path,
    change_id: &str,
    session_id: &str,
) -> Result<(), String> {
    let path = review_path(workspace_root, change_id);
    let mut review = read_change_set(&path)?;
    review.session_id = Some(session_id.to_string());
    write_change_set(&path, &review)
}

pub fn attach_operation(
    workspace_root: &Path,
    change_id: &str,
    operation_id: &str,
) -> Result<(), String> {
    let path = review_path(workspace_root, change_id);
    let mut review = read_change_set(&path)?;
    if !review.operation_ids.iter().any(|id| id == operation_id) {
        review.operation_ids.push(operation_id.to_string());
        write_change_set(&path, &review)?;
    }
    Ok(())
}

pub fn issue_token(workspace_root: &Path, change_id: &str) -> Result<ReviewLink, String> {
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
    if review.token_hashes.len() > 31 {
        review.token_hashes.drain(..review.token_hashes.len() - 31);
    }
    write_change_set(&path, &review)?;
    Ok(ReviewLink {
        change_id: change_id.to_string(),
        token,
    })
}

pub fn list_reviews(workspace_root: &Path) -> Vec<ChangeSet> {
    let Ok(entries) = fs::read_dir(workspace_root.join(REVIEW_DIR)) else {
        return Vec::new();
    };
    let mut reviews = entries
        .flatten()
        .filter_map(|entry| fs::read(entry.path()).ok())
        .filter_map(|body| serde_json::from_slice::<ChangeSet>(&body).ok())
        .collect::<Vec<_>>();
    reviews.sort_by_key(|review| std::cmp::Reverse(review.created_at));
    reviews
}

pub fn cleanup_expired(workspace_root: &Path) -> usize {
    let now = now_secs();
    let mut removed = 0;
    for review in list_reviews(workspace_root)
        .into_iter()
        .filter(|review| review.expires_at < now)
    {
        if fs::remove_file(review_path(workspace_root, &review.id)).is_ok() {
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
        .filter_map(|e| e.metadata().ok().map(|m| m.len()))
        .sum()
}

pub fn find_review(
    workspace_roots: impl IntoIterator<Item = PathBuf>,
    id: &str,
    token: &str,
) -> Result<ChangeSet, String> {
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// These inputs map directly to the persisted review record.
#[allow(clippy::too_many_arguments)]
pub(super) fn persist_change_set(
    workspace_root: &Path,
    change_id: &str,
    token_hash: String,
    workspace_key: String,
    scope: &str,
    session_id: Option<String>,
    operation_ids: Vec<String>,
    base_revision: Option<String>,
    summary: &str,
    files: Vec<ChangeFile>,
) -> Result<(), String> {
    let stats = ReviewStats {
        files: files.len(),
        additions: files.iter().map(|file| file.additions).sum(),
        deletions: files.iter().map(|file| file.deletions).sum(),
    };
    let created_at = now_secs();
    let change = ChangeSet {
        id: change_id.into(),
        token_hash,
        token_hashes: Vec::new(),
        workspace_key,
        session_id,
        scope: scope.into(),
        operation_ids,
        base_revision,
        created_at,
        expires_at: created_at + 30 * 24 * 60 * 60,
        summary: summary.into(),
        stats,
        files,
    };
    let dir = workspace_root.join(REVIEW_DIR);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_change_set(&dir.join(format!("{change_id}.json")), &change)
}
