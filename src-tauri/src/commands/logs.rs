use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::tunnel::log_dir_for_profile;
use crate::util::text::tail_chars;

const MAX_LOG_BYTES: usize = 8192;
const MAX_LOG_CHARS: usize = 4000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogChunk {
    pub name: String,
    pub content: String,
}

fn log_file_names(service: &str) -> AppResult<&'static [&'static str]> {
    match service {
        "mcp" => Ok(&["mcp-requests.log", "stderr.log", "stdout.log"]),
        other => Err(AppError::Message(format!("unknown log service: {other}"))),
    }
}

fn read_log_tail(path: &Path) -> AppResult<String> {
    let mut file = File::open(path)?;
    let size = file.seek(SeekFrom::End(0))?;
    let start = size.saturating_sub(MAX_LOG_BYTES as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::with_capacity((size - start).min(MAX_LOG_BYTES as u64) as usize);
    file.read_to_end(&mut buf)?;

    // When seeking into the middle of a file, skip leading UTF-8 continuation bytes
    // so we do not emit a garbled replacement character at the start of the tail.
    let slice = if start > 0 {
        let offset = buf
            .iter()
            .position(|byte| (byte & 0b1100_0000) != 0b1000_0000)
            .unwrap_or(buf.len());
        &buf[offset..]
    } else {
        &buf
    };

    let text = String::from_utf8_lossy(slice);
    Ok(tail_chars(&text, MAX_LOG_CHARS).to_owned())
}

#[tauri::command]
pub async fn read_workspace_logs(
    state: State<'_, AppState>,
    id: String,
    service: String,
) -> AppResult<Vec<LogChunk>> {
    let _ = state.workspace_path(&id)?;
    let names = log_file_names(&service)?;
    let log_dir = log_dir_for_profile(&id);

    tauri::async_runtime::spawn_blocking(move || {
        let mut chunks = Vec::new();
        for &name in names {
            let path = log_dir.join(name);
            if !path.exists() {
                continue;
            }
            let content = read_log_tail(&path)?;
            chunks.push(LogChunk {
                name: name.to_string(),
                content,
            });
        }
        Ok(chunks)
    })
    .await
    .map_err(|err| AppError::Message(format!("读取日志任务失败: {err}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_log_tail_handles_multibyte_utf8_and_char_limit() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("mcp-requests.log");
        // Write > MAX_LOG_BYTES of 3-byte Chinese characters so seek lands mid-file.
        let line = "日志记录测试内容\n";
        let repeated = line.repeat(600);
        std::fs::write(&path, &repeated).expect("write log");

        let tail = read_log_tail(&path).expect("read tail");
        assert!(tail.chars().count() <= MAX_LOG_CHARS);
        assert!(!tail.starts_with('\u{FFFD}'));
        assert!(tail.ends_with("日志记录测试内容\n"));
    }
}
