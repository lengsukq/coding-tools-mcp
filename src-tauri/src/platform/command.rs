use std::ffi::OsStr;
use std::process::Command;

/// Spawn internal, non-interactive helpers without flashing a console window
/// whenever the Tauri application is running as a Windows GUI process.
/// This is for our own background probes, not user-requested terminal UI.
pub(crate) fn background_command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
