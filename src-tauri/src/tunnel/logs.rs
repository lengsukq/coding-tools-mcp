use std::path::{Path, PathBuf};

use crate::platform::platform;

/// Maximum size of a single runtime/MCP/tunnel log file before rotating to `.old`.
pub(crate) const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024;

pub fn log_dir_for_profile(profile_id: &str) -> PathBuf {
    platform()
        .app_config_dir()
        .map(|home| home.join("logs").join(profile_id))
        .unwrap_or_else(|_| PathBuf::from("logs").join(profile_id))
}

/// Rotate `path` to `<path>.old` once it reaches `MAX_LOG_FILE_BYTES`, keeping
/// at most one archived generation so long-running sessions never fill disk.
pub(crate) fn rotate_log_if_needed(path: &Path) {
    rotate_log_if_exceeds(path, MAX_LOG_FILE_BYTES);
}

fn rotate_log_if_exceeds(path: &Path, max_bytes: u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() < max_bytes {
        return;
    }
    let backup = PathBuf::from(format!("{}.old", path.display()));
    let _ = std::fs::remove_file(&backup);
    let _ = std::fs::rename(path, &backup);
}

pub fn append_profile_log(profile_id: &str, file_name: &str, line: &str) {
    use std::io::Write;

    let log_dir = log_dir_for_profile(profile_id);
    if std::fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let path = log_dir.join(file_name);
    rotate_log_if_needed(&path);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_log_file_when_exceeding_size_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log_path = dir.path().join("mcp-requests.log");
        std::fs::write(&log_path, "0123456789").expect("write");

        rotate_log_if_exceeds(&log_path, 8);

        assert!(!log_path.exists());
        let backup = PathBuf::from(format!("{}.old", log_path.display()));
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "0123456789");
    }
}
