use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};

use crate::error::{AppError, AppResult};
use crate::platform::platform;
use crate::tunnel::cloudflare::stop_child;
use crate::tunnel::supervisor::log_dir_for_profile;
use crate::workspace::WorkspaceProfile;

use super::{
    build_frpc_toml_for_routes, frp_server_config, FrpServerConfig, VERSION as FRP_VERSION,
};

mod health;
mod install;
pub(crate) mod managed;
mod runtime;

#[cfg(test)]
use health::classify_public_mcp_body;
#[cfg(test)]
use health::successful_proxy_names;
pub(crate) use health::{
    frpc_log_name, frpc_reconnect_loop_detected, probe_local_mcp_ok, probe_public_mcp_endpoint,
    read_frpc_log_tail, PublicMcpProbe,
};
pub(crate) use install::{cached_frpc_path, download_frpc_to_cache};
pub use install::{ensure_frpc, resolve_frpc};
#[cfg(test)]
use managed::managed_frpc_pid_path;
pub(crate) use managed::{
    acquire_frpc_operation_lock, clear_managed_frpc_pid, managed_frpc_config_matches,
    managed_frpc_config_path, stop_recorded_frpc_instance,
};
#[cfg(test)]
use runtime::aggregate_uses_proxy;
pub use runtime::spawn_frpc;

const READY_TIMEOUT: Duration = Duration::from_secs(8);
const FRPC_RESTART_GRACE: Duration = Duration::from_millis(600);
const FRPC_OPERATION_LOCK_TIMEOUT: Duration = Duration::from_secs(15);
const FRPC_STALE_LOCK_AFTER: Duration = Duration::from_secs(30);

pub struct FrpcHandle {
    pub child: Child,
    pub pid: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::{
        aggregate_uses_proxy, classify_public_mcp_body, frpc_reconnect_loop_detected,
        managed_frpc_config_path, managed_frpc_pid_path, successful_proxy_names, PublicMcpProbe,
    };
    use crate::workspace::WorkspaceProfile;

    #[test]
    fn login_success_alone_is_not_a_ready_proxy() {
        let log = "login to server success, get run id [run-id]";
        assert!(successful_proxy_names(log).is_empty());
    }

    #[test]
    fn all_distinct_proxy_successes_are_counted() {
        let log = concat!(
            "[run-id] [first-mcp] start proxy success\n",
            "[run-id] [first-mcp] start proxy success\n",
            "[run-id] [second-mcp] proxy start success\n",
        );
        let names = successful_proxy_names(log);
        assert_eq!(names.len(), 2);
        assert!(names.contains("first-mcp"));
        assert!(names.contains("second-mcp"));
    }

    #[test]
    fn aggregate_proxy_is_enabled_when_any_route_requests_it() {
        let mut direct = WorkspaceProfile::new("C:/workspace/direct".into(), None);
        direct.tunnel.use_proxy = false;
        let mut proxied = WorkspaceProfile::new("C:/workspace/proxied".into(), None);
        proxied.tunnel.use_proxy = true;

        assert!(aggregate_uses_proxy(&[&direct, &proxied]));
        assert!(aggregate_uses_proxy(&[&proxied, &direct]));
        assert!(!aggregate_uses_proxy(&[&direct]));
    }

    #[test]
    fn managed_config_paths_are_isolated_by_workspace() {
        let first = managed_frpc_config_path("first-workspace").unwrap();
        let second = managed_frpc_config_path("second-workspace").unwrap();

        assert_ne!(first, second);
        assert!(first.ends_with("first-workspace/frpc.toml"));
        assert!(second.ends_with("second-workspace/frpc.toml"));
    }

    #[test]
    fn managed_pid_paths_are_isolated_and_sanitize_workspace_ids() {
        let path = managed_frpc_pid_path("../unsafe workspace").unwrap();
        let normalized = path.to_string_lossy().replace('\\', "/");

        assert!(normalized.ends_with("frpc/___unsafe_workspace/frpc.pid"));
        assert!(!normalized.contains("/../"));
    }

    #[test]
    fn reconnect_loop_is_detected_from_trailing_errors() {
        let log = "\
2026-08-02 15:38:30 [W] connect to server error: EOF\n\
2026-08-02 15:38:40 [I] try to connect to server...\n\
2026-08-02 15:38:50 [W] connect to server error: i/o deadline reached\n\
2026-08-02 15:39:10 [I] try to connect to server...\n\
2026-08-02 15:39:20 [W] connect to server error: i/o deadline reached\n\
2026-08-02 15:39:40 [I] try to connect to server...\n\
2026-08-02 15:39:50 [W] connect to server error: i/o deadline reached\n";
        assert!(frpc_reconnect_loop_detected(log));
    }

    #[test]
    fn successful_relogin_clears_reconnect_loop() {
        let log = "\
2026-08-02 15:38:50 [W] connect to server error: i/o deadline reached\n\
2026-08-02 15:39:10 [W] connect to server error: i/o deadline reached\n\
2026-08-02 15:39:20 [I] login to server success, get run id [abc]\n\
2026-08-02 15:39:21 [I] [ws-demo-mcp] start proxy success\n";
        assert!(!frpc_reconnect_loop_detected(log));
    }

    #[test]
    fn frp_not_found_html_is_classified_as_not_routed() {
        let body = r#"<!DOCTYPE html><title>Not Found</title>
<p>The page you requested was not found.</p>
<p>The server is powered by <a href="https://github.com/fatedier/frp">frp</a>.</p>"#;
        assert_eq!(
            classify_public_mcp_body(404, body),
            PublicMcpProbe::FrpNotRouted
        );
        assert_eq!(
            classify_public_mcp_body(
                200,
                r#"{"name":"coding-tools-mcp","protocolVersion":"2025-06-18"}"#
            ),
            PublicMcpProbe::Healthy
        );
    }
}
