mod agent_context;
mod app_info;
mod computer;
mod frp_profiles;
mod global_gateway;
mod history;
mod lan_peers;
mod logs;
mod planning;
pub(crate) mod runtime;
mod runtime_settings;
mod secrets;
mod software;
mod tool_audit;
pub(crate) mod ui_memory;
mod upstream_mcp;
mod usage;
pub(crate) mod window_chrome;
mod workspace;

pub use agent_context::{scan_agent_context, scan_global_agent_context};
pub use app_info::{check_app_update, open_url};
pub use computer::{
    approve_computer_screen_request, decide_computer_sensitive_action, enable_computer_access,
    get_computer_screen_status, list_computer_available_apps, pause_computer_access,
    request_computer_accessibility_permission, request_computer_screen_permission,
    resume_computer_access, revoke_computer_screen_access,
};
pub use frp_profiles::{
    delete_frp_profile, get_last_workspace_id, get_proxy, list_frp_profiles, save_frp_profile,
    set_last_workspace, set_proxy,
};
pub use global_gateway::{
    check_global_gateway_health, get_gemini_local_bridge_status, get_global_gateway_config,
    get_global_gateway_status, prepare_gemini_local_bridge, restart_global_cloudflare_tunnel,
    set_global_gateway_config,
};
pub use history::list_history_sessions;
pub use lan_peers::{
    cancel_lan_pairing_pin, create_lan_pairing_pin, get_lan_peers_overview, pair_lan_peer,
    probe_lan_peer_address, remove_lan_paired_peer, rename_lan_paired_peer,
    save_lan_federation_settings, scan_lan_peers, set_lan_paired_peer_enabled,
};
pub use logs::read_workspace_logs;
pub use planning::{
    accept_goal_review, accept_plan_review, delete_plan, get_planning_state, reject_goal_review,
    reject_plan_review, reset_planning_state, set_planning_mode,
};
pub use runtime::{
    get_global_mcp_overview, get_runtime_status, restore_runtime_state, start_runtime, stop_runtime,
};
pub use runtime_settings::{get_global_runtime_settings, set_global_runtime_settings};
pub use secrets::{get_shared_secret, regenerate_shared_secret, set_shared_secrets};
pub use software::{
    get_download_config, install_software, list_software, set_download_config, uninstall_software,
};
pub use tool_audit::{
    clear_tool_audit_records, list_tool_audit_records, set_tool_audit_retention_days,
};
pub use ui_memory::{get_webview_memory_sample, recreate_ui_webview};
pub use upstream_mcp::{
    delete_upstream_mcp_server, get_upstream_mcp_overview, import_upstream_mcp_candidates,
    list_upstream_mcp_takeover_backups, parse_upstream_mcp_json, preview_upstream_mcp_takeover,
    restore_upstream_mcp_takeover_backup, save_upstream_mcp_server,
    scan_local_upstream_mcp_clients, set_upstream_mcp_global_settings,
    set_upstream_mcp_server_enabled, set_upstream_mcp_server_mode, sync_all_upstream_mcp_servers,
    test_and_sync_upstream_mcp_server, update_upstream_mcp_disabled_tools,
};
pub use usage::get_service_usage_stats;
pub use window_chrome::{enter_compact_mode, hide_to_tray, quit_app};
pub use workspace::{
    create_session_review_url, create_workspace, create_workspace_review_url, delete_workspace,
    delete_workspace_review, detect_installed_ides, get_workspace_activity_metrics,
    get_workspace_git_summary, issue_workspace_review_url, list_workspace_reviews, list_workspaces,
    open_workspace_directory, open_workspace_in_ide, update_workspace,
};
