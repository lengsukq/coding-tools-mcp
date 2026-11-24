use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Component, Path};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use walkdir::{DirEntry, WalkDir};

use crate::platform::background_command;
use crate::tools::io_util::is_default_excluded_os;

use super::model::{BaselineEntry, ProjectBaseline};

const MAX_BASELINE_FILE_BYTES: u64 = 16 * 1024 * 1024;
const BINARY_SNIFF_BYTES: usize = 8 * 1024;
const STREAM_READ_BUFFER_BYTES: usize = 64 * 1024;

#[cfg(test)]
thread_local! {
    static BASELINE_CAPTURE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn baseline_capture_calls() -> usize {
    BASELINE_CAPTURE_CALLS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn reset_baseline_capture_calls() {
    BASELINE_CAPTURE_CALLS.with(|calls| calls.set(0));
}

pub(super) fn capture_baseline(root: &Path) -> ProjectBaseline {
    #[cfg(test)]
    BASELINE_CAPTURE_CALLS.with(|calls| calls.set(calls.get() + 1));

    let mut entries = Vec::new();
    let mut read_buf = vec![0u8; STREAM_READ_BUFFER_BYTES];
    for item in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| should_visit_entry(entry, root))
        .filter_map(Result::ok)
    {
        if item.depth() == 0 || !item.file_type().is_file() {
            continue;
        }
        let path = item.path();
        let Some((sha256, bytes, is_binary)) = hash_baseline_file(path, &mut read_buf) else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        entries.push(BaselineEntry {
            path: relative,
            exists: true,
            is_binary,
            sha256,
            bytes,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    let mut fingerprint = Sha256::new();
    for entry in &entries {
        fingerprint.update(entry.path.as_bytes());
        fingerprint.update(entry.sha256.as_bytes());
        fingerprint.update(entry.bytes.to_le_bytes());
    }
    let (branch, head) = git_branch_and_head(root);
    ProjectBaseline {
        branch,
        head,
        worktree_fingerprint: format!("{:x}", fingerprint.finalize()),
        entries,
        captured_at: timestamp(),
    }
}

pub(super) fn git_branch_and_head(root: &Path) -> (Option<String>, Option<String>) {
    let branch = git_value(root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    if branch.is_none() {
        return (None, None);
    }
    let head = git_value(root, &["rev-parse", "HEAD"]);
    (branch, head)
}

fn should_visit_entry(entry: &DirEntry, root: &Path) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name();
    if is_default_excluded_os(name) || is_harness_excluded_os(name) {
        return false;
    }
    if let Ok(relative) = entry.path().strip_prefix(root) {
        if is_managed_history_path(relative) {
            return false;
        }
    }
    true
}

fn is_harness_excluded_os(name: &OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(
            ".coding-tools"
                | ".mcp-probe-kit"
                | ".svelte-kit"
                | "Library"
                | "OneDrive"
                | "OneDriveTemp"
                | "AppData"
                | "Windows"
                | "Program Files"
                | "Program Files (x86)"
                | "ProgramData"
                | "$Recycle.Bin"
                | "System Volume Information"
        )
    )
}

fn is_managed_history_path(relative: &Path) -> bool {
    let mut components = relative.components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(first)), Some(Component::Normal(second)))
            if first == OsStr::new("docs") && second == OsStr::new("history-session")
    )
}

fn hash_baseline_file(path: &Path, buf: &mut [u8]) -> Option<(String, u64, bool)> {
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if metadata.len() > MAX_BASELINE_FILE_BYTES {
        return None;
    }

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut total_bytes: u64 = 0;
    let mut is_binary = false;
    let mut sniffed: usize = 0;

    loop {
        let read = reader.read(buf).ok()?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(read as u64);
        if total_bytes > MAX_BASELINE_FILE_BYTES {
            return None;
        }
        if !is_binary && sniffed < BINARY_SNIFF_BYTES {
            let sniff_len = read.min(BINARY_SNIFF_BYTES - sniffed);
            if buf[..sniff_len].contains(&0) {
                is_binary = true;
            }
            sniffed += sniff_len;
        }
        hasher.update(&buf[..read]);
    }

    Some((format!("{:x}", hasher.finalize()), total_bytes, is_binary))
}

fn git_value(root: &Path, args: &[&str]) -> Option<String> {
    let output = background_command("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub(super) fn workspace_id(root: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())[..32].to_string()
}

pub(super) fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".into())
}
