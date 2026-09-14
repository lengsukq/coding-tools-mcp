use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::model::{BaselineEntry, ProjectBaseline};

pub(super) fn capture_baseline(root: &Path) -> ProjectBaseline {
    let mut entries = Vec::new();
    for item in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = item.path();
        if path == root || should_skip(path, root) || !item.file_type().is_file() {
            continue;
        }
        let Ok(bytes) = fs::read(path) else { continue };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        entries.push(BaselineEntry {
            path: relative,
            exists: true,
            is_binary: bytes.contains(&0),
            sha256: format!("{:x}", hasher.finalize()),
            bytes: bytes.len() as u64,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    let mut fingerprint = Sha256::new();
    for entry in &entries {
        fingerprint.update(entry.path.as_bytes());
        fingerprint.update(entry.sha256.as_bytes());
        fingerprint.update(entry.bytes.to_le_bytes());
    }
    ProjectBaseline {
        branch: git_value(root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        head: git_value(root, &["rev-parse", "HEAD"]),
        worktree_fingerprint: format!("{:x}", fingerprint.finalize()),
        entries,
        captured_at: timestamp(),
    }
}

fn should_skip(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    if relative.starts_with(Path::new("docs").join("history-session")) {
        return true;
    }
    relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .any(|name| {
            matches!(
                name,
                ".git"
                    | ".coding-tools"
                    | ".mcp-probe-kit"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | ".svelte-kit"
            )
        })
}

fn git_value(root: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
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
