use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const GEMINI_STDIO_CLIENT_ID: &str = "gemini-local-stdio";
const AUTH_FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiStdioAuthFile {
    pub version: u32,
    pub auth_type: String,
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl GeminiStdioAuthFile {
    pub fn new_oauth(
        endpoint: String,
        server_url: String,
        access_token: String,
        expires_at: i64,
    ) -> Self {
        Self {
            version: AUTH_FILE_VERSION,
            auth_type: "oauth".into(),
            endpoint,
            access_token: Some(access_token),
            server_url: Some(server_url),
            client_id: Some(GEMINI_STDIO_CLIENT_ID.into()),
            expires_at: Some(expires_at),
        }
    }
}

/// Validate that the stdio bridge only talks to a loopback HTTP `/mcp` endpoint with an explicit port.
pub fn validate_loopback_mcp_endpoint(raw: &str) -> Result<String, String> {
    let parsed =
        reqwest::Url::parse(raw.trim()).map_err(|err| format!("Invalid MCP URL '{raw}': {err}"))?;
    if parsed.scheme() != "http" {
        return Err("Local MCP stdio bridge only accepts http:// loopback endpoints".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Local MCP endpoint must not include credentials".into());
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("Local MCP endpoint must not include query parameters or fragments".into());
    }
    if parsed.path() != "/mcp" {
        return Err("Local MCP endpoint path must be /mcp".into());
    }
    let port = parsed
        .port()
        .ok_or_else(|| "Local MCP endpoint must specify an explicit port".to_string())?;
    let host = parsed.host_str().unwrap_or("");
    let normalized_host = match host {
        "127.0.0.1" | "localhost" => "127.0.0.1",
        "::1" | "[::1]" => "[::1]",
        _ => return Err("Local MCP endpoint host must be 127.0.0.1, localhost, or [::1]".into()),
    };
    Ok(format!("http://{normalized_host}:{port}/mcp"))
}

pub fn gemini_stdio_bin_path() -> Result<PathBuf, String> {
    #[cfg(unix)]
    {
        let home = dirs::home_dir()
            .ok_or_else(|| "Unable to locate home directory for local MCP binary".to_string())?;
        Ok(home.join(".local").join("bin").join("coding-tools-mcp"))
    }
    #[cfg(not(unix))]
    {
        std::env::current_exe()
            .map_err(|err| format!("Unable to resolve current executable path: {err}"))
    }
}

pub fn gemini_stdio_auth_file_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir()
        .ok_or_else(|| "Unable to locate home directory for local MCP credentials".to_string())?;
    Ok(home
        .join(".local")
        .join("share")
        .join("coding-tools-mcp")
        .join("gemini-stdio-auth.json"))
}

pub fn format_mcp_stdio_command(binary_path: &Path, endpoint: &str) -> String {
    let bin = binary_path.to_string_lossy();
    let quoted_bin = if bin.chars().any(char::is_whitespace) {
        format!("\"{bin}\"")
    } else {
        bin.into_owned()
    };
    format!("{quoted_bin} mcp-stdio {endpoint}")
}

pub fn read_auth_file(path: &Path) -> Result<Option<GeminiStdioAuthFile>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("Unable to read local MCP credentials file: {err}"))?;
    let parsed = serde_json::from_str::<GeminiStdioAuthFile>(&raw)
        .map_err(|err| format!("Invalid local MCP credentials file: {err}"))?;
    Ok(Some(parsed))
}

pub fn write_private_auth_file(path: &Path, payload: &GeminiStdioAuthFile) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Invalid local MCP credentials file path".to_string())?;
    ensure_private_dir(parent)?;
    let encoded = serde_json::to_string_pretty(payload)
        .map_err(|err| format!("Unable to serialize local MCP credentials: {err}"))?;

    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|err| format!("Unable to write local MCP credentials file: {err}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|err| format!("Unable to lock local MCP credentials permissions: {err}"))?;
    }
    file.write_all(encoded.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|err| format!("Unable to flush local MCP credentials file: {err}"))
}

pub fn remove_auth_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "Unable to remove stale MCP credentials file: {err}"
        )),
    }
}

pub(super) fn load_bearer_token(auth_file: Option<&Path>) -> Option<String> {
    let path = auth_file?;
    match read_auth_file(path) {
        Ok(Some(file)) if file.auth_type == "oauth" => file
            .access_token
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty()),
        Ok(_) => None,
        Err(err) => {
            eprintln!("mcp-stdio warning: {err}");
            None
        }
    }
}

fn ensure_private_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|err| format!("Unable to create local MCP credentials directory: {err}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|err| format!("Unable to protect local MCP credentials directory: {err}"))?;
    }
    Ok(())
}
