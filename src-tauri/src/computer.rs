//! Explicitly approved, transport-session-scoped macOS screen observation.
//!
//! This module is deliberately host-scoped, not Workspace-scoped. In particular,
//! neither the Workspace "trusted" setting nor command execution can grant it.
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const REQUEST_TTL: Duration = Duration::from_secs(120);
const GRANT_TTL: Duration = Duration::from_secs(300);

#[derive(Debug)]
struct Pending {
    id: String,
    session: String,
    at: Instant,
}

#[derive(Debug)]
struct Grant {
    session: String,
    until: Instant,
}

#[derive(Debug, Default)]
struct Controller {
    pending: Option<Pending>,
    grant: Option<Grant>,
}

impl Controller {
    fn expire(&mut self) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.at.elapsed() >= REQUEST_TTL)
        {
            self.pending = None;
        }
        if self
            .grant
            .as_ref()
            .is_some_and(|g| Instant::now() >= g.until)
        {
            self.grant = None;
        }
    }

    fn request(&mut self, session: &str) -> Result<Value, String> {
        self.expire();
        if let Some(grant) = &self.grant {
            if grant.session == session {
                return Ok(
                    json!({ "status": "approved", "expires_in_seconds": grant.until.saturating_duration_since(Instant::now()).as_secs() }),
                );
            }
            return Err("另一 MCP 会话正在控制屏幕，请先在本机终止该会话".into());
        }
        if let Some(pending) = &self.pending {
            if pending.session != session {
                return Err("已有待审批的电脑观察请求".into());
            }
            return Ok(json!({ "status": "pending", "request_id": pending.id }));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.pending = Some(Pending {
            id: id.clone(),
            session: session.to_owned(),
            at: Instant::now(),
        });
        Ok(
            json!({ "status": "pending", "request_id": id, "message": "请用户在 Coding Tools MCP 桌面设置中批准此请求；审批前不能获取截图" }),
        )
    }

    fn approve(&mut self, id: &str) -> Result<(), String> {
        self.expire();
        let pending = self.pending.as_ref().ok_or("请求不存在或已过期")?;
        if pending.id != id {
            return Err("请求 ID 不匹配".into());
        }
        let session = pending.session.clone();
        self.pending = None;
        self.grant = Some(Grant {
            session,
            until: Instant::now() + GRANT_TTL,
        });
        Ok(())
    }

    fn allowed(&mut self, session: &str) -> bool {
        self.expire();
        self.grant
            .as_ref()
            .is_some_and(|grant| grant.session == session)
    }
}

fn control() -> &'static Mutex<Controller> {
    static CONTROL: OnceLock<Mutex<Controller>> = OnceLock::new();
    CONTROL.get_or_init(|| Mutex::new(Controller::default()))
}

fn locked() -> std::sync::MutexGuard<'static, Controller> {
    control()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Local Tauri UI only; never expose local_approve/local_revoke as MCP tools.
pub fn local_status() -> Value {
    let mut state = locked();
    state.expire();
    json!({
        "pendingRequestId": state.pending.as_ref().map(|p| p.id.as_str()),
        "pendingExpiresInSeconds": state.pending.as_ref().map(|p| REQUEST_TTL.saturating_sub(p.at.elapsed()).as_secs()),
        "active": state.grant.is_some(),
        "expiresInSeconds": state.grant.as_ref().map(|g| g.until.saturating_duration_since(Instant::now()).as_secs()),
        "supported": cfg!(target_os = "macos"),
        "screenRecordingGranted": screen_recording_permission()
    })
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[cfg(target_os = "macos")]
fn screen_recording_permission() -> Option<bool> {
    Some(unsafe { CGPreflightScreenCaptureAccess() })
}

#[cfg(not(target_os = "macos"))]
fn screen_recording_permission() -> Option<bool> {
    None
}

/// Only invoked from a local user gesture in Tauri, never from an MCP tool.
pub fn local_request_screen_permission() -> Value {
    #[cfg(target_os = "macos")]
    unsafe {
        let _ = CGRequestScreenCaptureAccess();
    }
    local_status()
}

pub fn local_approve(request_id: &str) -> Result<Value, String> {
    locked().approve(request_id)?;
    Ok(local_status())
}

pub fn local_revoke() -> Value {
    *locked() = Controller::default();
    local_status()
}

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "computer_session",
            "title": "Computer session",
            "description": "Request a temporary screen observation grant. Requires explicit approval in the local desktop app; no Workspace permission can authorize it.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["request", "status", "stop"] }
            }, "required": ["action"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false }
        }),
        json!({
            "name": "computer_observe",
            "title": "Observe computer screen",
            "description": "Return a fresh screenshot as an MCP image after local approval of this transport session. macOS only; screenshots can contain private content.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["screenshot"] },
                "display_index": { "type": "integer", "minimum": 1, "maximum": 16, "default": 1 }
            }, "required": ["action"], "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "openWorldHint": true }
        }),
    ]
}

fn tool_error(message: &str) -> Value {
    json!({ "isError": true, "content": [{ "type": "text", "text": message }], "structuredContent": { "ok": false, "error": message } })
}

fn tool_ok(value: Value) -> Value {
    json!({ "isError": false, "content": [{ "type": "text", "text": value.to_string() }], "structuredContent": value })
}

