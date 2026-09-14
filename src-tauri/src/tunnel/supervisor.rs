use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use tokio::process::Child;
use tokio::time::{sleep, Duration, Instant};

use crate::error::{AppError, AppResult};
use crate::platform::platform;
use crate::secret::SecretStore;
use crate::settings::AppSettings;
use crate::workspace::WorkspaceProfile;

mod helpers;

pub use helpers::{append_profile_log, log_dir_for_profile};
use helpers::{
    cloudflare_config, proxy_already_exists, public_url_for_profile, tunnel_type_for,
    validate_tunnel_requirements,
};

use super::cloudflare::{self, CloudflareTunnelHandle};
use super::frp::{self, FrpServerConfig};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    pub state: String,
    pub public_url: String,
    pub tunnel_pid: Option<u32>,
}

struct TunnelSession {
    public_url: String,
    pid: Option<u32>,
    child: Option<Child>,
}

struct FrpRoute {
    profile: WorkspaceProfile,
}

struct FrpcProcess {
    child: Child,
    pid: Option<u32>,
}

#[derive(Default)]
struct FrpcHealthState {
    unhealthy_streak: u32,
    last_restart_at: Option<Instant>,
}

pub struct TunnelSupervisor {
    sessions: HashMap<String, TunnelSession>,
    frp_routes: HashMap<String, FrpRoute>,
    frpc: HashMap<String, FrpcProcess>,
    frpc_health: HashMap<String, FrpcHealthState>,
}

impl Default for TunnelSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

const FRPC_HEALTH_STREAK_TO_RESTART: u32 = 2;
const FRPC_HEALTH_RESTART_COOLDOWN: Duration = Duration::from_secs(90);

