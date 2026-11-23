use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

const TERM_WAIT_STEP_MS: u64 = 20;
const TERM_WAIT_MAX_STEPS: usize = 10;

/// Memory footprint sample for the Linux desktop process tree (main + WebKitGTK helpers).
#[derive(Debug, Clone, Default)]
pub struct ProcessTreeMemory {
    pub main_bytes: u64,
    pub webview_bytes: u64,
    pub webview_process_count: u32,
}

pub fn is_process_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).is_dir()
}

pub fn process_image_path(pid: u32) -> AppResult<Option<String>> {
    let exe = Path::new("/proc").join(pid.to_string()).join("exe");
    match fs::read_link(exe) {
        Ok(path) => Ok(Some(
            strip_deleted_exe_suffix(&path.to_string_lossy()).to_string(),
        )),
        Err(_) => Ok(None),
    }
}

pub(crate) fn strip_deleted_exe_suffix(raw: &str) -> &str {
    raw.strip_suffix(" (deleted)").unwrap_or(raw)
}

pub fn terminate_processes_by_image_path(image_path: &Path) -> AppResult<usize> {
    let expected = normalized_image_path(image_path);
    let expected_name = expected.file_name();
    let input_name = image_path.file_name();
    let current_pid = std::process::id();
    let mut terminated = 0;

    let entries = fs::read_dir("/proc")
        .map_err(|err| AppError::Message(format!("read /proc failed: {err}")))?;
    for entry in entries.flatten() {
        let Some(pid_str) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !pid_str.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if pid == current_pid {
            continue;
        }
        let Some(actual) = process_image_path(pid)? else {
            continue;
        };
        let actual_path = Path::new(&actual);
        let actual_name = actual_path.file_name();
        if expected_name.is_some() && actual_name != expected_name && actual_name != input_name {
            continue;
        }
        if normalized_image_path(actual_path) != expected {
            continue;
        }
        terminate_process_tree(pid)?;
        terminated += 1;
    }
    Ok(terminated)
}

pub fn sample_process_tree_memory() -> AppResult<ProcessTreeMemory> {
    let root = std::process::id();
    let page_size = system_page_size();
    let main_bytes = read_pid_rss_bytes(root, page_size).unwrap_or(0);
    let descendants = collect_child_pids(root)?;
    let mut webview_bytes = 0u64;
    let mut webview_process_count = 0u32;

    for pid in descendants {
        let exe = process_image_path(pid).ok().flatten().unwrap_or_default();
        let comm = fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("comm"))
            .unwrap_or_default();
        if !is_webkit_helper_process(&exe, comm.trim()) {
            continue;
        }
        if let Some(rss) = read_pid_rss_bytes(pid, page_size) {
            webview_bytes = webview_bytes.saturating_add(rss);
            webview_process_count = webview_process_count.saturating_add(1);
        }
    }

    Ok(ProcessTreeMemory {
        main_bytes,
        webview_bytes,
        webview_process_count,
    })
}

pub(crate) fn parse_statm_resident_bytes(statm: &str, page_size: u64) -> Option<u64> {
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(resident_pages.saturating_mul(page_size))
}

pub(crate) fn is_webkit_helper_process(exe_path: &str, comm: &str) -> bool {
    let exe_name = Path::new(exe_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let comm_lower = comm.trim().to_ascii_lowercase();
    exe_name.starts_with("webkit")
        || comm_lower.starts_with("webkit")
        || exe_name.contains("webkitwebprocess")
        || exe_name.contains("webkitnetworkprocess")
        || exe_name.contains("webkitgpuprocess")
}

fn read_pid_rss_bytes(pid: u32, page_size: u64) -> Option<u64> {
    let statm = fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("statm")).ok()?;
    parse_statm_resident_bytes(&statm, page_size)
}

fn system_page_size() -> u64 {
    let raw = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if raw > 0 {
        raw as u64
    } else {
        4096
    }
}

fn normalized_image_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn terminate_process_tree(root_pid: u32) -> AppResult<()> {
    let children = collect_child_pids(root_pid)?;
    for pid in children.iter().copied().rev() {
        signal_pid(pid, libc::SIGTERM)?;
    }
    signal_pid(root_pid, libc::SIGTERM)?;

    for _ in 0..TERM_WAIT_MAX_STEPS {
        if !is_process_alive(root_pid) && children.iter().all(|&pid| !is_process_alive(pid)) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(TERM_WAIT_STEP_MS));
    }

    for &pid in &children {
        if is_process_alive(pid) {
            let _ = signal_pid(pid, libc::SIGKILL);
        }
    }
    if is_process_alive(root_pid) {
        signal_pid(root_pid, libc::SIGKILL)?;
    }
    Ok(())
}

fn collect_child_pids(root_pid: u32) -> AppResult<Vec<u32>> {
    let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
    let proc = Path::new("/proc");

    for entry in fs::read_dir(proc)
        .map_err(|err| AppError::Message(format!("read /proc failed: {err}")))?
        .flatten()
    {
        let file_name = entry.file_name();
        let Some(pid_str) = file_name.to_str() else {
            continue;
        };
        if !pid_str.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        // Short-lived processes can exit between read_dir and reading stat; skip them.
        let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some(parent) = parent_pid_from_stat(&stat) else {
            continue;
        };
        if parent != pid {
            children_by_parent.entry(parent).or_default().push(pid);
        }
    }

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
    Ok(ordered)
}

fn parent_pid_from_stat(stat: &str) -> Option<u32> {
    let end = stat.rfind(')')?;
    let rest = stat.get(end + 2..)?;
    let parent = rest.split_whitespace().nth(1)?;
    parent.parse().ok()
}

fn signal_pid(pid: u32, signal: i32) -> AppResult<()> {
    let result = unsafe { libc::kill(pid as i32, signal) };
    if result == 0 || std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
        return Ok(());
    }
    Err(AppError::Message(format!(
        "kill({pid}, {signal}) failed: {}",
        std::io::Error::last_os_error()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_statm_rss_and_identifies_webkitgtk_helpers() {
        assert_eq!(
            parse_statm_resident_bytes("120450 8192 4096 512 0 6000 0\n", 4096),
            Some(8192 * 4096)
        );
        assert!(is_webkit_helper_process(
            "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess",
            "WebKitWebProces"
        ));
        assert!(is_webkit_helper_process(
            "/usr/libexec/webkit2gtk-4.1/WebKitNetworkProcess",
            "WebKitNetworkPr"
        ));
        assert!(!is_webkit_helper_process("/usr/bin/git", "git"));
        assert_eq!(
            strip_deleted_exe_suffix("/opt/coding-tools-mcp/frpc (deleted)"),
            "/opt/coding-tools-mcp/frpc"
        );
        assert_eq!(
            parent_pid_from_stat("1234 (WebKit (Web)) S 999 999 999"),
            Some(999)
        );
    }
}
