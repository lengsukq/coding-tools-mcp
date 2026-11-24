use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};

use crate::error::{AppError, AppResult};
use crate::platform::platform;
use crate::settings::{AppSettings, GlobalGatewayConfig};
use crate::tunnel::cloudflare::stop_child;
use crate::tunnel::log_dir_for_profile;

use super::{build_frpc_toml, global_frp_server_config, FrpServerConfig, VERSION as FRP_VERSION};

mod health;
mod install;
pub(crate) mod managed;
mod runtime;

#[cfg(test)]
use health::successful_proxy_names;
pub(crate) use install::{cached_frpc_path, download_frpc_to_cache};
pub use install::{ensure_frpc, resolve_frpc};
#[cfg(test)]
use managed::managed_frpc_pid_path;
pub(crate) use managed::{
    acquire_frpc_operation_lock, clear_managed_frpc_pid, managed_frpc_config_path,
    managed_frpc_dir, stop_recorded_frpc_instance,
};
pub use runtime::spawn_global_frpc;

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
    use super::{managed_frpc_config_path, managed_frpc_pid_path, successful_proxy_names};

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
}
