//! Headless MCP stdio-to-HTTP bridge for desktop AI clients.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use super::gateway::MCP_SESSION_HEADER;

mod auth;
mod compact;
mod transport;
#[cfg(test)]
mod tests;

pub use auth::{
    format_mcp_stdio_command, gemini_stdio_auth_file_path, gemini_stdio_bin_path, read_auth_file,
    remove_auth_file_if_exists, validate_loopback_mcp_endpoint, write_private_auth_file,
    GeminiStdioAuthFile, GEMINI_STDIO_CLIENT_ID,
};

use auth::load_bearer_token;
use transport::{forward_process_stdio, BridgeState};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Run the headless `mcp-stdio` subcommand from `main`/`lib::run` before Tauri GUI initialization.
pub fn run_cli(args: &[String]) -> i32 {
    if args.len() != 1 || matches!(args[0].as_str(), "-h" | "--help") {
        eprintln!("Usage: coding-tools-mcp mcp-stdio <http://127.0.0.1:<port>/mcp>");
        return 2;
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("mcp-stdio failed to start async runtime: {err}");
            return 1;
        }
    };
    let auth_path = gemini_stdio_auth_file_path().ok();
    match runtime.block_on(run_bridge(&args[0], auth_path.as_deref())) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("mcp-stdio error: {err}");
            1
        }
    }
}

pub async fn run_bridge(endpoint: &str, auth_file: Option<&Path>) -> Result<(), String> {
    let endpoint = validate_loopback_mcp_endpoint(endpoint)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|err| format!("Unable to initialize HTTP client: {err}"))?;

    let initial_token = load_bearer_token(auth_file);
    let state = Arc::new(BridgeState {
        client,
        endpoint,
        auth_file: auth_file.map(Path::to_path_buf),
        bearer_token: RwLock::new(initial_token),
        session_id: RwLock::new(None),
    });

    forward_process_stdio(state).await
}
