use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::error::AppResult;
pub use crate::platform::process_common::ProcessTreeMemory;
use crate::platform::process_common::{
    collect_descendants_bfs, normalized_image_path, terminate_process_tree_unix,
};

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
    fn malloc_zone_pressure_relief(zone: *mut libc::c_void, goal: libc::size_t) -> libc::size_t;
}

/// Ask macOS libmalloc to release unused heap pages back to the OS.
pub fn trim_process_heap() {
    unsafe {
        let _ = malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
    }
}

pub fn is_process_alive(pid: u32) -> bool {
    let Ok(raw_pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if raw_pid <= 0 {
        return false;
    }
    let result = unsafe { libc::kill(raw_pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
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
    terminate_process_tree_unix(root_pid, &children, is_process_alive)
}

pub fn terminate_processes_by_image_path(image_path: &Path) -> AppResult<usize> {
    let expected = normalized_image_path(image_path);
    let expected_name = expected.file_name();
    let input_name = image_path.file_name();
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
    let root_rusage = process_rusage(root);
    let root_start_abstime = root_rusage
        .as_ref()
        .map(|info| info.ri_proc_start_abstime)
        .unwrap_or(0);
    let all_pids = super::net::all_pids()?;
    let descendants: HashSet<u32> = collect_child_pids_from(root, &all_pids)?
        .into_iter()
        .collect();

    let mut sample = ProcessTreeMemory {
        main_bytes: root_rusage.as_ref().map(rusage_memory_bytes).unwrap_or(0),
        webview_bytes: 0,
        webview_process_count: 0,
    };

    for raw_pid in all_pids {
        let pid = raw_pid as u32;
        if pid == root {
            continue;
        }
        let is_descendant = descendants.contains(&pid);
        // Short-circuit: avoid the extra responsibility lookup when already a direct descendant.
        // Require the current app's exact PID so processes launched by the same terminal/launcher
        // are not misattributed to this desktop app.
        if !is_descendant && responsible_pid(pid) != Some(root) {
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

fn collect_child_pids(root_pid: u32) -> AppResult<Vec<u32>> {
    let pids = super::net::all_pids()?;
    collect_child_pids_from(root_pid, &pids)
}

fn collect_child_pids_from(root_pid: u32, pids: &[i32]) -> AppResult<Vec<u32>> {
    let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::with_capacity(pids.len());
    for &raw_pid in pids {
        let Ok(pid) = u32::try_from(raw_pid) else {
            continue;
        };
        let Ok(Some(parent)) = super::net::parent_pid(pid) else {
            continue;
        };
        if parent != pid {
            children_by_parent.entry(parent).or_default().push(pid);
        }
    }
    Ok(collect_descendants_bfs(root_pid, &children_by_parent))
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
    fn is_process_alive_detects_current_process_and_rejects_zero() {
        assert!(is_process_alive(std::process::id()));
        assert!(!is_process_alive(0));
    }

    #[test]
    fn sample_process_tree_memory_reads_current_process_footprint() {
        let sample = sample_process_tree_memory().expect("采样当前进程树内存不应失败");
        assert!(sample.main_bytes > 0, "当前主进程内存应大于 0");
    }

    #[test]
    fn collect_descendants_bfs_handles_out_of_order_grandchildren() {
        // Grandchild 50 has a lower PID than its parent 200; single-pass iteration would miss it.
        let mut map: HashMap<u32, Vec<u32>> = HashMap::new();
        map.insert(200, vec![50, 51]);
        map.insert(100, vec![200]);
        map.insert(50, vec![25]);

        assert_eq!(collect_descendants_bfs(100, &map), vec![200, 50, 51, 25]);
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
