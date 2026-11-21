use std::collections::HashSet;

use super::*;

#[derive(Clone, serde::Serialize)]
pub struct DetectedIde {
    id: &'static str,
    name: &'static str,
}

struct IdeDefinition {
    id: &'static str,
    name: &'static str,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    mac_app: &'static str,
    cli: &'static str,
}

const IDE_DEFINITIONS: &[IdeDefinition] = &[
    IdeDefinition {
        id: "vscode",
        name: "Visual Studio Code",
        mac_app: "Visual Studio Code",
        cli: "code",
    },
    IdeDefinition {
        id: "cursor",
        name: "Cursor",
        mac_app: "Cursor",
        cli: "cursor",
    },
    IdeDefinition {
        id: "windsurf",
        name: "Windsurf",
        mac_app: "Windsurf",
        cli: "windsurf",
    },
    IdeDefinition {
        id: "webstorm",
        name: "WebStorm",
        mac_app: "WebStorm",
        cli: "webstorm",
    },
    IdeDefinition {
        id: "idea",
        name: "IntelliJ IDEA",
        mac_app: "IntelliJ IDEA",
        cli: "idea",
    },
    IdeDefinition {
        id: "pycharm",
        name: "PyCharm",
        mac_app: "PyCharm",
        cli: "pycharm",
    },
    IdeDefinition {
        id: "android-studio",
        name: "Android Studio",
        mac_app: "Android Studio",
        cli: "studio",
    },
    IdeDefinition {
        id: "zed",
        name: "Zed",
        mac_app: "Zed",
        cli: "zed",
    },
];

#[cfg(target_os = "macos")]
fn mac_app_installed(mac_app: &str) -> bool {
    let app = format!("{mac_app}.app");
    PathBuf::from("/Applications").join(&app).exists()
        || dirs::home_dir().is_some_and(|home| home.join("Applications").join(&app).exists())
}

fn native_cli_path(cli: &str) -> Option<PathBuf> {
    which::which(cli).ok()
}

/// Probe multiple CLI names in a single login shell invocation for GUI apps
/// launched on macOS/Linux whose inherited `PATH` omits shell-configured dirs.
#[cfg(not(target_os = "windows"))]
fn batch_shell_probe_clis(clis: &[&str]) -> HashSet<String> {
    if clis.is_empty() {
        return HashSet::new();
    }
    let list = clis.join(" ");
    let script = format!(
        "for c in {list}; do command -v \"$c\" >/dev/null 2>&1 && printf '%s\\n' \"$c\"; done"
    );
    let Ok(output) = Command::new("sh").args(["-lc", &script]).output() else {
        return HashSet::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn resolve_cli_via_login_shell(cli: &str) -> Option<PathBuf> {
    let script = format!("command -v {cli}");
    let output = Command::new("sh").args(["-lc", &script]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!resolved.is_empty()).then(|| PathBuf::from(resolved))
}

fn detect_installed_ides_blocking() -> Vec<DetectedIde> {
    let mut installed_ids: HashSet<&'static str> = HashSet::new();
    let mut unchecked_clis: Vec<&'static str> = Vec::new();

    for ide in IDE_DEFINITIONS {
        #[cfg(target_os = "macos")]
        if mac_app_installed(ide.mac_app) {
            installed_ids.insert(ide.id);
            continue;
        }
        if native_cli_path(ide.cli).is_some() {
            installed_ids.insert(ide.id);
        } else {
            unchecked_clis.push(ide.cli);
        }
    }

    #[cfg(not(target_os = "windows"))]
    if !unchecked_clis.is_empty() {
        let shell_hits = batch_shell_probe_clis(&unchecked_clis);
        for ide in IDE_DEFINITIONS {
            if shell_hits.contains(ide.cli) {
                installed_ids.insert(ide.id);
            }
        }
    }

    IDE_DEFINITIONS
        .iter()
        .filter(|ide| installed_ids.contains(ide.id))
        .map(|ide| DetectedIde {
            id: ide.id,
            name: ide.name,
        })
        .collect()
}

#[tauri::command]
pub async fn detect_installed_ides() -> AppResult<Vec<DetectedIde>> {
    tauri::async_runtime::spawn_blocking(detect_installed_ides_blocking)
        .await
        .map_err(|err| AppError::Message(format!("检测 IDE 失败: {err}")))
}

#[tauri::command]
pub async fn open_workspace_in_ide(
    state: State<'_, AppState>,
    workspace_id: String,
    ide_id: String,
) -> AppResult<()> {
    let root = state.workspace_path(&workspace_id)?;
    let ide = IDE_DEFINITIONS
        .iter()
        .find(|ide| ide.id == ide_id)
        .ok_or_else(|| AppError::Message(format!("不支持的 IDE: {ide_id}")))?;

    tauri::async_runtime::spawn_blocking(move || {
        if !root.is_dir() {
            return Err(AppError::Message(format!(
                "工作区目录不存在: {}",
                root.display()
            )));
        }

        #[cfg(target_os = "macos")]
        if mac_app_installed(ide.mac_app) {
            return Command::new("open")
                .args(["-a", ide.mac_app])
                .arg(&root)
                .spawn()
                .map(|_| ())
                .map_err(|err| {
                    AppError::Message(format!("无法使用 {} 打开工作区: {err}", ide.name))
                });
        }

        let cli_path = native_cli_path(ide.cli);
        #[cfg(not(target_os = "windows"))]
        let cli_path = cli_path.or_else(|| resolve_cli_via_login_shell(ide.cli));

        let Some(executable) = cli_path else {
            return Err(AppError::Message(format!("未检测到 {}", ide.name)));
        };

        background_command(&executable)
            .arg(&root)
            .spawn()
            .map(|_| ())
            .map_err(|err| AppError::Message(format!("无法使用 {} 打开工作区: {err}", ide.name)))
    })
    .await
    .map_err(|err| AppError::Message(format!("打开 IDE 任务失败: {err}")))?
}
