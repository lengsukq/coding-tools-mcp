use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};
use uuid::Uuid;

const REVIEW_DIR: &str = ".coding-tools/reviews";
const MAX_FILE_BYTES: usize = 512 * 1024;
const MAX_WORKSPACE_REVIEW_FILES: usize = 500;

fn default_review_scope() -> String { "operation".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewStats {
    pub files: usize,
    pub additions: usize,
    pub deletions: usize,
}

pub fn direct_git_repositories(workspace_root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(workspace_root) else { return Vec::new() };
    let mut repositories = entries.flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() { return None; }
            let name = entry.file_name().to_string_lossy().to_string();
            if review_path_excluded(&name) { return None; }
            let path = entry.path();
            is_git_repository_root(&path).then_some((name, path))
        })
        .collect::<Vec<_>>();
    repositories.sort_by(|left, right| left.0.cmp(&right.0));
    repositories
}

pub fn git_changed_file_count(repo_root: &Path, ignored_prefixes: &[String]) -> usize {
    git_repo_changes(repo_root, ignored_prefixes).map(|changes| changes.len()).unwrap_or(0)
}

fn git_repo_changes(repo_root: &Path, ignored_prefixes: &[String]) -> Result<Vec<(String, String, String)>, String> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-status", "-z", "-M", "HEAD", "--", "."])
        .current_dir(repo_root)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let fields = output.stdout.split(|byte| *byte == 0).filter(|part| !part.is_empty()).collect::<Vec<_>>();
    let mut changes = Vec::<(String, String, String)>::new();
    let mut i = 0;
    while i < fields.len() {
        let status = String::from_utf8_lossy(fields[i]).to_string();
        i += 1;
        if status.starts_with('R') || status.starts_with('C') {
            if i + 1 >= fields.len() { break; }
            let old = String::from_utf8_lossy(fields[i]).to_string();
            let new = String::from_utf8_lossy(fields[i + 1]).to_string();
            i += 2;
            changes.push((status, old, new));
        } else {
            if i >= fields.len() { break; }
            let path = String::from_utf8_lossy(fields[i]).to_string();
            i += 1;
            changes.push((status, path.clone(), path));
        }
    }
    let untracked = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| e.to_string())?;
    if untracked.status.success() {
        for raw in untracked.stdout.split(|byte| *byte == 0).filter(|part| !part.is_empty()) {
            let path = String::from_utf8_lossy(raw).to_string();
            changes.push(("A".into(), path.clone(), path));
        }
    }
    Ok(changes.into_iter().filter(|(_, old_path, new_path)| {
        !review_path_excluded(old_path)
            && !review_path_excluded(new_path)
            && !ignored_prefixes.iter().any(|prefix| {
                old_path == prefix || old_path.starts_with(&format!("{prefix}/"))
                    || new_path == prefix || new_path.starts_with(&format!("{prefix}/"))
            })
    }).collect())
}

fn prefixed_review_path(prefix: &str, path: &str) -> String {
    if prefix.is_empty() { path.to_string() } else { format!("{prefix}/{path}") }
}

