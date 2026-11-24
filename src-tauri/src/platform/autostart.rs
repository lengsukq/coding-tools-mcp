use std::path::Path;

use crate::error::{AppError, AppResult};

pub const AUTOSTART_ARG: &str = "--autostart";
pub const LIGHTWEIGHT_ARG: &str = "--lightweight";

#[cfg(any(target_os = "macos", target_os = "linux", test))]
const AUTOSTART_ID: &str = "com.codingtools.mcp.desktop";

#[cfg(target_os = "windows")]
const WINDOWS_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const WINDOWS_RUN_VALUE_NAME: &str = "CodingToolsMCP";

/// Decide whether this process launch should start in lightweight (tray-only /
/// zero-WebView on macOS) mode.
pub fn should_start_lightweight(args: &[String], launch_in_lightweight_mode: bool) -> bool {
    let forced = args.iter().any(|arg| arg == LIGHTWEIGHT_ARG);
    let autostart = args.iter().any(|arg| arg == AUTOSTART_ARG);
    forced || (autostart && launch_in_lightweight_mode)
}

#[cfg(any(target_os = "macos", test))]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(any(target_os = "macos", test))]
fn render_macos_launch_agent_plist(exe_path: &Path) -> String {
    let escaped_exe = xml_escape(&exe_path.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{AUTOSTART_ID}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{escaped_exe}</string>
    <string>{AUTOSTART_ARG}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>ProcessType</key>
  <string>Interactive</string>
</dict>
</plist>
"#
    )
}

#[cfg(any(target_os = "windows", test))]
fn render_windows_run_command(exe_path: &Path) -> String {
    let raw = exe_path.to_string_lossy().replace('"', "");
    format!("\"{raw}\" {AUTOSTART_ARG}")
}

#[cfg(any(target_os = "linux", test))]
fn render_linux_desktop_entry(exe_path: &Path) -> String {
    let escaped = exe_path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        "[Desktop Entry]\nType=Application\nVersion=1.0\nName=Coding Tools MCP\nComment=Local AI Coding Workspace and MCP Runtime\nExec=\"{escaped}\" {AUTOSTART_ARG}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(target_os = "macos")]
fn macos_plist_path() -> AppResult<std::path::PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::Message("无法定位用户主目录，无法配置 macOS 开机自启".into()))?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{AUTOSTART_ID}.plist")))
}

#[cfg(target_os = "linux")]
fn linux_desktop_entry_path() -> AppResult<std::path::PathBuf> {
    let config_dir = dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
        .ok_or_else(|| AppError::Message("无法定位用户配置目录，无法配置开机自启".into()))?;
    Ok(config_dir
        .join("autostart")
        .join(format!("{AUTOSTART_ID}.desktop")))
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn current_executable_path() -> AppResult<std::path::PathBuf> {
    let exe = std::env::current_exe()
        .map_err(|err| AppError::Message(format!("无法获取当前应用可执行路径: {err}")))?;
    Ok(exe.canonicalize().unwrap_or(exe))
}

#[cfg(target_os = "macos")]
pub fn is_enabled() -> bool {
    macos_plist_path()
        .map(|path| path.is_file())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
pub fn set_enabled(enabled: bool) -> AppResult<()> {
    let plist_path = macos_plist_path()?;
    if !enabled {
        match std::fs::remove_file(&plist_path) {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => {
                return Err(AppError::Message(format!(
                    "移除 macOS 开机自启配置失败（{}）: {err}",
                    plist_path.display()
                )));
            }
        }
    }

    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            AppError::Message(format!(
                "创建 LaunchAgents 目录失败（{}）: {err}",
                parent.display()
            ))
        })?;
    }
    let exe = current_executable_path()?;
    let content = render_macos_launch_agent_plist(&exe);
    crate::util::fs::atomic_write_bytes(&plist_path, content.as_bytes()).map_err(|err| {
        AppError::Message(format!(
            "保存 macOS 开机自启配置失败（{}）: {err}",
            plist_path.display()
        ))
    })
}

