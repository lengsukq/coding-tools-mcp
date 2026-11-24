use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::AppResult;

pub const DEFAULT_EXCLUDED_NAMES: &[&str] = &[
    ".git",
    ".reference",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "__pycache__",
    ".cache",
    ".next",
    ".turbo",
    "coverage",
];

/// Zero-allocation check against `DEFAULT_EXCLUDED_NAMES` for directory pruning.
pub fn is_default_excluded_os(name: &OsStr) -> bool {
    let Some(text) = name.to_str() else {
        return false;
    };
    DEFAULT_EXCLUDED_NAMES.contains(&text)
}

/// Zero-allocation check for a single path component during `WalkDir::filter_entry`.
pub fn is_ignored_os_component(name: &OsStr, include_hidden: bool, include_ignored: bool) -> bool {
    if !include_hidden && is_hidden_os_component(name) {
        return true;
    }
    if !include_ignored && is_default_excluded_os(name) {
        return true;
    }
    false
}

fn is_hidden_os_component(name: &OsStr) -> bool {
    if let Some(text) = name.to_str() {
        return text.starts_with('.') && text != ".";
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let bytes = name.as_bytes();
        bytes.starts_with(b".") && bytes != b"."
    }
    #[cfg(not(unix))]
    {
        let text = name.to_string_lossy();
        text.starts_with('.') && text != "."
    }
}

/// Expand leading `~`, `~/`, or `~\` to the current user's home directory.
pub fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(value));
    }
    if let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    Path::new(value).to_path_buf()
}

/// Exclusive advisory file lock released automatically on drop.
pub struct FileLock {
    file: File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Acquire an exclusive lock on `lock_path`, creating parent directories as needed.
pub fn lock_file(lock_path: &Path) -> io::Result<FileLock> {
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    file.lock()?;
    Ok(FileLock { file })
}

/// Atomically replace `target` using a UUID sibling temp file, fsync, and OS replace.
pub fn atomic_write_bytes(target: &Path, content: &[u8]) -> io::Result<()> {
    let parent = target.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "atomic write target has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;

    let stem = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temp_path = parent.join(format!(".{stem}.tmp-{}", uuid::Uuid::new_v4()));

    let write_result = (|| -> io::Result<()> {
        {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp_path)?;
            file.write_all(content)?;
            file.sync_all()?;
        }
        // Close the temp handle before replacing so Windows MoveFileExW never
        // encounters a sharing violation on the source path.
        atomic_replace(&temp_path, target)?;
        #[cfg(unix)]
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

/// Atomically write pretty-printed JSON with a trailing newline.
pub fn atomic_write_json<T: Serialize>(target: &Path, value: &T) -> AppResult<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    atomic_write_bytes(target, text.as_bytes())?;
    Ok(())
}

#[cfg(not(windows))]
pub fn atomic_replace(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
pub fn atomic_replace(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide: Vec<u16> = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let target_wide: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR(source_wide.as_ptr()),
            PCWSTR(target_wide.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|error| io::Error::other(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_json_replaces_existing_file_without_temp_leftovers() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("state.json");
        fs::write(&path, "corrupt").expect("seed file");

        atomic_write_json(&path, &serde_json::json!({ "ok": true })).expect("atomic write");
        let content = fs::read_to_string(&path).expect("read written json");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid json");
        assert_eq!(parsed["ok"], serde_json::json!(true));
        assert!(content.ends_with('\n'));
        assert_eq!(fs::read_dir(dir.path()).expect("list dir").count(), 1);
    }

    #[test]
    fn lock_file_creates_lock_and_releases_on_drop() {
        let dir = tempfile::tempdir().expect("temp dir");
        let lock_path = dir.path().join("nested").join(".state.lock");
        {
            let _guard = lock_file(&lock_path).expect("acquire first lock");
            assert!(lock_path.exists());
        }
        let _reacquired = lock_file(&lock_path).expect("reacquire released lock");
    }

    #[test]
    fn is_ignored_os_component_matches_hidden_and_excluded() {
        assert!(is_ignored_os_component(
            OsStr::new("node_modules"),
            false,
            false
        ));
        assert!(!is_ignored_os_component(
            OsStr::new("node_modules"),
            false,
            true
        ));
        assert!(is_ignored_os_component(OsStr::new(".git"), false, true));
        assert!(is_ignored_os_component(OsStr::new(".git"), true, false));
        assert!(!is_ignored_os_component(OsStr::new(".git"), true, true));
        assert!(!is_ignored_os_component(OsStr::new("src"), false, false));
    }
}
