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

fn command_exists(command: &str) -> bool {
    #[cfg(target_os = "windows")]
    let probe = background_command("where").arg(command).output();
    #[cfg(not(target_os = "windows"))]
    let probe = Command::new("sh")
        .args(["-lc", &format!("command -v {command}")])
        .output();
    probe.map(|output| output.status.success()).unwrap_or(false)
}

fn ide_installed(ide: &IdeDefinition) -> bool {
    #[cfg(target_os = "macos")]
    {
        let app = format!("{}.app", ide.mac_app);
        if PathBuf::from("/Applications").join(&app).exists()
            || dirs::home_dir().is_some_and(|home| home.join("Applications").join(&app).exists())
        {
            return true;
        }
    }
    command_exists(ide.cli)
}

#[tauri::command]
pub async fn detect_installed_ides() -> AppResult<Vec<DetectedIde>> {
    tauri::async_runtime::spawn_blocking(|| {
        IDE_DEFINITIONS
            .iter()
            .filter(|ide| ide_installed(ide))
            .map(|ide| DetectedIde {
                id: ide.id,
                name: ide.name,
            })
            .collect()
    })
    .await
    .map_err(|err| AppError::Message(format!("检测 IDE 失败: {err}")))
}

#[tauri::command]
pub fn open_workspace_in_ide(
    state: State<'_, AppState>,
    workspace_id: String,
    ide_id: String,
) -> AppResult<()> {
    let profile = state.with_workspaces(|store| {
        store
            .get(&workspace_id)
            .cloned()
            .ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}")))
    })?;
    let root = PathBuf::from(profile.path);
    if !root.is_dir() {
        return Err(AppError::Message(format!(
            "工作区目录不存在: {}",
            root.display()
        )));
    }
    let ide = IDE_DEFINITIONS
        .iter()
        .find(|ide| ide.id == ide_id)
        .ok_or_else(|| AppError::Message(format!("不支持的 IDE: {ide_id}")))?;
    if !ide_installed(ide) {
        return Err(AppError::Message(format!("未检测到 {}", ide.name)));
    }

    #[cfg(target_os = "macos")]
    if PathBuf::from("/Applications")
        .join(format!("{}.app", ide.mac_app))
        .exists()
        || dirs::home_dir().is_some_and(|home| {
            home.join("Applications")
                .join(format!("{}.app", ide.mac_app))
                .exists()
        })
    {
        return Command::new("open")
            .args(["-a", ide.mac_app])
            .arg(&root)
            .spawn()
            .map(|_| ())
            .map_err(|err| AppError::Message(format!("无法使用 {} 打开工作区: {err}", ide.name)));
    }
    background_command(ide.cli)
        .arg(&root)
        .spawn()
        .map(|_| ())
        .map_err(|err| AppError::Message(format!("无法使用 {} 打开工作区: {err}", ide.name)))
}
