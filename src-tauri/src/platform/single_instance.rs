//! Single-instance process coordination across desktop platforms.

#[cfg(all(target_os = "windows", feature = "desktop-ui"))]
static SHOW_APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

#[cfg(target_os = "windows")]
static INSTANCE_MUTEX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

#[cfg(target_os = "windows")]
pub fn acquire_single_instance() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows::Win32::System::Threading::{
        CreateEventW, CreateMutexW, OpenEventW, SetEvent, EVENT_MODIFY_STATE,
    };

    let Ok(mutex) = (unsafe {
        CreateMutexW(
            None,
            false,
            w!("Local\\CodingToolsMcpDesktop-SingleInstance"),
        )
    }) else {
        eprintln!("创建应用单实例锁失败，为避免误清理其他实例的 frpc，本次启动已取消");
        return false;
    };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        let _ = unsafe { CloseHandle(mutex) };
        if let Ok(event) = unsafe {
            OpenEventW(
                EVENT_MODIFY_STATE,
                false,
                w!("Local\\CodingToolsMcpDesktop-ShowWindow"),
            )
        } {
            let _ = unsafe { SetEvent(event) };
            let _ = unsafe { CloseHandle(event) };
        }
        return false;
    }

    // Keep mutex handle for process lifetime (do not CloseHandle).
    let _ = INSTANCE_MUTEX.set(mutex.0 as usize);

    let Ok(event) = (unsafe {
        CreateEventW(
            None,
            false,
            false,
            w!("Local\\CodingToolsMcpDesktop-ShowWindow"),
        )
    }) else {
        return true;
    };
    // Pass the handle as usize so the waiter thread is Send (HANDLE is !Send).
    let event_bits = event.0 as usize;
    std::thread::spawn(move || {
        use windows::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0};
        use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
        let event = HANDLE(event_bits as *mut std::ffi::c_void);
        loop {
            let status = unsafe { WaitForSingleObject(event, INFINITE) };
            if status == WAIT_FAILED {
                eprintln!(
                    "single-instance WaitForSingleObject failed ({:?}); stopping show-window listener",
                    unsafe { GetLastError() }
                );
                break;
            }
            if status != WAIT_OBJECT_0 {
                continue;
            }
            #[cfg(feature = "desktop-ui")]
            if let Some(app) = SHOW_APP_HANDLE.get() {
                let _ = crate::commands::window_chrome::show_main_window(app.clone());
            }
        }
    });
    true
}

#[cfg(not(target_os = "windows"))]
pub fn acquire_single_instance() -> bool {
    true
}

#[cfg(feature = "desktop-ui")]
pub fn register_show_window_handle(app_handle: &tauri::AppHandle) {
    #[cfg(target_os = "windows")]
    {
        let _ = SHOW_APP_HANDLE.set(app_handle.clone());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app_handle;
    }
}