#[cfg(target_os = "windows")]
pub fn is_enabled() -> bool {
    crate::platform::background_command("reg")
        .args(["query", WINDOWS_RUN_KEY, "/v", WINDOWS_RUN_VALUE_NAME])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
pub fn set_enabled(enabled: bool) -> AppResult<()> {
    if !enabled {
        if !is_enabled() {
            return Ok(());
        }
        let output = crate::platform::background_command("reg")
            .args([
                "delete",
                WINDOWS_RUN_KEY,
                "/v",
                WINDOWS_RUN_VALUE_NAME,
                "/f",
            ])
            .output()
            .map_err(|err| AppError::Message(format!("移除 Windows 开机自启项失败: {err}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Message(format!(
                "移除 Windows 开机自启项失败: {}",
                stderr.trim()
            )));
        }
        return Ok(());
    }

    let exe = current_executable_path()?;
    let value = render_windows_run_command(&exe);
    let output = crate::platform::background_command("reg")
        .args([
            "add",
            WINDOWS_RUN_KEY,
            "/v",
            WINDOWS_RUN_VALUE_NAME,
            "/t",
            "REG_SZ",
            "/d",
            &value,
            "/f",
        ])
        .output()
        .map_err(|err| AppError::Message(format!("写入 Windows 开机自启项失败: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Message(format!(
            "写入 Windows 开机自启项失败: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn is_enabled() -> bool {
    linux_desktop_entry_path()
        .map(|path| path.is_file())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
pub fn set_enabled(enabled: bool) -> AppResult<()> {
    let entry_path = linux_desktop_entry_path()?;
    if !enabled {
        match std::fs::remove_file(&entry_path) {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => {
                return Err(AppError::Message(format!(
                    "移除 Linux 开机自启文件失败（{}）: {err}",
                    entry_path.display()
                )));
            }
        }
    }

    if let Some(parent) = entry_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            AppError::Message(format!(
                "创建 autostart 目录失败（{}）: {err}",
                parent.display()
            ))
        })?;
    }
    let exe = current_executable_path()?;
    let content = render_linux_desktop_entry(&exe);
    std::fs::write(&entry_path, content).map_err(|err| {
        AppError::Message(format!(
            "写入 Linux 开机自启文件失败（{}）: {err}",
            entry_path.display()
        ))
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn set_enabled(enabled: bool) -> AppResult<()> {
    if enabled {
        Err(AppError::Message("当前操作系统暂不支持开机自动启动".into()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn lightweight_startup_triggers_only_on_autostart_setting_or_explicit_flag() {
        let manual = vec!["coding-tools-mcp".into()];
        let autostart = vec!["coding-tools-mcp".into(), AUTOSTART_ARG.into()];
        let explicit = vec!["coding-tools-mcp".into(), LIGHTWEIGHT_ARG.into()];

        assert!(!should_start_lightweight(&manual, false));
        assert!(!should_start_lightweight(&manual, true));
        assert!(!should_start_lightweight(&autostart, false));
        assert!(should_start_lightweight(&autostart, true));
        assert!(should_start_lightweight(&explicit, false));
    }

    #[test]
    fn macos_plist_escapes_xml_and_passes_autostart_arg() {
        let plist = render_macos_launch_agent_plist(Path::new(
            "/Applications/Coding & Tools <MCP>.app/Contents/MacOS/coding-tools-mcp-desktop",
        ));
        assert!(plist.contains("<string>com.codingtools.mcp.desktop</string>"));
        assert!(plist.contains(
            "/Applications/Coding &amp; Tools &lt;MCP&gt;.app/Contents/MacOS/coding-tools-mcp-desktop"
        ));
        assert!(plist.contains("<string>--autostart</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
    }

    #[test]
    fn windows_and_linux_autostart_entries_quote_paths_and_append_flag() {
        let win = render_windows_run_command(Path::new(
            r"C:\Program Files\Coding Tools MCP\coding-tools-mcp-desktop.exe",
        ));
        assert_eq!(
            win,
            r#""C:\Program Files\Coding Tools MCP\coding-tools-mcp-desktop.exe" --autostart"#
        );

        let linux = render_linux_desktop_entry(Path::new("/opt/Coding Tools/coding-tools-mcp"));
        assert!(linux.contains("Exec=\"/opt/Coding Tools/coding-tools-mcp\" --autostart"));
        assert!(linux.contains("X-GNOME-Autostart-enabled=true"));
    }
}
