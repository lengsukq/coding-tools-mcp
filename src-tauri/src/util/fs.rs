use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use fs2::FileExt;
use serde::Serialize;

use crate::error::AppResult;

/// Exclusive advisory file lock released automatically on drop.
pub struct FileLock {
    file: File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
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
    FileExt::lock_exclusive(&file)?;
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
fn atomic_replace(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, target: &Path) -> io::Result<()> {
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
}
