use super::*;

pub fn resolve_frpc() -> AppResult<PathBuf> {
    bundled_frpc()
        .or_else(|| {
            platform()
                .frpc_candidates()
                .into_iter()
                .find(|path| path.is_file())
        })
        .or_else(|| cached_frpc_path().filter(|path| path.is_file()))
        .ok_or_else(|| {
            AppError::Message(
                "未找到 frpc。连接隧道时将尝试自动下载；也可自行安装 frp 客户端。".into(),
            )
        })
}

pub async fn ensure_frpc() -> AppResult<PathBuf> {
    if let Ok(path) = resolve_frpc() {
        return Ok(path);
    }
    download_frpc_to_cache().await
}

fn bundled_frpc() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    #[cfg(windows)]
    let names = ["frpc.exe"];
    #[cfg(not(windows))]
    let names = ["frpc"];
    for name in names {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn cached_frpc_path() -> Option<PathBuf> {
    platform()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("bin").join(frpc_binary_name()))
}

pub(crate) fn frpc_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "frpc.exe"
    }
    #[cfg(not(windows))]
    {
        "frpc"
    }
}

pub(crate) async fn download_frpc_to_cache() -> AppResult<PathBuf> {
    let settings = crate::settings::AppSettings::load_or_default();
    let (archive_name, binary_in_archive) = frp_release_asset()?;
    let url =
        format!("https://github.com/fatedier/frp/releases/download/v{FRP_VERSION}/{archive_name}");
    let cache_dir = platform().app_config_dir()?.join("bin").join("downloads");
    std::fs::create_dir_all(&cache_dir)?;
    let archive_path = cache_dir.join(archive_name);
    let dest = cached_frpc_path().expect("cache path");

    if !archive_path.is_file() {
        let bytes =
            crate::tunnel::download::download_release_asset(&settings, &url, "frpc").await?;
        std::fs::write(&archive_path, bytes)?;
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if archive_name.ends_with(".zip") {
        extract_frpc_from_zip(&archive_path, &dest, binary_in_archive)?;
    } else {
        extract_frpc_from_tar_gz(&archive_path, &dest, binary_in_archive)?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&dest) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&dest, perms);
        }
    }

    if dest.is_file() {
        Ok(dest)
    } else {
        Err(AppError::Message("frpc 自动安装失败。".into()))
    }
}

fn extract_frpc_from_zip(archive_path: &Path, dest: &Path, binary_suffix: &str) -> AppResult<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|err| AppError::Message(format!("解压 frpc 安装包失败: {err}")))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|err| AppError::Message(format!("读取 frpc 安装包失败: {err}")))?;
        let name = entry.name().replace('\\', "/");
        if name.ends_with(binary_suffix) || name.ends_with("frpc") || name.ends_with("frpc.exe") {
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    Err(AppError::Message(
        "frpc 安装包中未找到 frpc 可执行文件。".into(),
    ))
}

fn extract_frpc_from_tar_gz(
    archive_path: &Path,
    dest: &Path,
    binary_suffix: &str,
) -> AppResult<()> {
    let file = std::fs::File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive
        .entries()
        .map_err(|err| AppError::Message(format!("解压 frpc 安装包失败: {err}")))?
    {
        let mut entry =
            entry.map_err(|err| AppError::Message(format!("读取 frpc 安装包失败: {err}")))?;
        let path = entry
            .path()
            .map_err(|err| AppError::Message(err.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if path.ends_with(binary_suffix) || path.ends_with("/frpc") {
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    Err(AppError::Message(
        "frpc 安装包中未找到 frpc 可执行文件。".into(),
    ))
}

fn frp_release_asset() -> AppResult<(&'static str, &'static str)> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Ok(("frp_0.61.2_windows_amd64.zip", "frpc.exe"))
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Ok(("frp_0.61.2_linux_amd64.tar.gz", "frpc"))
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        Ok(("frp_0.61.2_linux_arm64.tar.gz", "frpc"))
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Ok(("frp_0.61.2_darwin_amd64.tar.gz", "frpc"))
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Ok(("frp_0.61.2_darwin_arm64.tar.gz", "frpc"))
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
    )))]
    {
        Err(AppError::Message("当前平台暂不支持自动下载 frpc。".into()))
    }
}
