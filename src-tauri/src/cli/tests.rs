use std::path::Path;
use std::sync::Arc;

use tokio::sync::Notify;

use super::is_cli_subcommand;
use super::systemd_cmd::render_systemd_user_unit;

#[test]
fn classifies_cli_subcommands_without_intercepting_desktop_autostart_flags() {
    assert!(is_cli_subcommand("serve"));
    assert!(is_cli_subcommand("workspace"));
    assert!(is_cli_subcommand("lan"));
    assert!(is_cli_subcommand("mcp-stdio"));
    assert!(is_cli_subcommand("--version"));
    assert!(!is_cli_subcommand("--autostart"));
    assert!(!is_cli_subcommand("--lightweight"));
    assert!(!is_cli_subcommand("-psn_0_12345"));
}

#[test]
fn systemd_unit_quotes_paths_with_spaces_and_enables_lan_serve() {
    let unit = render_systemd_user_unit(Path::new("/opt/Coding Tools/coding-tools-mcp"));
    assert!(unit.contains("ExecStart=\"/opt/Coding Tools/coding-tools-mcp\" serve --lan"));
    assert!(unit.contains("Restart=on-failure"));
    assert!(unit.contains("WantedBy=default.target"));
}

#[cfg(unix)]
#[tokio::test]
async fn daemon_ipc_roundtrips_ping_and_graceful_stop() {
    use crate::app_state::AppState;
    use crate::platform::local_ipc::{
        send_daemon_request_at, start_daemon_ipc_server, LocalDaemonRequest, LocalDaemonResponse,
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let sock = temp.path().join("daemon.sock");
    let state = Arc::new(AppState::new().expect("app state"));
    let notify = Arc::new(Notify::new());
    let _guard = start_daemon_ipc_server(sock.clone(), Arc::clone(&state), Arc::clone(&notify))
        .await
        .expect("start ipc");

    let pong = send_daemon_request_at(&sock, &LocalDaemonRequest::Ping)
        .await
        .expect("ping");
    assert!(matches!(pong, LocalDaemonResponse::Pong { .. }));

    let dup = start_daemon_ipc_server(sock.clone(), Arc::clone(&state), Arc::clone(&notify)).await;
    assert!(dup.is_err(), "second daemon must detect running primary");

    let waiter = notify.notified();
    let ack = send_daemon_request_at(&sock, &LocalDaemonRequest::Stop)
        .await
        .expect("stop");
    assert!(matches!(ack, LocalDaemonResponse::Ack { .. }));
    tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
        .await
        .expect("stop notified");
}
