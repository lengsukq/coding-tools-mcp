use std::path::Path;

use tokio::process::Command;

use crate::error::{AppError, AppResult};

/// Configure a tunnel subprocess (`cloudflared`, `frpc`) to run in its own
/// process group without flashing a console window on Windows.
pub(crate) fn configure_tunnel_command(cmd: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }

    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
}

/// Mark a downloaded tunnel binary executable on Unix, surfacing permission errors honestly.
pub(crate) fn set_executable_permissions(path: &Path, label: &str) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let meta = std::fs::metadata(path)
            .map_err(|err| AppError::Message(format!("读取 {label} 权限失败: {err}")))?;
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)
            .map_err(|err| AppError::Message(format!("设置 {label} 可执行权限失败: {err}")))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, label);
    }
    Ok(())
}

/// Extract the first matching binary entry from a `.tar.gz` / `.tgz` archive into `dest`.
pub(crate) fn extract_binary_from_tar_gz(
    archive_path: &Path,
    dest: &Path,
    label: &str,
    matches_entry: impl Fn(&str) -> bool,
) -> AppResult<()> {
    let file = std::fs::File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive
        .entries()
        .map_err(|err| AppError::Message(format!("解压 {label} 安装包失败: {err}")))?
    {
        let mut entry =
            entry.map_err(|err| AppError::Message(format!("读取 {label} 安装包失败: {err}")))?;
        let path = entry
            .path()
            .map_err(|err| AppError::Message(err.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if matches_entry(&path) {
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    Err(AppError::Message(format!(
        "{label} 安装包中未找到可执行文件。"
    )))
}
