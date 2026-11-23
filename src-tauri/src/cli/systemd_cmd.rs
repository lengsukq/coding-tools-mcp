use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SYSTEMD_SERVICE_NAME: &str = "coding-tools-mcp.service";

pub fn render_systemd_user_unit(exe_path: &Path) -> String {
    let escaped = exe_path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        "\
[Unit]
Description=Coding Tools MCP Headless Runtime
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=\"{escaped}\" serve --lan
Restart=on-failure
RestartSec=3
Environment=RUST_BACKTRACE=1

[Install]
WantedBy=default.target
"
    )
}

fn systemd_user_unit_path() -> Result<PathBuf, String> {
    let base = dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .ok_or_else(|| "无法定位用户 ~/.config 目录".to_string())?;
    Ok(base.join("systemd").join("user").join(SYSTEMD_SERVICE_NAME))
}

pub fn cmd_service(args: &[String]) -> Result<(), String> {
    let action = args.first().map(String::as_str).unwrap_or("status");
    let exe = std::env::current_exe()
        .map(|p| p.canonicalize().unwrap_or(p))
        .map_err(|e| format!("无法获取当前可执行文件路径: {e}"))?;

    match action {
        "print" => {
            print!("{}", render_systemd_user_unit(&exe));
            Ok(())
        }
        "install" => {
            if !cfg!(target_os = "linux") {
                return Err(
                    "`service install` 仅面向 Linux systemd 环境；桌面系统可在设置中开启「开机自动启动」。"
                        .into(),
                );
            }
            let unit_path = systemd_user_unit_path()?;
            if let Some(parent) = unit_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("创建 systemd user 目录失败: {e}"))?;
            }
            fs::write(&unit_path, render_systemd_user_unit(&exe))
                .map_err(|e| format!("写入 {} 失败: {e}", unit_path.display()))?;
            println!("已写入服务文件: {}", unit_path.display());

            let reload = Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();
            let enable = Command::new("systemctl")
                .args(["--user", "enable", "--now", SYSTEMD_SERVICE_NAME])
                .status();
            match (reload, enable) {
                (Ok(r), Ok(e)) if r.success() && e.success() => {
                    println!("已启用并启动用户级服务 `{SYSTEMD_SERVICE_NAME}`。");
                    println!("提示：若需在 SSH 退出后或服务器无人登录时保持常驻，可执行 `sudo loginctl enable-linger $USER`。");
                }
                _ => {
                    println!(
                        "服务单元文件已生成，但当前会话无法直接连接 `systemctl --user` 总线；可手动执行：\n  systemctl --user daemon-reload\n  systemctl --user enable --now {SYSTEMD_SERVICE_NAME}"
                    );
                }
            }
            Ok(())
        }
        "uninstall" | "remove" => {
            if cfg!(target_os = "linux") {
                let _ = Command::new("systemctl")
                    .args(["--user", "disable", "--now", SYSTEMD_SERVICE_NAME])
                    .status();
            }
            let unit_path = systemd_user_unit_path()?;
            if unit_path.exists() {
                fs::remove_file(&unit_path)
                    .map_err(|e| format!("删除 {} 失败: {e}", unit_path.display()))?;
                if cfg!(target_os = "linux") {
                    let _ = Command::new("systemctl")
                        .args(["--user", "daemon-reload"])
                        .status();
                }
                println!("已卸载服务文件: {}", unit_path.display());
            } else {
                println!("未检测到已安装的 systemd 用户服务文件。");
            }
            Ok(())
        }
        "status" => {
            let unit_path = systemd_user_unit_path()?;
            println!(
                "单元文件: {} ({})",
                unit_path.display(),
                if unit_path.is_file() {
                    "已安装"
                } else {
                    "未安装"
                }
            );
            if cfg!(target_os = "linux") {
                if let Ok(output) = Command::new("systemctl")
                    .args(["--user", "is-active", SYSTEMD_SERVICE_NAME])
                    .output()
                {
                    println!(
                        "systemd 状态: {}",
                        String::from_utf8_lossy(&output.stdout).trim()
                    );
                }
            }
            Ok(())
        }
        other => Err(format!(
            "未知 `service` 动作 `{other}`（支持: install, uninstall, status, print）"
        )),
    }
}
