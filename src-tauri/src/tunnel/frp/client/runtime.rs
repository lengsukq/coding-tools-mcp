use super::*;

use super::health::{frpc_log_name, log_file_len, stream_frpc_logs, wait_for_frpc_ready};
use super::managed::write_managed_frpc_pid;

pub async fn spawn_global_frpc(
    config: &GlobalGatewayConfig,
    settings: &AppSettings,
) -> AppResult<FrpcHandle> {
    let frpc = ensure_frpc().await?;
    let frp_config = global_frp_server_config(config, settings, None);
    validate_frp_config(&frp_config)?;

    let scope_id = "global-mcp";
    let config_path = managed_frpc_config_path(scope_id)?;
    let log_dir = log_dir_for_profile(scope_id);
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(frpc_log_name());
    let config_text = build_frpc_toml(&frp_config);
    std::fs::write(&config_path, &config_text)?;
    let log_offset = log_file_len(&log_path);

    let mut cmd = Command::new(&frpc);
    cmd.args(["-c", config_path.to_string_lossy().as_ref()]);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.current_dir(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

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

    if config.use_proxy {
        crate::tunnel::cloudflare::apply_proxy_env(&mut cmd, &settings.proxy);
    }

    let mut child = cmd
        .spawn()
        .map_err(|err| AppError::Message(format!("启动 frpc 失败: {err}")))?;
    let pid = child.id();
    if let Some(pid) = pid {
        if let Err(error) = write_managed_frpc_pid(scope_id, pid, &frpc) {
            let _ = stop_child(child, Some(pid)).await;
            return Err(error);
        }
    }
    if let Some(stdout) = child.stdout.take() {
        let log_paths = vec![log_path.clone()];
        tokio::spawn(async move {
            stream_frpc_logs(stdout, log_paths).await;
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let log_paths = vec![log_path.clone()];
        tokio::spawn(async move {
            stream_frpc_logs(stderr, log_paths).await;
        });
    }

    let ready = match wait_for_frpc_ready(&mut child, &log_path, log_offset, 1).await {
        Ok(ready) => ready,
        Err(error) => {
            let _ = stop_child(child, pid).await;
            clear_managed_frpc_pid(scope_id);
            return Err(error);
        }
    };
    if !ready {
        let _ = stop_child(child, pid).await;
        clear_managed_frpc_pid(scope_id);
        return Err(AppError::Message(
            "frpc 已启动但很快退出。请检查 FRP 服务器地址、端口、Token 与子域名配置。".into(),
        ));
    }

    Ok(FrpcHandle { child, pid })
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
