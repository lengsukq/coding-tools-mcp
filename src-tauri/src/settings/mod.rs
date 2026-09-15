mod model;

pub use model::{AppSettings, DownloadConfig, FrpProfile, GlobalGatewayConfig, ProxyConfig};
pub(crate) use model::{
    default_global_allowed_commands, default_global_executable_paths,
    default_global_permission_mode, GLOBAL_RUNTIME_DEFAULTS_VERSION,
};
