use std::sync::LazyLock;

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use crate::data::DataStore;
use crate::error::AppResult;
use crate::platform::platform;
use crate::settings::AppSettings;
use crate::workspace::WorkspaceProfile;

use super::{TunnelServiceKind, TunnelSupervisor};

static TUNNEL_SUPERVISOR: LazyLock<Mutex<TunnelSupervisor>> =
    LazyLock::new(|| Mutex::new(TunnelSupervisor::new()));
static FRP_HEALTH_LOOP_STARTED: AtomicBool = AtomicBool::new(false);

const FRP_HEALTH_INTERVAL: Duration = Duration::from_secs(20);

pub fn supervisor() -> &'static Mutex<TunnelSupervisor> {
    &TUNNEL_SUPERVISOR
}

/// Start a background loop that restarts stuck FRP clients (process alive, proxy dead).
pub fn ensure_frp_health_loop() {
    if FRP_HEALTH_LOOP_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async {
        loop {
            sleep(FRP_HEALTH_INTERVAL).await;
            let settings = AppSettings::load_or_default();
            let mut guard = supervisor().lock().await;
            let _ = guard.heal_unhealthy_frpc(&settings).await;
        }
    });
}

fn tunnel_type_for(profile: &WorkspaceProfile, kind: TunnelServiceKind) -> &str {
    match kind {
        TunnelServiceKind::Mcp if profile.tunnel.use_global_gateway => "none",
        TunnelServiceKind::Actions if profile.actions.use_global_gateway => "none",
        TunnelServiceKind::Mcp => profile.tunnel.tunnel_type.as_str(),
        TunnelServiceKind::Actions => profile.actions.tunnel_type.as_str(),
    }
}

pub async fn maybe_start_for_runtime(
    profile: &WorkspaceProfile,
    kind: TunnelServiceKind,
) -> AppResult<Option<String>> {
    let tunnel_type = tunnel_type_for(profile, kind);
    if tunnel_type.is_empty() || tunnel_type == "none" {
        return Ok(None);
    }
    let settings = AppSettings::load_or_default();
    let mut guard = supervisor().lock().await;
    let status = guard.start(profile, kind, &settings).await?;
    Ok(Some(status.public_url))
}

pub async fn stop_for_runtime(
    profile: &WorkspaceProfile,
    kind: TunnelServiceKind,
) -> AppResult<()> {
    if matches!(kind, TunnelServiceKind::Mcp) && profile.tunnel.use_global_gateway
        || matches!(kind, TunnelServiceKind::Actions) && profile.actions.use_global_gateway
    {
        return Ok(());
    }
    let settings = AppSettings::load_or_default();
    let mut guard = supervisor().lock().await;
    guard.stop(profile, kind, &settings).await
}

pub async fn drop_workspace(workspace_id: &str) -> AppResult<()> {
    let mut guard = supervisor().lock().await;
    guard.drop_workspace(workspace_id).await
}

pub async fn sync_managed_runtime_routes(
    active_runtime_keys: HashSet<(String, TunnelServiceKind)>,
) -> AppResult<()> {
    let settings = AppSettings::load_or_default();
    let profiles = DataStore::read_file(|data| Ok(data.profiles.clone()))?;
    let mut guard = supervisor().lock().await;
    guard.restore_active_frp_routes(&profiles, &active_runtime_keys, &settings);
    Ok(())
}

pub async fn cleanup_orphan_for_runtime(
    profile: &WorkspaceProfile,
    kind: TunnelServiceKind,
    runtime_listening: bool,
) -> AppResult<()> {
    let port = match kind {
        TunnelServiceKind::Mcp => profile.runtime.local_port,
        TunnelServiceKind::Actions => profile.actions.local_port,
    };
    if runtime_listening || platform().find_pid_listening_on_port(port)?.is_some() {
        return Ok(());
    }
    let mut guard = supervisor().lock().await;
    // 等待 supervisor 锁期间 runtime 可能已经恢复，再确认一次才允许删除 route。
    if platform().find_pid_listening_on_port(port)?.is_some() {
        return Ok(());
    }
    guard.cleanup_orphan(profile, kind, false).await
}
