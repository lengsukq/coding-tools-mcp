use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::tools::workspace::{tool_ok, WorkspaceError};
use serde_json::{json, Value};

mod api;
mod buffer;
mod process;
mod signal;
mod store;

pub use api::{kill_session, read_output, write_stdin};
pub use process::ExecSession;
use signal::send_session_signal;
use store::EvictedCommand;
pub use store::{kill_workspace_sessions, workspace_session_store};

const SESSION_BUFFER_BYTES: usize = 1_048_576;
const SESSION_HEAD_BYTES: usize = 128 * 1024;
const SESSION_TAIL_BYTES: usize = SESSION_BUFFER_BYTES - SESSION_HEAD_BYTES;
const MAX_EVICTED_TOMBSTONES: usize = 256;

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
    evicted: Mutex<HashMap<String, EvictedCommand>>,
}

fn unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

impl Default for SessionStore {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            evicted: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::buffer::RetainedBuffer;
    use super::*;

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
        assert!(String::from_utf8_lossy(&buffer.tail).contains("UNIQUE_TAIL_ERROR"));

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
}
