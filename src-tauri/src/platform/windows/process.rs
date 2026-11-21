use std::collections::{HashMap, HashSet, VecDeque};
use std::mem;
use std::path::Path;

use windows::core::PWSTR;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_INSUFFICIENT_BUFFER, HANDLE, INVALID_HANDLE_VALUE,
    WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
    PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};

use crate::error::{AppError, AppResult};

const STACK_IMAGE_PATH_LEN: usize = 512;
const MAX_IMAGE_PATH_LEN: usize = 32_768;

pub fn is_process_alive(pid: u32) -> bool {
    unsafe {
        // Prefer synchronize access so we can tell a zombie/exiting process
        // from a truly live one. OpenProcess can succeed briefly after exit.
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        )
        .or_else(|_| OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid));
        let Ok(handle) = handle else {
            return false;
        };
        if handle == INVALID_HANDLE_VALUE {
            return false;
        }
        // WAIT_OBJECT_0 means the process object is already signaled (exited).
        let still_running = WaitForSingleObject(handle, 0) != WAIT_OBJECT_0;
        let _ = CloseHandle(handle);
        still_running
    }
}

pub fn process_image_path(pid: u32) -> AppResult<Option<String>> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|err| AppError::Message(format!("OpenProcess failed: {err}")))?;
        if handle == INVALID_HANDLE_VALUE {
            return Ok(None);
        }

        let mut stack_buf = [0u16; STACK_IMAGE_PATH_LEN];
        let mut size = stack_buf.len() as u32;
        if QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(stack_buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok()
        {
            let _ = CloseHandle(handle);
            return Ok(Some(String::from_utf16_lossy(&stack_buf[..size as usize])));
        }

        if GetLastError() != ERROR_INSUFFICIENT_BUFFER {
            let _ = CloseHandle(handle);
            return Ok(None);
        }

        let mut size = MAX_IMAGE_PATH_LEN as u32;
        let mut heap_buf = vec![0u16; MAX_IMAGE_PATH_LEN];
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(heap_buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        if ok.is_err() {
            return Ok(None);
        }
        heap_buf.truncate(size as usize);
        Ok(Some(String::from_utf16_lossy(&heap_buf)))
    }
}

pub fn terminate_process_tree(root_pid: u32) -> AppResult<()> {
    let children = collect_child_pids(root_pid)?;
    for pid in children.into_iter().rev() {
        terminate_pid(pid)?;
    }
    terminate_pid(root_pid)
}

/// 只终止镜像路径完全匹配的进程，避免误杀用户自行运行的其它 frpc。
pub fn terminate_processes_by_image_path(image_path: &Path) -> AppResult<usize> {
    let expected = normalize_image_path(image_path);
    let expected_name = Path::new(&expected)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase);
    let mut matched = Vec::new();

    for pid in process_ids()? {
        // System/protected processes may deny PROCESS_QUERY_LIMITED_INFORMATION.
        // They cannot be the managed frpc, so skip them without failing restart.
        if let Ok(Some(actual)) = process_image_path(pid) {
            if let Some(expected_name) = expected_name.as_deref() {
                let actual_name = Path::new(&actual)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_ascii_lowercase);
                if actual_name.as_deref() != Some(expected_name) {
                    continue;
                }
            }
            if normalize_image_path(Path::new(&actual)) == expected {
                matched.push(pid);
            }
        }
    }

    let mut terminated = 0;
    for pid in matched {
        if terminate_process_tree(pid).is_ok() {
            terminated += 1;
        }
    }
    Ok(terminated)
}

fn normalize_image_path(path: &Path) -> String {
    let normalized = std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('/', "\\")
        .trim_matches('"')
        .to_ascii_lowercase();

    // QueryFullProcessImageNameW may return either a DOS path or an NT path
    // with the `\\?\\` prefix. Treat both forms as the same executable so a
    // stale frpc from a previous application instance cannot escape cleanup.
    normalized
        .strip_prefix("\\\\?\\")
        .unwrap_or(&normalized)
        .to_string()
}

fn process_ids() -> AppResult<Vec<u32>> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut pids = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|err| AppError::Message(format!("CreateToolhelp32Snapshot failed: {err}")))?;
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(AppError::Message("invalid process snapshot".into()));
        }

        let mut entry = PROCESSENTRY32W {
            dwSize: mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                pids.push(entry.th32ProcessID);
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    Ok(pids)
}

fn collect_child_pids(root_pid: u32) -> AppResult<Vec<u32>> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|err| AppError::Message(format!("CreateToolhelp32Snapshot failed: {err}")))?;
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(AppError::Message("invalid process snapshot".into()));
        }

        let mut entry = PROCESSENTRY32W {
            dwSize: mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let parent = entry.th32ParentProcessID;
                let pid = entry.th32ProcessID;
                if pid != parent {
                    children_by_parent.entry(parent).or_default().push(pid);
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
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

fn terminate_pid(pid: u32) -> AppResult<()> {
    unsafe {
        let handle: HANDLE = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid)
            .or_else(|_| OpenProcess(PROCESS_TERMINATE, false, pid))
            .map_err(|err| AppError::Message(format!("OpenProcess terminate failed: {err}")))?;
        if handle == INVALID_HANDLE_VALUE {
            return Ok(());
        }
        // Already exited — treat as success so stop does not fail on zombies.
        if WaitForSingleObject(handle, 0) == WAIT_OBJECT_0 {
            let _ = CloseHandle(handle);
            return Ok(());
        }
        let result = TerminateProcess(handle, 1);
        // Give the kernel a moment to signal exit before returning.
        let _ = WaitForSingleObject(handle, 1_000);
        let _ = CloseHandle(handle);
        result.map_err(|err| AppError::Message(format!("TerminateProcess failed: {err}")))?;
        Ok(())
    }
}

/// Working-set sample for the desktop process tree (main + WebView2 children).
#[derive(Debug, Clone, Default)]
pub struct ProcessTreeMemory {
    pub main_bytes: u64,
    pub webview_bytes: u64,
    pub webview_process_count: u32,
}

pub fn sample_process_tree_memory() -> AppResult<ProcessTreeMemory> {
    let root = std::process::id();
    let descendants = collect_child_pids(root)?;
    let mut sample = ProcessTreeMemory {
        main_bytes: working_set_bytes(root).unwrap_or(0),
        webview_bytes: 0,
        webview_process_count: 0,
    };

    for pid in descendants {
        // Exited or access-denied children must not abort the entire memory sample.
        let Ok(Some(path)) = process_image_path(pid) else {
            continue;
        };
        let name = Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name != "msedgewebview2.exe" {
            continue;
        }
        if let Some(bytes) = working_set_bytes(pid) {
            sample.webview_bytes = sample.webview_bytes.saturating_add(bytes);
            sample.webview_process_count = sample.webview_process_count.saturating_add(1);
        }
    }
    Ok(sample)
}

fn working_set_bytes(pid: u32) -> Option<u64> {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut counters = PROCESS_MEMORY_COUNTERS::default();
        counters.cb = mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = GetProcessMemoryInfo(handle, &mut counters, counters.cb).is_ok();
        let _ = CloseHandle(handle);
        if ok {
            Some(counters.WorkingSetSize as u64)
        } else {
            None
        }
    }
}