fn is_git_repository_root(path: &Path) -> bool {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output();
    let Ok(output) = output else { return false };
    if !output.status.success() { return false; }
    let reported = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
    match (reported.canonicalize(), path.canonicalize()) {
        (Ok(reported), Ok(actual)) => reported == actual,
        _ => false,
    }
}
pub fn attach_session(workspace_root: &Path, change_id: &str, session_id: &str) -> Result<(), String> {
    let path = review_path(workspace_root, change_id); let body = fs::read(&path).map_err(|e| e.to_string())?;
    let mut review: ChangeSet = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    review.session_id = Some(session_id.to_string());
    fs::write(path, serde_json::to_vec_pretty(&review).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
pub fn issue_token(workspace_root: &Path, change_id: &str) -> Result<ReviewLink, String> {
    let path = review_path(workspace_root, change_id); let body = fs::read(&path).map_err(|e| e.to_string())?;
    let mut review: ChangeSet = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut hasher = Sha256::new(); hasher.update(token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());
    if !review.token_hashes.iter().any(|existing| existing == &token_hash) {
        review.token_hashes.push(token_hash);
    }
    // Keep a bounded number of re-issued links without invalidating the
    // original URL that may already have been sent to Chat/mobile.
    if review.token_hashes.len() > 31 {
        review.token_hashes.drain(..review.token_hashes.len() - 31);
    }
    fs::write(path, serde_json::to_vec_pretty(&review).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(ReviewLink { change_id: change_id.to_string(), token })
}

pub fn snapshot_text_files(root: &Path) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for entry in walkdir::WalkDir::new(root).into_iter().filter_map(Result::ok).filter(|e| e.file_type().is_file()) {
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        let path = rel.to_string_lossy().replace('\\', "/");
        if review_path_excluded(&path) || sensitive_path(&path) { continue; }
        let Ok(meta) = entry.metadata() else { continue }; if meta.len() as usize > MAX_FILE_BYTES { continue; }
        if let Ok(bytes) = fs::read(entry.path()) { if !bytes.contains(&0) { if let Ok(text) = String::from_utf8(bytes) { out.insert(path, text); } } }
    }
    out
}

pub fn create_snapshot_review(root: &Path, change_id: &str, operation_id: Option<&str>, summary: &str, before: &std::collections::BTreeMap<String,String>, after: &std::collections::BTreeMap<String,String>) -> Result<Option<ReviewLink>, String> {
    let paths = before.keys().chain(after.keys()).cloned().collect::<std::collections::BTreeSet<_>>();
    let affected = paths.iter().filter_map(|path| {
        let old = before.get(path).cloned().unwrap_or_default(); let new = after.get(path).cloned().unwrap_or_default();
        if old == new { return None }
        let status = if !before.contains_key(path) { "add" } else if !after.contains_key(path) { "delete" } else { "update" };
        Some((path.clone(), status.to_string(), old, new))
    }).collect::<Vec<_>>();
    if affected.is_empty() { return Ok(None) }
    create_patch_review(root, change_id, operation_id, summary, &affected).map(Some)
}

pub fn attach_operation(workspace_root: &Path, change_id: &str, operation_id: &str) -> Result<(), String> {
    let path = review_path(workspace_root, change_id);
    let body = fs::read(&path).map_err(|e| e.to_string())?;
    let mut review: ChangeSet = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if !review.operation_ids.iter().any(|id| id == operation_id) {
        review.operation_ids.push(operation_id.to_string());
        fs::write(path, serde_json::to_vec_pretty(&review).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn list_reviews(workspace_root: &Path) -> Vec<ChangeSet> {
    let Ok(entries) = fs::read_dir(workspace_root.join(REVIEW_DIR)) else { return Vec::new() };
    let mut reviews = entries.flatten().filter_map(|entry| fs::read(entry.path()).ok())
        .filter_map(|body| serde_json::from_slice::<ChangeSet>(&body).ok()).collect::<Vec<_>>();
    reviews.sort_by_key(|review| std::cmp::Reverse(review.created_at));
    reviews
}

pub fn cleanup_expired(workspace_root: &Path) -> usize {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let mut removed = 0;
    for review in list_reviews(workspace_root).into_iter().filter(|review| review.expires_at < now) {
        if fs::remove_file(review_path(workspace_root, &review.id)).is_ok() { removed += 1; }
    }
    removed
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

pub fn create_patch_review(
    workspace_root: &Path,
    change_id: &str,
    operation_id: Option<&str>,
    summary: &str,
    affected: &[(String, String, String, String)],
) -> Result<ReviewLink, String> {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());
    let workspace_key = {
        let mut h = Sha256::new();
        h.update(workspace_root.to_string_lossy().as_bytes());
        format!("{:x}", h.finalize())
    };
    let files = affected.iter().map(|(path, status, before, after)| {
        change_file_from_bytes(path, status, before.as_bytes(), after.as_bytes())
    }).collect::<Vec<_>>();
    persist_change_set(
        workspace_root,
        change_id,
        token_hash,
        workspace_key,
        "operation",
        None,
        operation_id.into_iter().map(str::to_string).collect(),
        git_head(workspace_root),
        summary,
        files,
    )?;
    Ok(ReviewLink { change_id: change_id.into(), token })
}

pub fn create_workspace_review(workspace_root: &Path, summary: &str) -> Result<Option<ReviewLink>, String> {
    let mut repositories = Vec::<(String, PathBuf)>::new();
    if is_git_repository_root(workspace_root) {
        repositories.push((String::new(), workspace_root.to_path_buf()));
    }
    repositories.extend(direct_git_repositories(workspace_root));
    if repositories.is_empty() {
        return Err("workspace diff requires a Git repository or a first-level Git repository".into());
    }

    let nested_repo_prefixes = direct_git_repositories(workspace_root)
        .into_iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    let mut files = Vec::new();
    let mut base_revisions = Vec::new();
    for (prefix, repo_root) in repositories {
        let Some(head) = git_head(&repo_root) else { continue };
        base_revisions.push(if prefix.is_empty() { head.clone() } else { format!("{prefix}:{head}") });
        let ignored = if repo_root == workspace_root { nested_repo_prefixes.as_slice() } else { &[] };
        let changes = git_repo_changes(&repo_root, ignored)?;
        for (status, old_path, new_path) in changes {
            if files.len() >= MAX_WORKSPACE_REVIEW_FILES {
                return Err(format!("workspace review has more than {MAX_WORKSPACE_REVIEW_FILES} changed files"));
            }
            let code = status.chars().next().unwrap_or('M');
            let before = if code == 'A' { Vec::new() } else { git_blob_at_head(&repo_root, &old_path) };
            let after = if code == 'D' { Vec::new() } else { fs::read(repo_root.join(&new_path)).unwrap_or_default() };
            let display_old = prefixed_review_path(&prefix, &old_path);
            let display_new = prefixed_review_path(&prefix, &new_path);
            let (display_path, file_status) = match code {
                'A' => (display_new, "add"),
                'D' => (display_old, "delete"),
                'R' | 'C' => (format!("{display_old} → {display_new}"), "rename"),
                _ => (display_new, "update"),
            };
            files.push(change_file_from_bytes(&display_path, file_status, &before, &after));
        }
    }
    if files.is_empty() { return Ok(None); }

    let change_id = Uuid::new_v4().simple().to_string();
    let (token, token_hash) = fresh_token();
    persist_change_set(
        workspace_root,
        &change_id,
        token_hash,
        workspace_key(workspace_root),
        "workspace",
        None,
        Vec::new(),
        Some(base_revisions.join(" | ")),
        summary,
        files,
    )?;
    Ok(Some(ReviewLink { change_id, token }))
}

pub fn create_session_review(workspace_root: &Path, session_id: &str, summary: &str) -> Result<Option<ReviewLink>, String> {
    use std::collections::BTreeMap;
    let mut reviews = list_reviews(workspace_root).into_iter()
        .filter(|review| review.scope == "operation" && review.session_id.as_deref() == Some(session_id))
        .collect::<Vec<_>>();
    reviews.sort_by_key(|review| review.created_at);
    if reviews.is_empty() { return Ok(None); }

    let mut by_path = BTreeMap::<String, ChangeFile>::new();
    let mut operations = Vec::new();
    for review in reviews {
        operations.extend(review.operation_ids);
        for file in review.files {
            by_path.entry(file.path.clone()).and_modify(|merged| {
                if !merged.patch.is_empty() && !file.patch.is_empty() { merged.patch.push('\n'); }
                merged.patch.push_str(&file.patch);
                merged.additions += file.additions;
                merged.deletions += file.deletions;
                merged.redacted |= file.redacted;
                merged.status = file.status.clone();
            }).or_insert(file);
        }
    }
    let change_id = Uuid::new_v4().simple().to_string();
    let (token, token_hash) = fresh_token();
    persist_change_set(
        workspace_root,
        &change_id,
        token_hash,
        workspace_key(workspace_root),
        "session",
        Some(session_id.to_string()),
        operations,
        git_head(workspace_root),
        summary,
        by_path.into_values().collect(),
    )?;
    Ok(Some(ReviewLink { change_id, token }))
}

fn fresh_token() -> (String, String) {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());
    (token, token_hash)
}

fn workspace_key(workspace_root: &Path) -> String {
    let mut h = Sha256::new();
    h.update(workspace_root.to_string_lossy().as_bytes());
    format!("{:x}", h.finalize())
}

fn persist_change_set(
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
    let created_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
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
    fs::write(dir.join(format!("{change_id}.json")), serde_json::to_vec_pretty(&change).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn change_file_from_bytes(path: &str, status: &str, before: &[u8], after: &[u8]) -> ChangeFile {
    if sensitive_path(path) {
        return ChangeFile { path: path.into(), status: status.into(), additions: 0, deletions: 0, patch: "Sensitive file content hidden".into(), redacted: true };
    }
    if before.len() > MAX_FILE_BYTES || after.len() > MAX_FILE_BYTES {
        return ChangeFile { path: path.into(), status: status.into(), additions: 0, deletions: 0, patch: "File too large to render".into(), redacted: true };
    }
    if before.contains(&0) || after.contains(&0) {
        return ChangeFile { path: path.into(), status: status.into(), additions: 0, deletions: 0, patch: "Binary file — content not rendered".into(), redacted: true };
    }
    let Ok(before) = std::str::from_utf8(before) else {
        return ChangeFile { path: path.into(), status: status.into(), additions: 0, deletions: 0, patch: "Binary file — content not rendered".into(), redacted: true };
    };
    let Ok(after) = std::str::from_utf8(after) else {
        return ChangeFile { path: path.into(), status: status.into(), additions: 0, deletions: 0, patch: "Binary file — content not rendered".into(), redacted: true };
    };
    let patch = unified_patch(path, before, after, false);
    let additions = patch.lines().filter(|line| line.starts_with('+') && !line.starts_with("+++")).count();
    let deletions = patch.lines().filter(|line| line.starts_with('-') && !line.starts_with("---")).count();
    ChangeFile { path: path.into(), status: status.into(), additions, deletions, patch, redacted: false }
}

fn git_blob_at_head(root: &Path, path: &str) -> Vec<u8> {
    std::process::Command::new("git")
        .args(["show", &format!("HEAD:{path}")])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| output.stdout)
        .unwrap_or_default()
}

fn review_path_excluded(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.split('/').any(|part| matches!(
        part,
        ".git" | ".coding-tools" | ".local-build" | ".scratch" | "node_modules" | "target" | "build" | "dist" | ".venv"
    ))
}

pub fn load_review(workspace_root: &Path, change_id: &str, token: &str) -> Result<ChangeSet, String> {
    if !change_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("invalid review id".into());
    }
    let body = fs::read(workspace_root.join(REVIEW_DIR).join(format!("{change_id}.json"))).map_err(|_| "review not found".to_string())?;
    let review: ChangeSet = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if review.expires_at < now { return Err("review expired".into()); }
    let mut h = Sha256::new(); h.update(token.as_bytes());
    let candidate = format!("{:x}", h.finalize());
    if candidate != review.token_hash && !review.token_hashes.iter().any(|hash| hash == &candidate) {
        return Err("invalid review token".into());
    }
    Ok(review)
}

fn sensitive_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.split('/').any(|part| part == ".env" || part.starts_with(".env.") || part.contains("credential") || part.contains("secret"))
        || lower.ends_with(".pem") || lower.ends_with(".key") || lower.ends_with("id_rsa") || lower.ends_with("id_ed25519")
}

fn unified_patch(path: &str, before: &str, after: &str, ignore_whitespace: bool) -> String {
    if before == after { return String::new(); }
    let (old, new);
    let (before, after) = if ignore_whitespace {
        old = before.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n");
        new = after.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n");
        (old.as_str(), new.as_str())
    } else { (before, after) };
    let diff = TextDiff::from_lines(before, after);
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    for group in diff.grouped_ops(3) {
        let first = group.first().unwrap(); let last = group.last().unwrap();
        out.push_str(&format!("@@ -{},{} +{},{} @@\n", first.old_range().start + 1, last.old_range().end - first.old_range().start, first.new_range().start + 1, last.new_range().end - first.new_range().start));
        for op in group { for change in diff.iter_changes(&op) {
            out.push(match change.tag() { ChangeTag::Delete => '-', ChangeTag::Insert => '+', ChangeTag::Equal => ' ' });
            out.push_str(change.value()); if !change.value().ends_with('\n') { out.push('\n'); }
        }}
    }
    out
}

pub fn delete_review(workspace_root: &Path, change_id: &str) -> Result<(), String> {
    if !change_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { return Err("invalid review id".into()); }
    fs::remove_file(review_path(workspace_root, change_id)).map_err(|e| e.to_string())
}

pub fn storage_bytes(workspace_root: &Path) -> u64 {
    fs::read_dir(workspace_root.join(REVIEW_DIR)).ok().into_iter().flatten().flatten()
        .filter_map(|e| e.metadata().ok().map(|m| m.len())).sum()
}

fn git_head(root: &Path) -> Option<String> {
    std::process::Command::new("git").args(["rev-parse", "HEAD"]).current_dir(root).output().ok()
        .filter(|out| out.status.success()).map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn review_path(root: &Path, id: &str) -> PathBuf { root.join(REVIEW_DIR).join(format!("{id}.json")) }

pub fn find_review(workspace_roots: impl IntoIterator<Item = PathBuf>, id: &str, token: &str) -> Result<ChangeSet, String> {
    for root in workspace_roots {
        if review_path(&root, id).is_file() { return load_review(&root, id, token); }
    }
    Err("review not found".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sensitive_files_are_detected() {
        assert!(sensitive_path(".env"));
        assert!(sensitive_path("config/client.key"));
        assert!(!sensitive_path("src/main.rs"));
    }
    #[test]
    fn review_id_rejects_traversal() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_review(dir.path(), "../secret", "x").is_err());
    }
    #[test]
    fn patch_contains_only_changed_hunks_with_context() {
        let before = "a\nb\nc\nd\ne\nf\ng\nh\ni\n";
        let after = "a\nb\nc\nd\nE\nf\ng\nh\ni\n";
        let patch = unified_patch("x.txt", before, after, false);
        assert!(patch.contains("-e\n+E\n"));
        assert!(!patch.contains("-a\n"));
    }
    #[test]
    fn reissuing_token_does_not_invalidate_existing_review_url() {
        let dir = tempfile::tempdir().unwrap();
        let original = create_patch_review(
            dir.path(),
            "change-1",
            None,
            "test",
            &[("src/a.txt".into(), "update".into(), "before\n".into(), "after\n".into())],
        ).unwrap();
        assert!(load_review(dir.path(), "change-1", &original.token).is_ok());

        let second = issue_token(dir.path(), "change-1").unwrap();
        assert!(load_review(dir.path(), "change-1", &original.token).is_ok());
        assert!(load_review(dir.path(), "change-1", &second.token).is_ok());
    }
    #[test]
    fn workspace_review_freezes_tracked_and_untracked_changes_vs_head() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git").args(args).current_dir(dir.path()).output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        };
        git(&["init"]);
        git(&["config", "user.email", "review@example.test"]);
        git(&["config", "user.name", "Review Test"]);
        fs::write(dir.path().join("tracked.txt"), "before\n").unwrap();
        git(&["add", "tracked.txt"]);
        git(&["commit", "-m", "baseline"]);
        fs::write(dir.path().join("tracked.txt"), "after\n").unwrap();
        fs::write(dir.path().join("new.txt"), "new\n").unwrap();
        fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
        fs::write(dir.path().join("node_modules/pkg/generated.js"), "ignored\n").unwrap();

        let link = create_workspace_review(dir.path(), "workspace").unwrap().unwrap();
        let review = load_review(dir.path(), &link.change_id, &link.token).unwrap();
        assert_eq!(review.scope, "workspace");
        assert_eq!(review.stats.files, 2);
        assert!(review.files.iter().any(|file| file.path == "tracked.txt" && file.patch.contains("-before") && file.patch.contains("+after")));
        assert!(review.files.iter().any(|file| file.path == "new.txt" && file.status == "add"));
        assert!(!review.files.iter().any(|file| file.path.contains("node_modules")));
    }

    #[test]
    fn session_review_aggregates_operation_reviews_by_file() {
        let dir = tempfile::tempdir().unwrap();
        let first = create_patch_review(
            dir.path(), "one", Some("op-1"), "one",
            &[("src/a.txt".into(), "update".into(), "a\n".into(), "b\n".into())],
        ).unwrap();
        attach_session(dir.path(), &first.change_id, "chat-1").unwrap();
        let second = create_patch_review(
            dir.path(), "two", Some("op-2"), "two",
            &[("src/a.txt".into(), "update".into(), "b\n".into(), "c\n".into())],
        ).unwrap();
        attach_session(dir.path(), &second.change_id, "chat-1").unwrap();

        let aggregate = create_session_review(dir.path(), "chat-1", "session").unwrap().unwrap();
        let review = load_review(dir.path(), &aggregate.change_id, &aggregate.token).unwrap();
        assert_eq!(review.scope, "session");
        assert_eq!(review.session_id.as_deref(), Some("chat-1"));
        assert_eq!(review.stats.files, 1);
        assert_eq!(review.operation_ids.len(), 2);
        assert!(review.files[0].patch.contains("-a"));
        assert!(review.files[0].patch.contains("+c"));
    }

    #[test]
    fn workspace_review_detects_first_level_git_repositories() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("service-a");
        fs::create_dir_all(&child).unwrap();
        let git = |cwd: &Path, args: &[&str]| {
            let output = std::process::Command::new("git").args(args).current_dir(cwd).output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        };
        git(&child, &["init"]);
        git(&child, &["config", "user.email", "review@example.test"]);
        git(&child, &["config", "user.name", "Review Test"]);
        fs::write(child.join("index.txt"), "before\n").unwrap();
        git(&child, &["add", "index.txt"]);
        git(&child, &["commit", "-m", "baseline"]);
        fs::write(child.join("index.txt"), "after\n").unwrap();

        let repositories = direct_git_repositories(dir.path());
        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].0, "service-a");

        let link = create_workspace_review(dir.path(), "workspace").unwrap().unwrap();
        let review = load_review(dir.path(), &link.change_id, &link.token).unwrap();
        assert_eq!(review.stats.files, 1);
        assert_eq!(review.files[0].path, "service-a/index.txt");
        assert!(review.files[0].patch.contains("-before"));
        assert!(review.files[0].patch.contains("+after"));
    }
}
