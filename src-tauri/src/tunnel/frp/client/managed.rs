use super::*;
use crate::tunnel::process::{
    clear_managed_pid, managed_service_dir, verify_recorded_managed_pid, write_managed_pid,
};

pub(crate) fn managed_frpc_dir(workspace_id: &str) -> AppResult<PathBuf> {
    managed_service_dir("frpc", workspace_id, "workspace")
}

pub(crate) fn managed_frpc_config_path(workspace_id: &str) -> AppResult<PathBuf> {
    Ok(managed_frpc_dir(workspace_id)?.join("frpc.toml"))
}

pub(crate) fn managed_frpc_pid_path(workspace_id: &str) -> AppResult<PathBuf> {
    Ok(managed_frpc_dir(workspace_id)?.join("frpc.pid"))
}

fn managed_frpc_operation_lock_path(workspace_id: &str) -> AppResult<PathBuf> {
    Ok(managed_frpc_dir(workspace_id)?.join("frpc-operation.lock"))
}

/// 跨应用实例串行化 frpc 的停止与启动。
///
/// 同一个进程内由 `TunnelSupervisor` 的 Tokio mutex 保证串行；但用户
/// 可能在旧实例尚未完全退出时启动新实例，此时仅靠内存 mutex 仍会出现
/// stop/spawn 交叉，最终留下两个 frpc。这个短生命周期锁把整个替换过程
/// 扩展到应用进程之间，并在持有者崩溃后允许新实例回收过期锁。
pub(crate) struct FrpcOperationLock {
    path: PathBuf,
    _file: std::fs::File,
}

impl Drop for FrpcOperationLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) async fn acquire_frpc_operation_lock(
    workspace_id: &str,
) -> AppResult<FrpcOperationLock> {
    let path = managed_frpc_operation_lock_path(workspace_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let started = Instant::now();
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())?;
                return Ok(FrpcOperationLock { path, _file: file });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if stale_lock(&path) {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                if started.elapsed() >= FRPC_OPERATION_LOCK_TIMEOUT {
                    return Err(AppError::Message(
                        "等待另一个 frpc 操作完成超时，请稍后重试。".into(),
                    ));
                }
                sleep(Duration::from_millis(50)).await;
            }
            Err(error) => {
                return Err(AppError::Message(format!("创建 frpc 操作锁失败：{error}")));
            }
        }
    }
}

fn stale_lock(path: &Path) -> bool {
    if let Ok(contents) = std::fs::read_to_string(path) {
        if let Some(pid) = contents
            .lines()
            .next()
            .and_then(|line| line.trim().parse().ok())
        {
            return !platform().is_process_alive(pid);
        }
    }

    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age >= FRPC_STALE_LOCK_AFTER)
}

pub(super) fn write_managed_frpc_pid(
    workspace_id: &str,
    pid: u32,
    image_path: &Path,
) -> AppResult<()> {
    write_managed_pid(&managed_frpc_pid_path(workspace_id)?, pid, image_path)
}

pub(crate) fn clear_managed_frpc_pid(workspace_id: &str) {
    if let Ok(path) = managed_frpc_pid_path(workspace_id) {
        clear_managed_pid(&path);
    }
}

pub(crate) async fn stop_recorded_frpc_instance(workspace_id: &str) -> AppResult<bool> {
    let path = managed_frpc_pid_path(workspace_id)?;
    let Some((pid, recorded_image)) = verify_recorded_managed_pid(&path)? else {
        return Ok(false);
    };

    // Soft terminate, then escalate with retries. A single TerminateProcess can
    // leave frpc alive long enough on Windows to fail the whole MCP stop.
    let soft_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let _ = platform().terminate_process_tree(pid);
    while platform().is_process_alive(pid) && tokio::time::Instant::now() < soft_deadline {
        sleep(Duration::from_millis(50)).await;
    }

    if platform().is_process_alive(pid) {
        let force_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while platform().is_process_alive(pid) && tokio::time::Instant::now() < force_deadline {
            let _ = platform().terminate_process_tree(pid);
            // Also match by image path in case the PID was recycled or the
            // process tree root no longer owns child frpc workers.
            let _ = platform().terminate_processes_by_image_path(Path::new(&recorded_image));
            sleep(Duration::from_millis(100)).await;
        }
    }

    if platform().is_process_alive(pid) {
        // Last resort: kill every process running our managed frpc binary.
        let _ = platform().terminate_processes_by_image_path(Path::new(&recorded_image));
        sleep(Duration::from_millis(200)).await;
    }

    if platform().is_process_alive(pid) {
        return Err(AppError::Message(format!(
            "停止工作区 frpc 超时，PID {pid} 仍在运行。"
        )));
    }
    clear_managed_pid(&path);
    sleep(FRPC_RESTART_GRACE).await;
    Ok(true)
}
