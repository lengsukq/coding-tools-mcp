use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

const RUSAGE_INFO_V2: libc::c_int = 2;

#[repr(C)]
struct RUsageInfoV2 {
    ri_uuid: [u8; 16],
    ri_user_time: u64,
    ri_system_time: u64,
    ri_pkg_idle_wkups: u64,
    ri_interrupt_wkups: u64,
    ri_pageins: u64,
    ri_wired_size: u64,
    ri_resident_size: u64,
    ri_phys_footprint: u64,
    ri_proc_start_abstime: u64,
    ri_proc_exit_abstime: u64,
    ri_child_user_time: u64,
    ri_child_system_time: u64,
    ri_child_pkg_idle_wkups: u64,
    ri_child_interrupt_wkups: u64,
    ri_child_pageins: u64,
    ri_child_elapsed_abstime: u64,
    ri_diskio_bytesread: u64,
    ri_diskio_byteswritten: u64,
}

#[link(name = "proc")]
extern "C" {
    fn proc_pidpath(pid: libc::c_int, buffer: *mut libc::c_void, buffer_size: u32) -> i32;
    fn proc_pid_rusage(
        pid: libc::c_int,
        flavor: libc::c_int,
        buffer: *mut RUsageInfoV2,
    ) -> libc::c_int;
}

extern "C" {
    fn responsibility_get_pid_responsible_for_pid(pid: libc::pid_t) -> libc::pid_t;
}

/// Memory footprint sample for the desktop process tree (main + WKWebView XPC helpers).
#[derive(Debug, Clone, Default)]
pub struct ProcessTreeMemory {
    pub main_bytes: u64,
    pub webview_bytes: u64,
    pub webview_process_count: u32,
}

