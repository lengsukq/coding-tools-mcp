//! Unified async runtime facade across Desktop Tauri (`desktop-ui`) and
//! pure-Tokio headless CLI/daemon builds (`--no-default-features`).

use std::future::Future;

#[cfg(feature = "desktop-ui")]
pub type JoinHandle<T> = tauri::async_runtime::JoinHandle<T>;

#[cfg(not(feature = "desktop-ui"))]
pub type JoinHandle<T> = tokio::task::JoinHandle<T>;

#[cfg(feature = "desktop-ui")]
pub fn set(handle: tokio::runtime::Handle) {
    tauri::async_runtime::set(handle);
}

#[cfg(not(feature = "desktop-ui"))]
static EXTERNAL_HANDLE: std::sync::OnceLock<tokio::runtime::Handle> = std::sync::OnceLock::new();

#[cfg(not(feature = "desktop-ui"))]
static FALLBACK_RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

#[cfg(not(feature = "desktop-ui"))]
pub fn set(handle: tokio::runtime::Handle) {
    let _ = EXTERNAL_HANDLE.set(handle);
}

#[cfg(not(feature = "desktop-ui"))]
fn global_handle() -> tokio::runtime::Handle {
    if let Some(handle) = EXTERNAL_HANDLE.get() {
        return handle.clone();
    }
    FALLBACK_RUNTIME
        .get_or_init(|| {
            let workers = std::thread::available_parallelism()
                .map(|n| n.get().clamp(2, 4))
                .unwrap_or(2);
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(workers)
                .max_blocking_threads(16)
                .enable_all()
                .build()
                .expect("failed to initialize headless tokio runtime")
        })
        .handle()
        .clone()
}

pub fn spawn<F>(task: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    #[cfg(feature = "desktop-ui")]
    {
        tauri::async_runtime::spawn(task)
    }
    #[cfg(not(feature = "desktop-ui"))]
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(task)
        } else {
            global_handle().spawn(task)
        }
    }
}

pub fn spawn_blocking<F, R>(task: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    #[cfg(feature = "desktop-ui")]
    {
        tauri::async_runtime::spawn_blocking(task)
    }
    #[cfg(not(feature = "desktop-ui"))]
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn_blocking(task)
        } else {
            global_handle().spawn_blocking(task)
        }
    }
}

pub fn block_on<F: Future>(task: F) -> F::Output {
    #[cfg(feature = "desktop-ui")]
    {
        tauri::async_runtime::block_on(task)
    }
    #[cfg(not(feature = "desktop-ui"))]
    {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
                tokio::task::block_in_place(|| handle.block_on(task))
            }
            _ => global_handle().block_on(task),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_on_and_spawn_work_outside_tokio_context() {
        let value = block_on(async {
            let handle = spawn(async { 21 * 2 });
            let blocking = spawn_blocking(|| 8);
            let a = handle.await.expect("spawned task");
            let b = blocking.await.expect("blocking task");
            a + b
        });
        assert_eq!(value, 50);
    }
}
