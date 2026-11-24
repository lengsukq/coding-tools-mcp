use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use regex::RegexBuilder;
use serde_json::{json, Value};

use crate::util::hash::sha256_hex;
use crate::util::text::truncate_utf8;

const OUTPUT_TTL: Duration = Duration::from_secs(3 * 24 * 60 * 60);
const OUTPUT_CAPACITY_BYTES: u64 = 512 * 1024 * 1024;
const CLEANUP_MIN_INTERVAL_SECS: u64 = 60;
const SEARCH_PREVIEW_BYTES: usize = 512;

#[derive(Debug, Clone)]
pub(super) struct FullOutputPage {
    pub content: Vec<u8>,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub total_bytes: usize,
}

#[derive(Debug)]
pub(super) struct CommandOutputStore {
    root: PathBuf,
    last_cleanup_unix: AtomicU64,
}

pub(super) fn workspace_output_dir(workspace_root: &Path) -> PathBuf {
    let workspace_key = sha256_hex(workspace_root.to_string_lossy().as_bytes());
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("coding-tools-mcp")
        .join("command-output")
        .join(&workspace_key[..16])
}

impl CommandOutputStore {
    pub(super) fn for_workspace(workspace_root: &Path) -> Self {
        let root = workspace_output_dir(workspace_root);
        let _ = fs::create_dir_all(&root);
        Self {
            root,
            last_cleanup_unix: AtomicU64::new(0),
        }
    }

    pub(super) fn prepare_command(&self, command_id: &str) -> bool {
        let directory = self.command_dir(command_id);
        if fs::create_dir_all(&directory).is_err() {
            return false;
        }
        let mut ready = true;
        for stream in ["stdout", "stderr"] {
            ready &= File::create(self.stream_path(command_id, stream)).is_ok();
        }
        self.touch_command(command_id);
        ready
    }

