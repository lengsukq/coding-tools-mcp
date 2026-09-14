use std::path::Path;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use tokio::process::Command;

#[cfg(windows)]
pub(super) fn windows_hidden_creation_flags() -> u32 {
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
}

pub(super) fn command_for_program(program: &str, args: &[String]) -> Command {
    #[cfg(windows)]
    {
        let extension = Path::new(program)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some("bat") | Some("cmd") => {
                let mut command = Command::new("cmd.exe");
                command.args(["/d", "/s", "/c"]);
                command
                    .as_std_mut()
                    .raw_arg(windows_batch_command_line(program, args));
                command.creation_flags(windows_hidden_creation_flags());
                return command;
            }
            Some("ps1") => {
                let shell = which::which("pwsh")
                    .or_else(|_| which::which("powershell"))
                    .unwrap_or_else(|_| std::path::PathBuf::from("powershell.exe"));
                let mut command = Command::new(shell);
                command
                    .args([
                        "-NoLogo",
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File",
                        windows_command_path(program).as_str(),
                    ])
                    .args(args);
                command.creation_flags(windows_hidden_creation_flags());
                return command;
            }
            _ => {}
        }
    }

    let mut command = Command::new(program);
    command.args(args);
    #[cfg(windows)]
    command.creation_flags(windows_hidden_creation_flags());
    command
}

#[cfg(windows)]
pub(super) fn windows_batch_command_line(program: &str, args: &[String]) -> String {
    let mut command_line = String::from("call ");
    command_line.push_str(&windows_batch_token(&windows_command_path(program)));
    for arg in args {
        command_line.push(' ');
        command_line.push_str(&windows_batch_token(arg));
    }
    command_line
}

#[cfg(windows)]
fn windows_batch_token(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(super) fn platform_command_path(path: &Path) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        std::path::PathBuf::from(windows_command_path(&path.to_string_lossy()))
    }
    #[cfg(not(windows))]
    path.to_path_buf()
}

#[cfg(windows)]
fn windows_command_path(path: &str) -> String {
    path.strip_prefix("\\\\?\\").unwrap_or(path).to_string()
}
