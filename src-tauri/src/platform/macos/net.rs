use std::mem;
use std::process::Command;

use crate::error::{AppError, AppResult};

/// Exact ABI layout of macOS `struct proc_bsdshortinfo` (`sys/proc_info.h`, 64 bytes).
#[repr(C)]
struct ProcBsdShortInfo {
    pbsi_pid: u32,
    pbsi_ppid: u32,
    pbsi_pgid: u32,
    pbsi_status: u32,
    pbsi_comm: [i8; 16],
    pbsi_flags: u32,
    pbsi_uid: u32,
    pbsi_gid: u32,
    pbsi_ruid: u32,
    pbsi_rgid: u32,
    pbsi_svuid: u32,
    pbsi_svgid: u32,
    pbsi_rfu: u32,
}

const PROC_PIDT_SHORTBSDINFO: i32 = 13;
const ALL_PIDS_HEADROOM: usize = 128;
const ALL_PIDS_MAX_RETRIES: usize = 3;

#[link(name = "proc")]
extern "C" {
    fn proc_listallpids(buffer: *mut libc::c_void, buffersize: i32) -> i32;
    fn proc_pidinfo(
        pid: libc::c_int,
        flavor: i32,
        arg: u64,
        buffer: *mut libc::c_void,
        buffersize: i32,
    ) -> i32;
}

pub fn find_pid_listening_on_port(port: u16) -> AppResult<Option<u32>> {
    let output = Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-t"])
        .output()
        .map_err(|err| AppError::Message(format!("lsof failed: {err}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return Ok(None);
    }

    let pid = first_line
        .parse::<u32>()
        .map_err(|err| AppError::Message(format!("invalid lsof pid output: {err}")))?;
    Ok(Some(pid))
}

pub fn parent_pid(pid: u32) -> AppResult<Option<u32>> {
    let Ok(raw_pid) = libc::c_int::try_from(pid) else {
        return Ok(None);
    };
    if raw_pid <= 0 {
        return Ok(None);
    }
    let expected_size = mem::size_of::<ProcBsdShortInfo>() as i32;
    let mut info = mem::MaybeUninit::<ProcBsdShortInfo>::uninit();
    let size = unsafe {
        proc_pidinfo(
            raw_pid,
            PROC_PIDT_SHORTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            expected_size,
        )
    };
    if size < expected_size {
        return Ok(None);
    }
    let info = unsafe { info.assume_init() };
    Ok(Some(info.pbsi_ppid))
}

pub fn all_pids() -> AppResult<Vec<i32>> {
    let estimated = unsafe { proc_listallpids(std::ptr::null_mut(), 0) };
    if estimated <= 0 {
        return Err(AppError::Message(
            "proc_listallpids size query failed".into(),
        ));
    }
    let mut capacity = (estimated as usize).saturating_add(ALL_PIDS_HEADROOM);
    for _ in 0..ALL_PIDS_MAX_RETRIES {
        let mut pids = vec![0i32; capacity];
        let byte_size =
            i32::try_from(pids.len().saturating_mul(mem::size_of::<i32>())).unwrap_or(i32::MAX);
        let count = unsafe { proc_listallpids(pids.as_mut_ptr().cast(), byte_size) };
        if count <= 0 {
            return Err(AppError::Message("proc_listallpids failed".into()));
        }
        let pid_count = (count as usize).min(pids.len());
        if pid_count < pids.len() {
            pids.truncate(pid_count);
            pids.retain(|pid| *pid > 0);
            return Ok(pids);
        }
        capacity = capacity.saturating_mul(2);
    }
    Err(AppError::Message(
        "proc_listallpids buffer remained full after retries".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_bsd_short_info_matches_kernel_abi_size() {
        assert_eq!(mem::size_of::<ProcBsdShortInfo>(), 64);
    }

    #[test]
    fn parent_pid_matches_libc_getppid_for_current_process() {
        let current = std::process::id();
        let expected_ppid = unsafe { libc::getppid() } as u32;
        let observed = parent_pid(current).expect("parent_pid query should succeed");
        assert_eq!(observed, Some(expected_ppid));
    }

    #[test]
    fn all_pids_includes_current_and_parent_processes() {
        let pids = all_pids().expect("all_pids should succeed");
        let current = std::process::id() as i32;
        let parent = unsafe { libc::getppid() };
        assert!(pids.contains(&current));
        if parent > 0 {
            assert!(pids.contains(&parent));
        }
    }
}