pub fn is_process_alive(pid: u32) -> bool {
    let mut buffer = [0u8; libc::PATH_MAX as usize];
    let size = unsafe { proc_pidpath(pid as i32, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    size > 0
}

pub fn process_image_path(pid: u32) -> AppResult<Option<String>> {
    let mut buffer = [0u8; libc::PATH_MAX as usize];
    let size = unsafe { proc_pidpath(pid as i32, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    if size <= 0 {
        return Ok(None);
    }
    // proc_pidpath 返回的长度不包含末尾 NUL；只解析有效字节。
    // 若对 buffer[..size] 按 C 字符串读取，macOS 上会稳定报 invalid proc path。
    Ok(Some(
        String::from_utf8_lossy(&buffer[..size as usize]).into_owned(),
    ))
}

pub fn terminate_process_tree(root_pid: u32) -> AppResult<()> {
    let children = collect_child_pids(root_pid)?;
    for pid in children.iter().copied().rev() {
        signal_pid(pid, libc::SIGTERM)?;
    }
    signal_pid(root_pid, libc::SIGTERM)?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    for pid in &children {
        if is_process_alive(*pid) {
            let _ = signal_pid(*pid, libc::SIGKILL);
        }
    }
    if is_process_alive(root_pid) {
        signal_pid(root_pid, libc::SIGKILL)?;
    }
    Ok(())
}

pub fn terminate_processes_by_image_path(image_path: &Path) -> AppResult<usize> {
    let expected = normalized_image_path(image_path);
    let current_pid = std::process::id();
    let mut terminated = 0;

    for pid in super::net::all_pids()? {
        let pid = pid as u32;
        if pid == current_pid {
            continue;
        }
        let Some(actual) = process_image_path(pid)? else {
            continue;
        };
        if normalized_image_path(Path::new(&actual)) != expected {
            continue;
        }
        terminate_process_tree(pid)?;
        terminated += 1;
    }

    Ok(terminated)
}

pub fn sample_process_tree_memory() -> AppResult<ProcessTreeMemory> {
    let root = std::process::id();
    let root_rusage = process_rusage(root);
    let root_start_abstime = root_rusage
        .as_ref()
        .map(|info| info.ri_proc_start_abstime)
        .unwrap_or(0);
    let root_responsible = responsible_pid(root).unwrap_or(root);
    let descendants: std::collections::HashSet<u32> =
        collect_child_pids(root)?.into_iter().collect();

    let mut sample = ProcessTreeMemory {
        main_bytes: root_rusage.as_ref().map(rusage_memory_bytes).unwrap_or(0),
        webview_bytes: 0,
        webview_process_count: 0,
    };

    for raw_pid in super::net::all_pids()? {
        let pid = raw_pid as u32;
        if pid == root {
            continue;
        }
        let is_descendant = descendants.contains(&pid);
        let shares_responsibility = responsible_pid(pid) == Some(root_responsible);
        if !is_descendant && !shares_responsibility {
            continue;
        }
        let Ok(Some(path)) = process_image_path(pid) else {
            continue;
        };
        if !is_webkit_helper_path(&path) {
            continue;
        }
        let Some(info) = process_rusage(pid) else {
            continue;
        };
        if !is_descendant && info.ri_proc_start_abstime < root_start_abstime {
            continue;
        }
        sample.webview_bytes = sample
            .webview_bytes
            .saturating_add(rusage_memory_bytes(&info));
        sample.webview_process_count = sample.webview_process_count.saturating_add(1);
    }

    Ok(sample)
}

fn process_rusage(pid: u32) -> Option<RUsageInfoV2> {
    let mut info = std::mem::MaybeUninit::<RUsageInfoV2>::uninit();
    let rc = unsafe { proc_pid_rusage(pid as libc::c_int, RUSAGE_INFO_V2, info.as_mut_ptr()) };
    if rc == 0 {
        Some(unsafe { info.assume_init() })
    } else {
        None
    }
}

fn rusage_memory_bytes(info: &RUsageInfoV2) -> u64 {
    if info.ri_phys_footprint > 0 {
        info.ri_phys_footprint
    } else {
        info.ri_resident_size
    }
}

fn responsible_pid(pid: u32) -> Option<u32> {
    let resp = unsafe { responsibility_get_pid_responsible_for_pid(pid as libc::pid_t) };
    if resp > 0 {
        Some(resp as u32)
    } else {
        None
    }
}

fn is_webkit_helper_path(path: &str) -> bool {
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.starts_with("com.apple.webkit.")
}

fn normalized_image_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn collect_child_pids(root_pid: u32) -> AppResult<Vec<u32>> {
    let pids = super::net::all_pids()?;
    let mut pending = vec![root_pid];
    let mut seen = std::collections::HashSet::from([root_pid]);
    let mut ordered = Vec::new();

    for pid in pids {
        let Some(parent) = super::net::parent_pid(pid as u32)? else {
            continue;
        };
        if pending.contains(&parent) && seen.insert(pid as u32) {
            ordered.push(pid as u32);
            pending.push(pid as u32);
        }
    }
    Ok(ordered)
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
    fn process_image_path_reads_current_process() {
        let path = process_image_path(std::process::id())
            .expect("读取当前进程路径不应失败")
            .expect("当前进程应存在可读取的镜像路径");

        assert!(!path.trim().is_empty());
    }

    #[test]
    fn sample_process_tree_memory_reads_current_process_footprint() {
        let sample = sample_process_tree_memory().expect("采样当前进程树内存不应失败");
        assert!(sample.main_bytes > 0, "当前主进程内存应大于 0");
    }

    #[test]
    fn recognizes_webkit_xpc_helper_paths() {
        assert!(is_webkit_helper_path(
            "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent"
        ));
        assert!(is_webkit_helper_path(
            "/System/Volumes/Preboot/Cryptexes/OS/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.GPU.xpc/Contents/MacOS/com.apple.WebKit.GPU"
        ));
        assert!(!is_webkit_helper_path("/usr/bin/lsof"));
    }
}
