use super::*;

use super::health::{log_file_len, stream_frpc_logs, wait_for_frpc_ready};
use super::managed::write_managed_frpc_pid;

pub async fn spawn_frpc(
    workspace_id: &str,
    routes: &[&WorkspaceProfile],
    settings: &crate::settings::AppSettings,
) -> AppResult<FrpcHandle> {
    let Some(first_profile) = routes.first().copied() else {
        return Err(AppError::Message("没有可启动的 FRP 线路。".into()));
    };
    let frpc = ensure_frpc().await?;

    let configs: Vec<FrpServerConfig> = routes
        .iter()
        .map(|profile| frp_server_config(profile, settings, None))
        .collect();
    for config in &configs {
        validate_frp_config(config)?;
    }

    let config_path = managed_frpc_config_path(workspace_id)?;
    let log_paths: Vec<PathBuf> = routes
        .iter()
        .map(|profile| -> AppResult<PathBuf> {
            let profile_log_dir = log_dir_for_profile(&profile.id);
            std::fs::create_dir_all(&profile_log_dir)?;
            Ok(profile_log_dir.join(frpc_log_name()))
        })
        .collect::<AppResult<Vec<_>>>()?;
    let log_path = log_paths
        .first()
        .cloned()
        .ok_or_else(|| AppError::Message("没有可写入的 frpc 日志路径。".into()))?;
    let config_text = build_frpc_toml_for_routes(&configs);
    std::fs::write(&config_path, &config_text)?;
    let log_offset = log_file_len(&log_path);

    let mut cmd = Command::new(&frpc);
    cmd.args(["-c", config_path.to_string_lossy().as_ref()]);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.current_dir(&first_profile.path);

    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        // frpc 是控制台程序。显式禁止创建控制台，避免安装版或开发版
        // 从桌面应用启动时短暂闪出黑色窗口。
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }

    #[cfg(unix)]
    {
        cmd.process_group(0);
    }

    // 一个聚合 frpc 只有一套进程环境，不能按工作区分别设置代理。
    // 任一路由要求使用代理时，为整个聚合连接启用代理；这样 HashMap
    // 的迭代顺序不会随机决定最终行为，也不会因偏好不同丢弃其它路由。
    let use_proxy = aggregate_uses_proxy(routes);
    if use_proxy {
        crate::tunnel::cloudflare::apply_proxy_env(&mut cmd, &settings.proxy);
    }

    let mut child = cmd
        .spawn()
        .map_err(|err| AppError::Message(format!("启动 frpc 失败: {err}")))?;
    let pid = child.id();
    if let Some(pid) = pid {
        if let Err(error) = write_managed_frpc_pid(workspace_id, pid, &frpc) {
            let _ = stop_child(child, Some(pid)).await;
            return Err(error);
        }
    }
    if let Some(stdout) = child.stdout.take() {
        let log_paths = log_paths.clone();
        tokio::spawn(async move {
            stream_frpc_logs(stdout, log_paths).await;
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let log_paths = log_paths.clone();
        tokio::spawn(async move {
            stream_frpc_logs(stderr, log_paths).await;
        });
    }

    let ready = match wait_for_frpc_ready(&mut child, &log_path, log_offset, configs.len()).await {
        Ok(ready) => ready,
        Err(error) => {
            let _ = stop_child(child, pid).await;
            clear_managed_frpc_pid(workspace_id);
            return Err(error);
        }
    };
    if !ready {
        let _ = stop_child(child, pid).await;
        clear_managed_frpc_pid(workspace_id);
        return Err(AppError::Message(
            "frpc 已启动但很快退出。请检查 FRP 服务器地址、端口、Token 与子域名配置。".into(),
        ));
    }

    Ok(FrpcHandle { child, pid })
}

pub(super) fn aggregate_uses_proxy(routes: &[&WorkspaceProfile]) -> bool {
    routes.iter().any(|profile| profile.tunnel.use_proxy)
}

fn validate_frp_config(config: &FrpServerConfig) -> AppResult<()> {
    if config.server_addr.trim().is_empty() {
        return Err(AppError::Message("FRP 模式需要填写服务器域名。".into()));
    }
    if config.proxy.subdomain.trim().is_empty() {
        return Err(AppError::Message("FRP 模式需要填写子域名。".into()));
    }
    if config.server_port == 0 {
        return Err(AppError::Message("FRP 服务器端口无效。".into()));
    }
    Ok(())
}