    pub(super) async fn open_writer(
        &self,
        command_id: &str,
        stream: &str,
    ) -> Option<tokio::fs::File> {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.stream_path(command_id, stream))
            .await
            .ok()
    }

    pub(super) fn read_page(
        &self,
        command_id: &str,
        stream: &str,
        requested_offset: usize,
        limit: usize,
    ) -> std::io::Result<Option<FullOutputPage>> {
        let path = self.stream_path(command_id, stream);
        if !path.is_file() {
            return Ok(None);
        }
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let total_bytes = file.metadata()?.len() as usize;
        let offset = requested_offset.min(total_bytes);
        file.seek(SeekFrom::Start(offset as u64))?;
        let mut content = vec![0u8; limit.min(1_048_576)];
        let read = file.read(&mut content)?;
        content.truncate(read);
        let logical_end = offset.saturating_add(read);
        self.touch_command(command_id);
        Ok(Some(FullOutputPage {
            content,
            offset,
            next_offset: (logical_end < total_bytes).then_some(logical_end),
            total_bytes,
        }))
    }

    pub(super) fn search(
        &self,
        command_id: &str,
        stream: &str,
        query: &str,
        regex: bool,
        case_sensitive: bool,
        max_matches: usize,
    ) -> std::io::Result<Option<Value>> {
        let path = self.stream_path(command_id, stream);
        if !path.is_file() {
            return Ok(None);
        }
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let total_bytes = file.metadata()?.len() as usize;
        let matcher = if regex {
            Some(
                RegexBuilder::new(query)
                    .case_insensitive(!case_sensitive)
                    .build()
                    .map_err(|error| {
                        std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string())
                    })?,
            )
        } else {
            None
        };
        let normalized_query = (!case_sensitive).then(|| query.to_lowercase());
        let mut reader = BufReader::new(file);
        let mut line = Vec::new();
        let mut byte_offset = 0usize;
        let mut line_number = 0usize;
        let mut total_matches = 0usize;
        let mut matches = Vec::new();
        loop {
            line.clear();
            let read = reader.read_until(b'\n', &mut line)?;
            if read == 0 {
                break;
            }
            line_number += 1;
            let text = String::from_utf8_lossy(&line);
            let matched = if let Some(matcher) = &matcher {
                matcher.is_match(&text)
            } else if case_sensitive {
                text.contains(query)
            } else {
                text.to_lowercase()
                    .contains(normalized_query.as_deref().unwrap_or_default())
            };
            if matched {
                total_matches += 1;
                if matches.len() < max_matches {
                    matches.push(json!({
                        "line": line_number,
                        "offset": byte_offset,
                        "preview": utf8_preview(&text, SEARCH_PREVIEW_BYTES)
                    }));
                }
            }
            byte_offset = byte_offset.saturating_add(read);
        }
        self.touch_command(command_id);
        Ok(Some(json!({
            "query": query,
            "regex": regex,
            "case_sensitive": case_sensitive,
            "matches": matches,
            "total_matches": total_matches,
            "truncated": total_matches > max_matches,
            "total_stream_bytes": total_bytes
        })))
    }

    pub(super) fn cleanup_best_effort(self: &Arc<Self>) {
        if !self.try_claim_cleanup_window() {
            return;
        }
        let store = Arc::clone(self);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn_blocking(move || store.cleanup_now());
        } else {
            crate::async_rt::spawn_blocking(move || store.cleanup_now());
        }
    }

    fn try_claim_cleanup_window(&self) -> bool {
        let now_secs = current_unix_secs().max(1);
        loop {
            let last = self.last_cleanup_unix.load(Ordering::Relaxed);
            if last != 0 && now_secs.saturating_sub(last) < CLEANUP_MIN_INTERVAL_SECS {
                return false;
            }
            if self
                .last_cleanup_unix
                .compare_exchange(last, now_secs, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
        }
    }

    fn cleanup_now(&self) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        let now = SystemTime::now();
        let mut commands = Vec::<(PathBuf, SystemTime, u64)>::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let (modified, bytes) = directory_stats(&path);
            if now.duration_since(modified).unwrap_or_default() > OUTPUT_TTL {
                let _ = fs::remove_dir_all(path);
                continue;
            }
            commands.push((path, modified, bytes));
        }
        let mut total = commands.iter().map(|(_, _, bytes)| *bytes).sum::<u64>();
        if total <= OUTPUT_CAPACITY_BYTES {
            return;
        }
        commands.sort_by_key(|(_, modified, _)| *modified);
        for (path, _, bytes) in commands {
            if total <= OUTPUT_CAPACITY_BYTES {
                break;
            }
            if fs::remove_dir_all(path).is_ok() {
                total = total.saturating_sub(bytes);
            }
        }
    }

    fn command_dir(&self, command_id: &str) -> PathBuf {
        self.root.join(command_id)
    }

    fn stream_path(&self, command_id: &str, stream: &str) -> PathBuf {
        self.command_dir(command_id).join(format!("{stream}.log"))
    }

    fn touch_command(&self, command_id: &str) {
        let _ = fs::write(
            self.command_dir(command_id).join(".access"),
            current_unix_secs().to_string(),
        );
    }

    #[cfg(test)]
    fn at_root(root: PathBuf) -> Self {
        Self {
            root,
            last_cleanup_unix: AtomicU64::new(0),
        }
    }
}

fn directory_stats(path: &Path) -> (SystemTime, u64) {
    let mut newest = SystemTime::UNIX_EPOCH;
    let mut bytes = 0u64;
    for metadata in fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
    {
        bytes = bytes.saturating_add(metadata.len());
        newest = newest.max(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
    }
    (newest, bytes)
}

fn current_unix_secs() -> u64 {
    crate::util::time::unix_now_secs()
}

fn utf8_preview(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.trim_end_matches(['\r', '\n']).to_string();
    }
    format!(
        "{}…",
        truncate_utf8(value, max_bytes).trim_end_matches(['\r', '\n'])
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn unavailable_cache_root_degrades_without_panicking() {
        let root = tempdir().expect("temp");
        let file_root = root.path().join("not-a-directory");
        std::fs::write(&file_root, "occupied").expect("file root");
        let store = CommandOutputStore::at_root(file_root);
        assert!(!store.prepare_command("command"));
        assert!(store
            .read_page("command", "stdout", 0, 1024)
            .expect("read fallback")
            .is_none());
    }

    #[test]
    fn cleanup_window_throttles_back_to_back_scans() {
        let root = tempdir().expect("temp");
        let store = CommandOutputStore::at_root(root.path().to_path_buf());
        assert!(store.try_claim_cleanup_window());
        assert!(
            !store.try_claim_cleanup_window(),
            "second cleanup inside CLEANUP_MIN_INTERVAL_SECS must be throttled"
        );
    }
}
