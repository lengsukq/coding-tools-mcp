mod agent_context;
mod app_info;
mod frp_profiles;
mod global_gateway;
mod health;
mod history;
mod logs;
mod planning;
pub(crate) mod runtime;
mod runtime_settings;
mod secrets;
mod software;
pub(crate) mod ui_memory;
mod usage;
pub(crate) mod window_chrome;
mod workspace;

pub use agent_context::{scan_agent_context, scan_global_agent_context};
pub use app_info::{check_app_update, open_url};
pub use frp_profiles::{
    delete_frp_profile, get_app_settings, get_last_workspace_id, get_proxy, list_frp_profiles,
    save_frp_profile, set_last_workspace, set_proxy,
};
pub use global_gateway::{
    check_global_gateway_health, get_global_gateway_config, get_global_gateway_status,
    set_global_gateway_config, start_global_gateway, stop_global_gateway,
};
pub use health::run_global_health_checks;
pub use history::list_history_sessions;
pub use logs::read_workspace_logs;
pub use planning::{
    accept_goal_review, accept_plan_review, delete_plan, get_planning_state, reject_goal_review,
    reject_plan_review, reset_planning_state, set_planning_mode,
};
pub use runtime::{
    get_global_mcp_overview, get_runtime_status, restart_runtime, restore_runtime_state,
    start_runtime, stop_runtime,
};
pub use runtime_settings::{get_global_runtime_settings, set_global_runtime_settings};
pub use secrets::{
    get_shared_secret, regenerate_shared_secret, set_shared_secret, set_shared_secrets,
};
pub use software::{
    get_download_config, install_software, list_software, set_download_config, uninstall_software,
};
pub use ui_memory::{get_webview_memory_sample, recreate_ui_webview};
pub use usage::get_service_usage_stats;
pub use window_chrome::{hide_to_tray, quit_app, show_main_window};
pub use workspace::{
    create_session_review_url, create_workspace, create_workspace_review_url, delete_workspace, delete_workspace_review, detect_installed_ides, get_workspace_git_summary, issue_workspace_review_url, list_workspace_reviews,
    get_workspace_activity_metrics, list_workspaces, open_workspace_directory, open_workspace_in_ide, update_workspace,
};