impl TunnelSupervisor {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            frp_routes: HashMap::new(),
            frpc: HashMap::new(),
            frpc_health: HashMap::new(),
        }
    }

    /// Probe active FRP workspaces; restart frpc when process is alive but proxy is dead.
    pub async fn heal_unhealthy_frpc(&mut self, settings: &AppSettings) -> usize {
        let workspace_ids: Vec<String> = self.frpc.keys().cloned().collect();
        let mut restarted = 0usize;
        for workspace_id in workspace_ids {
            let Some(reason) = self.diagnose_frpc_unhealthy(&workspace_id, settings).await else {
                if let Some(state) = self.frpc_health.get_mut(&workspace_id) {
                    state.unhealthy_streak = 0;
                }
                continue;
            };

            let state = self.frpc_health.entry(workspace_id.clone()).or_default();
            state.unhealthy_streak = state.unhealthy_streak.saturating_add(1);
            let cooled_down = state
                .last_restart_at
                .map(|at| at.elapsed() >= FRPC_HEALTH_RESTART_COOLDOWN)
                .unwrap_or(true);
            if state.unhealthy_streak < FRPC_HEALTH_STREAK_TO_RESTART || !cooled_down {
                continue;
            }

            append_profile_log(
                &workspace_id,
                "frpc-mcp.log",
                &format!("[health] auto-restarting frpc: {reason}"),
            );
            match self.restart_workspace_frpc(&workspace_id, settings).await {
                Ok(()) => {
                    if let Some(state) = self.frpc_health.get_mut(&workspace_id) {
                        state.unhealthy_streak = 0;
                        state.last_restart_at = Some(Instant::now());
                    }
                    restarted += 1;
                }
                Err(error) => {
                    append_profile_log(
                        &workspace_id,
                        "frpc-mcp.log",
                        &format!("[health] auto-restart failed: {error}"),
                    );
                }
            }
        }
        restarted
    }

    async fn diagnose_frpc_unhealthy(
        &self,
        workspace_id: &str,
        _settings: &AppSettings,
    ) -> Option<String> {
        let process_alive = self.frpc.get(workspace_id).is_some_and(|process| {
            process
                .pid
                .map(|pid| platform().is_process_alive(pid))
                .unwrap_or(true)
        });
        if !process_alive {
            return Some("frpc process exited".into());
        }

        let route = self.frp_routes.get(workspace_id)?;

        let log_path = log_dir_for_profile(workspace_id).join(frp::frpc_log_name());
        let log_tail = frp::read_frpc_log_tail(&log_path);
        if frp::frpc_reconnect_loop_detected(&log_tail) {
            return Some("frpc reconnect loop detected in log".into());
        }

        let public_url = route.profile.public_endpoint();
        if public_url.is_empty() {
            return None;
        }
        // Only treat FRP's own 404 page as proof the proxy is dead. Network
        // blips (Unreachable) alone must not force a restart.
        let local_ok = frp::probe_local_mcp_ok(route.profile.runtime.local_port).await;
        if !local_ok {
            return None;
        }
        if frp::probe_public_mcp_endpoint(&public_url).await == frp::PublicMcpProbe::FrpNotRouted {
            return Some(format!(
                "public endpoint returns FRP not-found page ({public_url})"
            ));
        }
        None
    }

    pub fn status(&self, profile: &WorkspaceProfile, settings: &AppSettings) -> TunnelStatus {
        let key = profile.id.as_str();
        if self.session_is_running(key) {
            if let Some(session) = self.sessions.get(key) {
                return TunnelStatus {
                    state: "running".into(),
                    public_url: session.public_url.clone(),
                    tunnel_pid: session.pid,
                };
            }
        }

        TunnelStatus {
            state: "stopped".into(),
            public_url: public_url_for_profile(profile, settings),
            tunnel_pid: None,
        }
    }

    pub fn route_profile(&self, workspace_id: &str) -> Option<WorkspaceProfile> {
        self.frp_routes
            .get(workspace_id)
            .map(|route| route.profile.clone())
    }

    pub async fn start(
        &mut self,
        profile: &WorkspaceProfile,
        settings: &AppSettings,
    ) -> AppResult<TunnelStatus> {
        let key = profile.id.clone();
        let tunnel_type = tunnel_type_for(profile);
        if self.session_is_running(&key) && tunnel_type != "frp" {
            return Ok(self.status(profile, settings));
        }

        // 暂存旧状态，直到新线路完成校验并成功启动。这样配置填写错误、
        // 线路冲突或 frpc 启动失败时，当前可用线路不会因为一次点击而丢失。
        // 对 FRP 来说，这也是“替换当前 Workspace 代理”而不是先删除再重建。
        let mut previous_session = self.sessions.remove(&key);
        let mut previous_route = self.frp_routes.remove(&key);

        if let Err(error) = validate_tunnel_requirements(profile, settings) {
            self.restore_route_state(&key, previous_route.take(), previous_session.take());
            return Err(error);
        }

        if tunnel_type == "frp" {
            let config = frp::frp_server_config(profile, settings, None);
            if let Err(error) =
                self.validate_frp_route_compatibility(&profile.id, &config, settings)
            {
                self.restore_route_state(&key, previous_route.take(), previous_session.take());
                return Err(error);
            }

            self.frp_routes.insert(
                key.clone(),
                FrpRoute {
                    profile: profile.clone(),
                },
            );
            if let Err(error) = self.ensure_frpc_matches_routes(&profile.id, settings).await {
                self.frp_routes.remove(&key);
                self.restore_route_state(&key, previous_route.take(), previous_session.take());
                if let Err(rollback_error) =
                    self.ensure_frpc_matches_routes(&profile.id, settings).await
                {
                    return Err(AppError::Message(format!(
                        "启动新的 FRP 线路失败，且恢复原有线路失败：{error}; rollback: {rollback_error}"
                    )));
                }
                return Err(error);
            }

            let public_url = frp::frp_public_url(profile, settings);
            let pid = self.frpc.get(&profile.id).and_then(|process| process.pid);
            self.sessions.insert(
                key,
                TunnelSession {
                    public_url: public_url.clone(),
                    pid,
                    child: None,
                },
            );
            return Ok(TunnelStatus {
                state: "running".into(),
                public_url,
                tunnel_pid: pid,
            });
        }

        if tunnel_type != "cloudflare" {
            self.restore_route_state(&key, previous_route.take(), previous_session.take());
            return Err(AppError::Message("当前仅支持 FRP 和 Cloudflare。".into()));
        }

        let (port, mode, token, named_url, log_name) = match cloudflare_config(profile) {
            Ok(config) => config,
            Err(error) => {
                self.restore_route_state(&key, previous_route.take(), previous_session.take());
                return Err(error);
            }
        };
        let use_proxy = profile.tunnel.use_proxy;
        let log_path = log_dir_for_profile(&profile.id).join(log_name);
        let handle = cloudflare::spawn_cloudflare_tunnel(
            port,
            std::path::Path::new(&profile.path),
            &log_path,
            mode,
            &token,
            &named_url,
            use_proxy,
        )
        .await
        .inspect_err(|_| {
            self.restore_route_state(&key, previous_route.take(), previous_session.take());
        })?;

        let CloudflareTunnelHandle {
            child,
            public_url,
            pid,
        } = handle;

        self.sessions.insert(
            key,
            TunnelSession {
                public_url: public_url.clone(),
                pid,
                child: Some(child),
            },
        );

        Ok(TunnelStatus {
            state: "running".into(),
            public_url,
            tunnel_pid: pid,
        })
    }

    pub async fn stop(
        &mut self,
        profile: &WorkspaceProfile,
        settings: &AppSettings,
    ) -> AppResult<()> {
        self.stop_internal(&profile.id, settings).await
    }

    async fn stop_internal(&mut self, workspace_id: &str, settings: &AppSettings) -> AppResult<()> {
        let key = workspace_id.to_string();
        if let Some(route) = self.frp_routes.remove(&key) {
            let session = self.sessions.remove(&key);
            if let Err(error) = self
                .ensure_frpc_matches_routes(workspace_id, settings)
                .await
            {
                self.frp_routes.insert(key.clone(), route);
                if let Some(session) = session {
                    self.sessions.insert(key, session);
                }
                if let Err(rollback_error) = self
                    .ensure_frpc_matches_routes(workspace_id, settings)
                    .await
                {
                    return Err(AppError::Message(format!(
                        "停止 FRP 线路失败，且恢复原有线路失败：{error}; rollback: {rollback_error}"
                    )));
                }
                return Err(error);
            }
            return Ok(());
        }

        let Some(mut session) = self.sessions.remove(&key) else {
            return Ok(());
        };

        if let Some(child) = session.child.take() {
            let _ = cloudflare::stop_child(child, session.pid).await;
        } else if let Some(pid) = session.pid {
            let _ = platform().terminate_process_tree(pid);
        }
        Ok(())
    }

    pub async fn drop_workspace(&mut self, workspace_id: &str) -> AppResult<()> {
        let settings = AppSettings::load_or_default();
        let key = workspace_id.to_string();

        // 非 FRP session 正常情况下必须持有 Child。先完成归属预检，再修改
        // FRP route；不能确认归属时保持所有线路原样，避免部分删除。
        if !self.frp_routes.contains_key(&key)
            && self
                .sessions
                .get(&key)
                .is_some_and(|session| session.child.is_none())
        {
            return Err(AppError::Message(format!(
                "无法确认工作区 {} 的 MCP 隧道进程归属，已取消删除。",
                workspace_id,
            )));
        }

        let removed_route = self.frp_routes.remove(&key);
        let removed_session = removed_route
            .as_ref()
            .and_then(|_| self.sessions.remove(&key));

        if let Some(route) = removed_route {
            if let Err(error) = self
                .ensure_frpc_matches_routes(workspace_id, &settings)
                .await
            {
                self.frp_routes.insert(key.clone(), route);
                if let Some(session) = removed_session {
                    self.sessions.insert(key.clone(), session);
                }
                if let Err(rollback_error) = self
                    .ensure_frpc_matches_routes(workspace_id, &settings)
                    .await
                {
                    return Err(AppError::Message(format!(
                        "删除工作区 FRP 线路失败，且恢复原有线路失败：{error}; rollback: {rollback_error}"
                    )));
                }
                return Err(error);
            }
        }

        if let Some(mut session) = self.sessions.remove(&key) {
            let child = session.child.take().ok_or_else(|| {
                AppError::Message("隧道进程归属状态在删除期间发生变化，已停止操作。".into())
            })?;
            cloudflare::stop_child(child, session.pid).await?;
        }

        Ok(())
    }

    /// Terminate a supervised tunnel when the local runtime is not listening.
    pub async fn cleanup_orphan(
        &mut self,
        profile: &WorkspaceProfile,
        runtime_listening: bool,
    ) -> AppResult<()> {
        if runtime_listening {
            return Ok(());
        }
        let settings = AppSettings::load_or_default();
        let key = profile.id.clone();
        if self.frp_routes.contains_key(&key) && !self.frp_route_matches(&key, profile, &settings) {
            // 清理任务携带的是旧 runtime/profile；当前 route 已被新的端口、
            // subdomain 或配置替换，不能按相同 workspace key 删除新线路。
            return Ok(());
        }
        self.stop_internal(&profile.id, &settings).await
    }

    pub fn restore_active_frp_routes(
        &mut self,
        profiles: &[WorkspaceProfile],
        active_runtime_keys: &HashSet<String>,
        settings: &AppSettings,
    ) {
        let mut changed_workspaces = HashSet::new();
        for profile in profiles {
            let key = profile.id.clone();
            if tunnel_type_for(profile) != "frp" || !active_runtime_keys.contains(&key) {
                continue;
            }

            match self.frp_routes.get_mut(&key) {
                Some(route) => {
                    route.profile = profile.clone();
                    changed_workspaces.insert(profile.id.clone());
                }
                None => {
                    self.frp_routes.insert(
                        key,
                        FrpRoute {
                            profile: profile.clone(),
                        },
                    );
                    changed_workspaces.insert(profile.id.clone());
                }
            }
        }

        changed_workspaces.extend(self.frpc.keys().cloned());
        for workspace_id in changed_workspaces {
            let pid = self.frpc.get(&workspace_id).and_then(|process| process.pid);
            self.sync_frp_sessions_for_workspace(settings, &workspace_id, pid);
        }
    }

    fn validate_frp_route_compatibility(
        &self,
        workspace_id: &str,
        config: &FrpServerConfig,
        settings: &AppSettings,
    ) -> AppResult<()> {
        if let Some(conflict) = self.frp_routes.values().find(|route| {
            let existing = frp::frp_server_config(&route.profile, settings, None);
            existing
                .proxy
                .subdomain
                .trim()
                .eq_ignore_ascii_case(config.proxy.subdomain.trim())
        }) {
            return Err(AppError::Message(format!(
                "FRP 子域名“{}”已被工作区“{}”的 {} 服务使用，不能重复。",
                config.proxy.subdomain.trim(),
                conflict.profile.name,
                "MCP"
            )));
        }

        let _ = workspace_id;
        Ok(())
    }

    async fn restart_workspace_frpc(
        &mut self,
        workspace_id: &str,
        settings: &AppSettings,
    ) -> AppResult<()> {
        // 工作区锁覆盖“停止旧进程 → 等待退出 → 启动新进程”的完整窗口，
        // 防止两个桌面实例同时管理同一工作区，同时允许不同工作区独立运行。
        let _operation_lock = frp::acquire_frpc_operation_lock(workspace_id).await?;

        if let Some(process) = self.frpc.remove(workspace_id) {
            let pid = process.pid;
            cloudflare::stop_child(process.child, pid).await?;
            if pid.is_some_and(|pid| platform().is_process_alive(pid)) {
                return Err(AppError::Message(format!(
                    "停止工作区 frpc 超时，PID {} 仍在运行。",
                    pid.unwrap_or_default()
                )));
            }
            frp::clear_managed_frpc_pid(workspace_id);
        }
        self.sync_frp_sessions_for_workspace(settings, workspace_id, None);

        // 仅回收当前工作区 PID 文件明确记录的 frpc。应用重启后即使
        // supervisor 丢失 Child，也不能按镜像路径批量终止其他工作区实例。
        frp::stop_recorded_frpc_instance(workspace_id).await?;

        if !self.frp_routes.contains_key(workspace_id) {
            return Ok(());
        }

        let route_specs = [self.frp_routes[workspace_id].profile.clone()];
        let route_refs: Vec<&WorkspaceProfile> = route_specs.iter().collect();
        let deadline = Instant::now() + Duration::from_secs(35);
        let handle = loop {
            match frp::spawn_frpc(workspace_id, &route_refs, settings).await {
                Ok(handle) => break handle,
                Err(error) if proxy_already_exists(&error) && Instant::now() < deadline => {
                    sleep(Duration::from_secs(1)).await;
                }
                Err(error) => return Err(error),
            }
        };
        let pid = handle.pid;
        self.frpc.insert(
            workspace_id.to_string(),
            FrpcProcess {
                child: handle.child,
                pid,
            },
        );
        self.sync_frp_sessions_for_workspace(settings, workspace_id, pid);
        Ok(())
    }

    async fn ensure_frpc_matches_routes(
        &mut self,
        workspace_id: &str,
        settings: &AppSettings,
    ) -> AppResult<()> {
        let has_routes = self.frp_routes.contains_key(workspace_id);
        if !has_routes {
            self.restart_workspace_frpc(workspace_id, settings).await?;
            return Ok(());
        }

        let route_specs = [self.frp_routes[workspace_id].profile.clone()];
        let route_refs: Vec<&WorkspaceProfile> = route_specs.iter().collect();
        let expected = frp::build_frpc_toml_for_route_refs(&route_refs, settings);

        let process_alive = self.frpc.get(workspace_id).is_some_and(|process| {
            process
                .pid
                .map(|pid| platform().is_process_alive(pid))
                .unwrap_or(true)
        });

        if process_alive && frp::managed_frpc_config_matches(workspace_id, &expected)? {
            let pid = self.frpc.get(workspace_id).and_then(|process| process.pid);
            self.sync_frp_sessions_for_workspace(settings, workspace_id, pid);
            return Ok(());
        }

        self.restart_workspace_frpc(workspace_id, settings).await
    }

    fn sync_frp_sessions_for_workspace(
        &mut self,
        settings: &AppSettings,
        workspace_id: &str,
        pid: Option<u32>,
    ) {
        let has_route = self.frp_routes.contains_key(workspace_id);
        self.sessions
            .retain(|key, session| key != workspace_id || session.child.is_some() || has_route);

        if let Some(route) = self.frp_routes.get(workspace_id) {
            let public_url = public_url_for_profile(&route.profile, settings);
            match self.sessions.get_mut(workspace_id) {
                Some(session) => {
                    session.public_url = public_url.clone();
                    session.pid = pid;
                }
                None => {
                    self.sessions.insert(
                        workspace_id.to_string(),
                        TunnelSession {
                            public_url,
                            pid,
                            child: None,
                        },
                    );
                }
            }
        }
    }

    fn restore_route_state(
        &mut self,
        key: &str,
        route: Option<FrpRoute>,
        session: Option<TunnelSession>,
    ) {
        if let Some(route) = route {
            self.frp_routes.insert(key.to_string(), route);
        }
        if let Some(session) = session {
            self.sessions.insert(key.to_string(), session);
        }
    }

    fn frp_route_matches(
        &self,
        key: &str,
        profile: &WorkspaceProfile,
        settings: &AppSettings,
    ) -> bool {
        let Some(route) = self.frp_routes.get(key) else {
            return false;
        };
        let existing = frp::frp_server_config(&route.profile, settings, None);
        let requested = frp::frp_server_config(profile, settings, None);
        existing == requested && route.profile.tunnel.use_proxy == profile.tunnel.use_proxy
    }

    fn session_is_running(&self, key: &str) -> bool {
        if self.frp_routes.contains_key(key) {
            let process_alive = self.frpc.get(key).is_some_and(|process| {
                process
                    .pid
                    .map(|pid| platform().is_process_alive(pid))
                    .unwrap_or(true)
            });
            return process_alive && self.sessions.contains_key(key);
        }
        self.sessions.get(key).is_some_and(|session| {
            session
                .pid
                .map(|pid| platform().is_process_alive(pid))
                .unwrap_or(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frp_profile(name: &str, subdomain: &str) -> WorkspaceProfile {
        let mut profile = WorkspaceProfile::new(format!("C:/workspace/{name}"), Some(name.into()));
        profile.tunnel.tunnel_type = "frp".into();
        profile.tunnel.frp_server = "frp.example.com".into();
        profile.tunnel.frp_server_port = 7000;
        profile.tunnel.frp_subdomain = subdomain.into();
        profile
    }

    #[test]
    fn active_routes_reject_duplicate_subdomains_case_insensitively() {
        let settings = AppSettings::default();
        let first = frp_profile("first", "shared");
        let second = frp_profile("second", "SHARED");
        let mut supervisor = TunnelSupervisor::new();
        supervisor
            .frp_routes
            .insert(first.id.clone(), FrpRoute { profile: first });

        let config = frp::frp_server_config(&second, &settings, None);
        let error = supervisor
            .validate_frp_route_compatibility(&second.id, &config, &settings)
            .unwrap_err();
        assert!(error.to_string().contains("不能重复"));
    }

    #[test]
    fn active_routes_allow_distinct_subdomains() {
        let settings = AppSettings::default();
        let first = frp_profile("first", "first");
        let second = frp_profile("second", "second");
        let mut supervisor = TunnelSupervisor::new();
        supervisor
            .frp_routes
            .insert(first.id.clone(), FrpRoute { profile: first });

        let config = frp::frp_server_config(&second, &settings, None);
        assert!(supervisor
            .validate_frp_route_compatibility(&second.id, &config, &settings)
            .is_ok());
    }

    #[test]
    fn active_routes_allow_mixed_proxy_preferences() {
        let settings = AppSettings::default();
        let mut direct = frp_profile("direct", "direct");
        direct.tunnel.use_proxy = false;
        let mut proxied = frp_profile("proxied", "proxied");
        proxied.tunnel.use_proxy = true;
        let mut supervisor = TunnelSupervisor::new();
        supervisor
            .frp_routes
            .insert(direct.id.clone(), FrpRoute { profile: direct });

        let config = frp::frp_server_config(&proxied, &settings, None);
        assert!(supervisor
            .validate_frp_route_compatibility(&proxied.id, &config, &settings)
            .is_ok());
    }

    #[test]
    fn different_workspaces_may_use_different_frp_servers() {
        let settings = AppSettings::default();
        let first = frp_profile("first", "first");
        let mut second = frp_profile("second", "second");
        second.tunnel.frp_server = "another-frp.example.com".into();
        let mut supervisor = TunnelSupervisor::new();
        supervisor
            .frp_routes
            .insert(first.id.clone(), FrpRoute { profile: first });

        let config = frp::frp_server_config(&second, &settings, None);
        assert!(supervisor
            .validate_frp_route_compatibility(&second.id, &config, &settings)
            .is_ok());
    }

    #[test]
    fn stale_profile_does_not_match_a_replaced_route() {
        let settings = AppSettings::default();
        let current = frp_profile("demo", "aa");
        let mut stale = current.clone();
        stale.tunnel.frp_subdomain = "a".into();
        let key = current.id.clone();
        let mut supervisor = TunnelSupervisor::new();
        supervisor
            .frp_routes
            .insert(key.clone(), FrpRoute { profile: current });

        assert!(!supervisor.frp_route_matches(&key, &stale, &settings));
    }

    #[test]
    fn restore_active_frp_routes_rehydrates_all_listening_workspaces() {
        let settings = AppSettings::default();
        let first = frp_profile("first", "gp");
        let second = frp_profile("second", "lb");
        let active_runtime_keys = HashSet::from([first.id.clone(), second.id.clone()]);

        let mut supervisor = TunnelSupervisor::new();
        supervisor.restore_active_frp_routes(
            &[first.clone(), second.clone()],
            &active_runtime_keys,
            &settings,
        );

        assert_eq!(supervisor.frp_routes.len(), 2);
        assert!(supervisor.frp_routes.contains_key(&first.id));
        assert!(supervisor.frp_routes.contains_key(&second.id));
        assert_eq!(supervisor.sessions.len(), 2);
        assert_eq!(
            supervisor
                .sessions
                .get(&second.id)
                .map(|session| session.public_url.as_str()),
            Some("https://lb.frp.example.com")
        );
    }

    #[test]
    fn syncing_one_workspace_does_not_change_another_workspace_pid() {
        let settings = AppSettings::default();
        let first = frp_profile("first", "first");
        let second = frp_profile("second", "second");
        let first_key = first.id.clone();
        let second_key = second.id.clone();
        let mut supervisor = TunnelSupervisor::new();
        supervisor
            .frp_routes
            .insert(first_key.clone(), FrpRoute { profile: first });
        supervisor
            .frp_routes
            .insert(second_key.clone(), FrpRoute { profile: second });
        supervisor.sync_frp_sessions_for_workspace(&settings, &second_key, Some(99));

        supervisor.sync_frp_sessions_for_workspace(&settings, &first_key, Some(42));

        assert_eq!(
            supervisor.sessions.get(&first_key).and_then(|s| s.pid),
            Some(42)
        );
        assert_eq!(
            supervisor.sessions.get(&second_key).and_then(|s| s.pid),
            Some(99)
        );
    }
}
