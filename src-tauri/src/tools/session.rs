use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{Mutex as AsyncMutex, Notify};
use uuid::Uuid;

use crate::tools::workspace::{tool_ok, WorkspaceError};
use serde_json::{json, Value};

mod api;
mod buffer;
mod output_store;
mod process;
mod signal;
mod store;

pub use api::{kill_session, read_output, write_stdin};
pub use process::ExecSession;
use signal::send_session_signal;
#[cfg(test)]
use store::EvictedCommand;
use store::EvictedTombstones;
pub use store::{drop_workspace_session_store, kill_workspace_sessions, workspace_session_store};

const SESSION_BUFFER_BYTES: usize = 1_048_576;
const SESSION_HEAD_BYTES: usize = 128 * 1024;
const SESSION_TAIL_BYTES: usize = SESSION_BUFFER_BYTES - SESSION_HEAD_BYTES;
const MAX_EVICTED_TOMBSTONES: usize = 256;
pub(crate) const DEFAULT_SESSION_MAX_OUTPUT_BYTES: usize = 32_768;
// Completed commands persist full output to CommandOutputStore on disk; keep
// in-memory buffers only briefly so idle finished commands do not hold up to
// 2 MB each in RAM.
const FINISHED_COMMAND_IDLE_TTL: Duration = Duration::from_secs(3 * 60);

/// One command runtime per workspace for the lifetime of the desktop process.
///
/// MCP listeners own a ToolContext, and listener instances can be
/// restarted without restarting the desktop application. Keeping the store here
/// makes running commands a workspace resource instead of a transport-session
/// resource.
static WORKSPACE_SESSION_STORES: OnceLock<Mutex<HashMap<PathBuf, Arc<SessionStore>>>> =
    OnceLock::new();