/// The caller identity is the *server-generated transport session*, never the
/// caller-supplied `_meta` field or an arbitrary workspace_id.
pub fn call_tool(name: &str, session_id: Option<&str>, args: &Value) -> Value {
    let Some(session) = session_id.filter(|s| !s.is_empty()) else {
        return tool_error(
            "COMPUTER_SESSION_REQUIRED: 必须建立持久 MCP transport session 才能申请截图授权",
        );
    };
    let action = args.get("action").and_then(Value::as_str).unwrap_or("");
    match (name, action) {
        ("computer_session", "request") => match locked().request(session) {
            Ok(result) => tool_ok(result),
            Err(error) => tool_error(&error),
        },
        ("computer_session", "status") => {
            let mut state = locked();
            let approved = state.allowed(session);
            let pending = state.pending.as_ref().is_some_and(|p| p.session == session);
            tool_ok(
                json!({ "status": if approved { "approved" } else if pending { "pending" } else { "disabled" } }),
            )
        }
        ("computer_session", "stop") => {
            let mut state = locked();
            state.expire();
            if state.grant.as_ref().is_some_and(|p| p.session == session)
                || state.pending.as_ref().is_some_and(|p| p.session == session)
            {
                *state = Controller::default();
                tool_ok(json!({ "status": "stopped" }))
            } else {
                tool_error("会话无权终止其他客户端的 Computer Use 授权")
            }
        }
        ("computer_observe", "screenshot") => {
            if !locked().allowed(session) {
                return tool_error("COMPUTER_NOT_APPROVED: 请先通过 computer_session(request) 申请，并在本机桌面应用中批准");
            }
            let Some(index) = args
                .get("display_index")
                .map(Value::as_u64)
                .unwrap_or(Some(1))
                .filter(|n| (1..=16).contains(n))
            else {
                return tool_error("display_index 必须为 1 到 16");
            };
            match capture(index) {
                Ok((data, width, height)) => {
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                    let metadata = json!({ "ok": true, "display_index": index, "width": width, "height": height, "mime_type": "image/jpeg", "snapshot_id": uuid::Uuid::new_v4().to_string() });
                    json!({
                        "isError": false,
                        "content": [
                            { "type": "text", "text": metadata.to_string() },
                            { "type": "image", "data": encoded, "mimeType": "image/jpeg" }
                        ],
                        "structuredContent": metadata
                    })
                }
                Err(error) => tool_error(&error),
            }
        }
        _ => tool_error("不支持的 computer action"),
    }
}

#[cfg(target_os = "macos")]
fn capture(index: u64) -> Result<(Vec<u8>, u32, u32), String> {
    use image::codecs::jpeg::JpegEncoder;
    use image::ImageEncoder;
    // P1 bootstrap backend. Switch to ScreenCaptureKit native capture in the
    // next step; this isolated OS command never goes through exec_command.
    let file = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()
        .map_err(|e| format!("无法创建临时截图: {e}"))?;
    let mut child = std::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", "-t", "png", "-D", &index.to_string()])
        .arg(file.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动 macOS 截图失败: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return Err(
                "SCREEN_CAPTURE_FAILED: 截图失败。检查屏幕录制权限、显示器序号以及 Mac 是否解锁"
                    .into(),
            ),
            Ok(None) if started.elapsed() >= Duration::from_secs(12) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "SCREEN_CAPTURE_TIMEOUT: 12 秒内未完成截图，请确认屏幕授权弹窗和桌面解锁状态"
                        .into(),
                );
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("截图子进程状态检查失败: {error}"));
            }
        }
    }
    let pixels = std::fs::read(file.path()).map_err(|e| e.to_string())?;
    if pixels.len() > 40 * 1024 * 1024 {
        return Err("SCREEN_CAPTURE_TOO_LARGE: 图片超出 40MiB 限制".into());
    }
    let frame = image::load_from_memory(&pixels).map_err(|e| format!("截图图片格式不可用: {e}"))?;
    let scaled = frame.thumbnail(1920, 1200).to_rgb8();
    let (width, height) = scaled.dimensions();
    let mut data = Vec::new();
    JpegEncoder::new_with_quality(&mut data, 75)
        .write_image(
            scaled.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())?;
    Ok((data, width, height))
}

#[cfg(not(target_os = "macos"))]
fn capture(_: u64) -> Result<(Vec<u8>, u32, u32), String> {
    Err("UNSUPPORTED_PLATFORM: 首版 Computer Use 截图仅支持 macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_is_opt_in_and_session_scoped() {
        let mut state = Controller::default();
        assert!(!state.allowed("a"));
        let result = state.request("a").unwrap();
        assert!(state.request("b").is_err());
        assert!(state.approve("wrong").is_err());
        state
            .approve(result["request_id"].as_str().unwrap())
            .unwrap();
        assert!(state.allowed("a"));
        assert!(!state.allowed("b"));
    }

    #[test]
    fn stale_grants_and_requests_are_revoked() {
        let mut state = Controller {
            pending: Some(Pending {
                id: "p".into(),
                session: "a".into(),
                at: Instant::now() - REQUEST_TTL,
            }),
            grant: Some(Grant {
                session: "b".into(),
                until: Instant::now() - Duration::from_secs(1),
            }),
        };
        state.expire();
        assert!(state.pending.is_none());
        assert!(state.grant.is_none());
    }
}
