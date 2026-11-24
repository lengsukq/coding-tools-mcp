//! Cross-platform process tree helpers shared by macOS, Linux, and Windows backends.

use std::collections::{HashMap, HashSet, VecDeque};
#[cfg(unix)]
use std::path::{Path, PathBuf};

#[cfg(unix)]
use crate::error::{AppError, AppResult};

#[cfg(unix)]
const TERM_WAIT_STEP_MS: u64 = 20;
#[cfg(unix)]
const TERM_WAIT_MAX_STEPS: usize = 10;

/// Memory footprint sample for the desktop process tree (main + WebView helpers).
#[derive(Debug, Clone, Default)]
pub struct ProcessTreeMemory {
    pub main_bytes: u64,
    pub webview_bytes: u64,
    pub webview_process_count: u32,
}

#[cfg(unix)]
pub(crate) fn normalized_image_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn collect_descendants_bfs(
    root_pid: u32,
    children_by_parent: &HashMap<u32, Vec<u32>>,
) -> Vec<u32> {
    let mut queue = VecDeque::from([root_pid]);
    let mut seen = HashSet::from([root_pid]);
    let mut ordered = Vec::new();
    while let Some(parent) = queue.pop_front() {
        if let Some(children) = children_by_parent.get(&parent) {
            for &child in children {
                if seen.insert(child) {
                    ordered.push(child);
                    queue.push_back(child);
                }
            }
        }
    }
    ordered
}

#[cfg(unix)]
pub(crate) fn signal_pid(pid: u32, signal: i32) -> AppResult<()> {
    let result = unsafe { libc::kill(pid as i32, signal) };
    if result == 0 || std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
        return Ok(());
    }
    Err(AppError::Message(format!(
        "kill({pid}, {signal}) failed: {}",
        std::io::Error::last_os_error()
    )))
}

#[cfg(unix)]
pub(crate) fn terminate_process_tree_unix(
    root_pid: u32,
    children: &[u32],
    is_alive: impl Fn(u32) -> bool,
) -> AppResult<()> {
    for pid in children.iter().copied().rev() {
        signal_pid(pid, libc::SIGTERM)?;
    }
    signal_pid(root_pid, libc::SIGTERM)?;

    for _ in 0..TERM_WAIT_MAX_STEPS {
        if !is_alive(root_pid) && children.iter().all(|&pid| !is_alive(pid)) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(TERM_WAIT_STEP_MS));
    }

    for &pid in children {
        if is_alive(pid) {
            let _ = signal_pid(pid, libc::SIGKILL);
        }
    }
    if is_alive(root_pid) {
        signal_pid(root_pid, libc::SIGKILL)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_descendants_bfs_handles_out_of_order_grandchildren() {
        // Grandchild 50 has a lower PID than its parent 200; single-pass iteration would miss it.
        let mut map: HashMap<u32, Vec<u32>> = HashMap::new();
        map.insert(200, vec![50, 51]);
        map.insert(100, vec![200]);
        map.insert(50, vec![25]);

        assert_eq!(collect_descendants_bfs(100, &map), vec![200, 50, 51, 25]);
    }
}