pub struct SessionStore {
    sessions: Mutex<HashMap<String, Arc<ExecSession>>>,
    evicted: Mutex<EvictedTombstones>,
    output_store: Arc<output_store::CommandOutputStore>,
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Drop only completed, unreferenced command sessions. A live child is
/// intentionally never killed, even if its MCP connection is gone.
pub async fn reap_idle_command_sessions() -> usize {
    let stores = WORKSPACE_SESSION_STORES
        .get()
        .map(|registry| {
            lock_unpoisoned(registry)
                .values()
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut removed = 0;
    for store in stores {
        removed += store.reap_idle_completed().await;
    }
    removed
}

fn unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

impl SessionStore {
    fn for_workspace(workspace_root: &Path) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            evicted: Mutex::new(EvictedTombstones::default()),
            output_store: Arc::new(output_store::CommandOutputStore::for_workspace(
                workspace_root,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::buffer::RetainedBuffer;
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_completed_commands_reap_but_running_commands_are_preserved() {
        let workspace = tempdir().expect("workspace");
        let store = SessionStore::for_workspace(workspace.path());
        let mut completed = tokio::process::Command::new("git");
        completed.arg("--version");
        let finished = store.insert(ExecSession::new(completed.spawn().expect("git")));
        let finished_id = finished.session_id.clone();
        drop(finished);
        let mut running = tokio::process::Command::new("sleep");
        running.arg("20");
        let active = store.insert(ExecSession::new(running.spawn().expect("sleep")));
        let active_id = active.session_id.clone();
        drop(active);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(store.reap_idle_completed_with_ttl(Duration::ZERO).await, 1);
        assert!(store
            .get(&finished_id)
            .err()
            .map(|e| e.to_string())
            .is_some());
        assert!(
            store.get(&active_id).is_ok(),
            "running command must not be killed"
        );
        let active = store.get(&active_id).unwrap();
        active.kill_and_wait().await;
        store.remove(&active_id);
    }

    #[test]
    fn retained_buffer_keeps_head_and_tail_of_large_stream() {
        let mut buffer = RetainedBuffer::default();
        buffer.append(b"UNIQUE_HEAD_ERROR\n");
        buffer.append(&vec![b'x'; 10 * 1024 * 1024]);
        buffer.append(b"\nUNIQUE_TAIL_ERROR");

        assert!(buffer.total_bytes > 10 * 1024 * 1024);
        assert_eq!(buffer.head.len(), SESSION_HEAD_BYTES);
        assert_eq!(buffer.tail.len(), SESSION_TAIL_BYTES);
        assert!(buffer.evicted_bytes() > 0);
        assert!(String::from_utf8_lossy(&buffer.head).contains("UNIQUE_HEAD_ERROR"));
        assert!(
            String::from_utf8_lossy(buffer.tail.make_contiguous()).contains("UNIQUE_TAIL_ERROR")
        );

        let preview = buffer.preview(64 * 1024);
        assert!(preview.truncated);
        assert!(preview.content.contains("UNIQUE_HEAD_ERROR"));
        assert!(preview.content.contains("UNIQUE_TAIL_ERROR"));

        let head_page = buffer.read_page(0, SESSION_HEAD_BYTES);
        assert!(String::from_utf8_lossy(&head_page.content).contains("UNIQUE_HEAD_ERROR"));
        let tail_offset = head_page
            .next_offset
            .expect("tail offset after evicted gap");
        assert_eq!(tail_offset, buffer.tail_start());
        let tail_page = buffer.read_page(tail_offset, SESSION_TAIL_BYTES);
        assert!(String::from_utf8_lossy(&tail_page.content).contains("UNIQUE_TAIL_ERROR"));
        assert_eq!(tail_page.next_offset, None);
        assert_eq!(tail_page.evicted_bytes, buffer.evicted_bytes());
    }

    #[test]
    fn retained_buffer_skips_an_evicted_offset_to_the_tail() {
        let mut buffer = RetainedBuffer::default();
        buffer.append(&vec![b'a'; SESSION_BUFFER_BYTES * 2]);
        let gap_offset = SESSION_HEAD_BYTES + 1024;
        let page = buffer.read_page(gap_offset, 4096);
        assert!(buffer.evicted_bytes() > 0);
        assert_eq!(page.offset, buffer.tail_start());
        assert!(page.offset > gap_offset);
    }

    #[test]
    fn preview_keeps_tail_even_before_tail_retention_segment_is_used() {
        let mut buffer = RetainedBuffer::default();
        buffer.append(b"PREVIEW_HEAD\n");
        buffer.append(&vec![b'm'; 96 * 1024]);
        buffer.append(b"\nPREVIEW_TAIL");
        assert_eq!(buffer.evicted_bytes(), 0);
        assert!(buffer.tail.is_empty());

        let preview = buffer.preview(16 * 1024);
        assert!(preview.truncated);
        assert!(preview.content.contains("PREVIEW_HEAD"));
        assert!(preview.content.contains("PREVIEW_TAIL"));
    }

    #[test]
    fn full_output_ref_survives_without_an_active_session_and_searches_middle_content() {
        let workspace = tempdir().expect("workspace");
        let store = SessionStore::for_workspace(workspace.path());
        let command_id = "completed-command";
        assert!(store.output_store.prepare_command(command_id));
        let mut writer =
            crate::async_rt::block_on(store.output_store.open_writer(command_id, "stdout"))
                .expect("full output writer");
        let mut payload = Vec::new();
        payload.extend_from_slice(b"FULL_HEAD\n");
        payload.extend_from_slice(&vec![b'a'; 5_500_000]);
        let middle_offset = payload.len();
        payload.extend_from_slice(b"\nFULL_MIDDLE_MARKER\n");
        payload.extend_from_slice(&vec![b'b'; 5_500_000]);
        payload.extend_from_slice(b"\nFULL_TAIL\n");
        crate::async_rt::block_on(writer.write_all(&payload)).expect("write full output");
        crate::async_rt::block_on(writer.flush()).expect("flush full output");
        drop(writer);
        lock_unpoisoned(&store.evicted).insert_with_limit(
            command_id.to_string(),
            EvictedCommand {
                command_id: command_id.to_string(),
                evicted_at: unix_timestamp(),
                termination_reason: "exited".into(),
                exit_code: Some(0),
            },
            MAX_EVICTED_TOMBSTONES,
        );

        let page = api::read_output(
            &store,
            &json!({
                "output_ref": format!("command:{command_id}:stdout"),
                "offset": middle_offset,
                "limit": 128
            }),
        )
        .expect("read persisted output");
        assert_eq!(page["source"], "full");
        assert_eq!(page["raw_available"], true);
        assert!(page["content"]
            .as_str()
            .unwrap_or_default()
            .contains("FULL_MIDDLE_MARKER"));

        let search = api::read_output(
            &store,
            &json!({
                "output_ref": format!("command:{command_id}:stdout"),
                "query": "FULL_MIDDLE_MARKER"
            }),
        )
        .expect("search persisted output");
        assert_eq!(search["mode"], "search");
        assert_eq!(search["total_matches"], 1);
        assert_eq!(search["matches"][0]["offset"], middle_offset as u64 + 1);

        let legacy = api::read_output(
            &store,
            &json!({
                "output_ref": format!("session:{command_id}:stdout"),
                "offset": middle_offset,
                "limit": 128
            }),
        )
        .expect("legacy ref remains readable");
        assert_eq!(legacy["source"], "full");
        assert!(legacy["content"]
            .as_str()
            .unwrap_or_default()
            .contains("FULL_MIDDLE_MARKER"));
    }

    #[test]
    fn remember_evicted_evicts_oldest_entries_in_fifo_order() {
        let mut evicted = EvictedTombstones::default();
        let total = MAX_EVICTED_TOMBSTONES + 3;
        for index in 0..total {
            let id = format!("cmd-{index}");
            evicted.insert_with_limit(
                id.clone(),
                EvictedCommand {
                    command_id: id,
                    evicted_at: unix_timestamp(),
                    termination_reason: "exited".into(),
                    exit_code: Some(0),
                },
                MAX_EVICTED_TOMBSTONES,
            );
        }

        assert_eq!(evicted.len(), MAX_EVICTED_TOMBSTONES);
        for evicted_index in 0..3 {
            assert!(
                evicted.get(&format!("cmd-{evicted_index}")).is_none(),
                "oldest tombstone cmd-{evicted_index} should be evicted first"
            );
        }
        for retained_index in 3..total {
            assert!(
                evicted.get(&format!("cmd-{retained_index}")).is_some(),
                "newer tombstone cmd-{retained_index} should remain retained"
            );
        }
    }

    #[test]
    fn kill_workspace_sessions_does_not_create_empty_store_or_directory() {
        let workspace = tempdir().expect("workspace");
        let output_dir = output_store::workspace_output_dir(workspace.path());
        assert!(!output_dir.exists());

        let killed = kill_workspace_sessions(workspace.path());
        assert_eq!(killed, 0);
        assert!(
            !output_dir.exists(),
            "kill_workspace_sessions must not create command output cache dir for unseen workspace"
        );
        let registered = WORKSPACE_SESSION_STORES
            .get()
            .is_some_and(|registry| lock_unpoisoned(registry).contains_key(workspace.path()));
        assert!(
            !registered,
            "kill_workspace_sessions must not register an empty SessionStore"
        );
    }
}
