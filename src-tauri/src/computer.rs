//! Explicitly approved, transport-session-scoped macOS screen observation.
//!
//! This module is deliberately host-scoped, not Workspace-scoped. In particular,
//! neither the Workspace "trusted" setting nor command execution can grant it.
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::computer_geometry::CaptureFrame;
use serde_json::{json, Value};

const REQUEST_TTL: Duration = Duration::from_secs(120);
const GRANT_TTL: Duration = Duration::from_secs(300);
const SNAPSHOT_TTL: Duration = Duration::from_secs(45);

#[derive(Debug)]
struct Pending {
    id: String,
    session: String,
    scope: String,
    at: Instant,
}

#[derive(Debug)]
struct Snapshot {
    id: String,
    lease_id: String,
    session: String,
    created_at: Instant,
    frame: CaptureFrame,
}

fn action_lock() -> std::sync::MutexGuard<'static, ()> {
    static ACTIONS: OnceLock<Mutex<()>> = OnceLock::new();
    ACTIONS
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn local_request_accessibility_permission() -> Value {
    #[cfg(target_os = "macos")]
    native::request_accessibility();
    local_status()
}

#[cfg(target_os = "macos")]
fn accessibility_permission() -> Option<bool> {
    Some(native::accessibility_granted())
}
#[cfg(not(target_os = "macos"))]
fn accessibility_permission() -> Option<bool> {
    None
}

#[derive(Debug)]
struct Grant {
    id: String,
    session: String,
    scope: String,
    app_bundle: Option<String>,
    until: Instant,
    actions_used: u16,
}

#[derive(Debug, Default)]
struct Controller {
    pending: Option<Pending>,
    grant: Option<Grant>,
    snapshot: Option<Snapshot>,
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
            self.snapshot = None;
        }
        if self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.created_at.elapsed() >= SNAPSHOT_TTL)
        {
            self.snapshot = None;
        }
    }

    #[cfg(test)]
    fn revoked_or_replaced_grant_never_authorizes_in_flight_observation() {
        let mut state = Controller::default();
        let a = state.request("same-client", "observe").unwrap();
        state
            .approve(a["request_id"].as_str().unwrap(), None)
            .unwrap();
        let before = state.lease_token("same-client").unwrap();
        assert!(state.lease_matches("same-client", &before));
        state.grant = None;
        assert!(!state.lease_matches("same-client", &before));
        let b = state.request("same-client", "observe").unwrap();
        state
            .approve(b["request_id"].as_str().unwrap(), None)
            .unwrap();
        assert!(!state.lease_matches("same-client", &before));
        assert!(state.lease_token("some-other-client").is_none());
    }

    #[cfg(test)]
    fn coordinate_actions_require_recent_matching_snapshot() {
        let mut state = Controller::default();
        let req = state.request("s", "control").unwrap();
        state
            .approve(
                req["request_id"].as_str().unwrap(),
                Some("com.apple.Safari".into()),
            )
            .unwrap();
        let lease = state.lease_token("s").unwrap();
        let geometry = json!({
            "origin_x": -1440.0, "origin_y": 0.0, "point_width": 1440.0,
            "point_height": 900.0, "pixel_width": 1920, "pixel_height": 1200
        });
        let snap = state.record_snapshot("s", &lease, &geometry).unwrap();
        assert!(state
            .prepare_action("s", &json!({"action":"click","x":20,"y":30}))
            .is_err());
        assert!(state
            .prepare_action(
                "s",
                &json!({"action":"click","x":20,"y":30,"snapshot_id":"forged"})
            )
            .is_err());
        let (_, converted) = state.prepare_action("s", &json!({
            "action":"click","x":960,"y":600,"coordinate_space":"image_pixels","snapshot_id":snap
        })).unwrap();
        assert!(converted["x"].as_f64().unwrap() < 0.0);
        assert_eq!(state.grant.as_ref().unwrap().actions_used, 1);
        state.snapshot.as_mut().unwrap().created_at = Instant::now() - SNAPSHOT_TTL;
        assert!(state
            .prepare_action(
                "s",
                &json!({"action":"click","x":1,"y":2,"snapshot_id":snap})
            )
            .is_err());
    }

    #[cfg(test)]
    fn action_target(&mut self, session: &str) -> Option<String> {
        self.expire();
        let grant = self.grant.as_mut().filter(|grant| {
            grant.session == session && grant.scope == "control" && grant.actions_used < 100
        })?;
        let target = grant.app_bundle.clone()?;
        grant.actions_used += 1;
        Some(target)
    }

    fn request(&mut self, session: &str, scope: &str) -> Result<Value, String> {
        self.expire();
        if let Some(grant) = &self.grant {
            if grant.session == session {
                if grant.scope != scope {
                    return Err("切换读取/控制权限时请先撤销现有授权，再重新申请".into());
                }
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
            if pending.scope != scope {
                return Err("请先取消上一权限申请，再切换授权模式".into());
            }
            return Ok(json!({ "status": "pending", "request_id": pending.id }));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.pending = Some(Pending {
            id: id.clone(),
            session: session.to_owned(),
            scope: scope.to_owned(),
            at: Instant::now(),
        });
        Ok(
            json!({ "status": "pending", "scope": scope, "request_id": id, "message": "请用户在 Coding Tools MCP 本机设置中审批 Computer Use；未经批准不能查看或控制屏幕" }),
        )
    }

    fn approve(&mut self, id: &str, app_bundle: Option<String>) -> Result<(), String> {
        self.expire();
        let pending = self.pending.as_ref().ok_or("请求不存在或已过期")?;
        if pending.id != id {
            return Err("请求 ID 不匹配".into());
        }
        let session = pending.session.clone();
        let scope = pending.scope.clone();
        self.pending = None;
        self.snapshot = None;
        self.grant = Some(Grant {
            id: uuid::Uuid::new_v4().to_string(),
            session,
            scope,
            app_bundle,
            until: Instant::now() + GRANT_TTL,
            actions_used: 0,
        });
        Ok(())
    }

    fn allowed(&mut self, session: &str) -> bool {
        self.expire();
        self.grant
            .as_ref()
            .is_some_and(|grant| grant.session == session)
    }

    /// Snapshot the exact lease, not just the session identifier: a revoked
    /// lease must not release a screenshot after another grant is approved.
    fn lease_token(&mut self, session: &str) -> Option<String> {
        self.expire();
        self.grant
            .as_ref()
            .filter(|grant| grant.session == session)
            .map(|grant| grant.id.clone())
    }

    fn lease_matches(&mut self, session: &str, token: &str) -> bool {
        self.lease_token(session).as_deref() == Some(token)
    }

    fn record_snapshot(
        &mut self,
        session: &str,
        lease: &str,
        metadata: &Value,
    ) -> Result<String, String> {
        if !self.lease_matches(session, lease) {
            return Err("COMPUTER_REVOKED: 授权在截图期间失效".into());
        }
        let frame = CaptureFrame::from_metadata(metadata)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.snapshot = Some(Snapshot {
            id: id.clone(),
            lease_id: lease.to_owned(),
            session: session.to_owned(),
            created_at: Instant::now(),
            frame,
        });
        Ok(id)
    }

    fn prepare_action(&mut self, session: &str, args: &Value) -> Result<(String, Value), String> {
        self.expire();
        validate_action(args)?;
        let grant = self
            .grant
            .as_ref()
            .filter(|g| g.session == session && g.scope == "control" && g.actions_used < 100)
            .ok_or(
                "CONTROL_NOT_APPROVED: 请用户在本机以 control 模式授权此会话，或重新批准操作额度",
            )?;
        let target = grant.app_bundle.clone().ok_or("CONTROL_TARGET_REQUIRED")?;
        let action = args["action"].as_str().unwrap_or("");
        let prepared = if matches!(
            action,
            "click" | "double_click" | "right_click" | "move" | "drag"
        ) {
            let frame = self
                .snapshot
                .as_ref()
                .filter(|s| {
                    s.session == session
                        && s.lease_id == grant.id
                        && s.created_at.elapsed() < SNAPSHOT_TTL
                        && args["snapshot_id"].as_str() == Some(s.id.as_str())
                })
                .map(|s| &s.frame)
                .ok_or(
                    "STALE_SNAPSHOT: 坐标操作须引用当前控制会话 45 秒内的 screenshot snapshot_id",
                )?;
            frame.map_action(args)?
        } else {
            args.clone()
        };
        self.grant.as_mut().expect("checked grant").actions_used += 1;
        Ok((target, prepared))
    }
}

fn validate_action(args: &Value) -> Result<(), String> {
    let action = args.get("action").and_then(Value::as_str).unwrap_or("");
    let required = match action {
        "click" | "double_click" | "right_click" | "move" => &["x", "y"][..],
        "drag" => &["x", "y", "end_x", "end_y"][..],
        "scroll" => &["amount"][..],
        "type" => &["text"][..],
        "key" | "focus_app" => {
            if action == "key" {
                &["key"][..]
            } else {
                &[][..]
            }
        }
        "press_element" => &["label"][..],
        _ => return Err("INVALID_ACTION: unsupported action".into()),
    };
    for field in required {
        let value = args
            .get(*field)
            .ok_or_else(|| format!("INVALID_ARGUMENT: missing {field}"))?;
        if matches!(*field, "x" | "y" | "end_x" | "end_y")
            && !value.as_f64().is_some_and(f64::is_finite)
        {
            return Err(format!("INVALID_COORDINATE: invalid {field}"));
        }
        if *field == "amount" && !value.as_i64().is_some_and(|n| (-15..=15).contains(&n)) {
            return Err("INVALID_SCROLL: amount out of range".into());
        }
        if matches!(*field, "text" | "key" | "label") && !value.is_string() {
            return Err(format!("INVALID_ARGUMENT: {field} must be string"));
        }
    }
    if action == "type" && args["text"].as_str().unwrap_or("").encode_utf16().count() > 2000 {
        return Err("INVALID_TEXT: too long".into());
    }
    if action == "press_element"
        && !args["label"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.chars().count() <= 160)
    {
        return Err("INVALID_ELEMENT_LABEL".into());
    }
    if action == "key" {
        if let Some(modifiers) = args.get("modifiers") {
            let Some(all) = modifiers.as_array() else {
                return Err("INVALID_MODIFIERS".into());
            };
            if all.len() > 4
                || all.iter().any(|m| {
                    !matches!(m.as_str(), Some("command" | "shift" | "control" | "option"))
                })
            {
                return Err("INVALID_MODIFIERS".into());
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn native_frontmost() -> Result<Value, String> {
    native::frontmost()
}
#[cfg(target_os = "macos")]
fn native_apps() -> Result<Value, String> {
    native::apps()
}
#[cfg(target_os = "macos")]
fn native_ui_tree() -> Result<Value, String> {
    native::ui_tree()
}
#[cfg(target_os = "macos")]
fn native_action(target: &str, args: &Value) -> Result<Value, String> {
    if args["action"] != "focus_app" && native_frontmost()?["bundle_id"].as_str() != Some(target) {
        return Err("FOREGROUND_APP_CHANGED: 请将本机批准的目标应用切换到前台".into());
    }
    let mut request = args.clone();
    request["bundle_id"] = json!(target);
    native::action(&request)
}
#[cfg(not(target_os = "macos"))]
fn native_frontmost() -> Result<Value, String> {
    Err("UNSUPPORTED_PLATFORM".into())
}
#[cfg(not(target_os = "macos"))]
fn native_apps() -> Result<Value, String> {
    Err("UNSUPPORTED_PLATFORM".into())
}
#[cfg(not(target_os = "macos"))]
fn native_ui_tree() -> Result<Value, String> {
    Err("UNSUPPORTED_PLATFORM".into())
}
#[cfg(not(target_os = "macos"))]
fn native_action(_: &str, _: &Value) -> Result<Value, String> {
    Err("UNSUPPORTED_PLATFORM".into())
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
        "pendingScope": state.pending.as_ref().map(|p| p.scope.as_str()),
        "activeScope": state.grant.as_ref().map(|p| p.scope.as_str()),
        "remainingControlActions": state.grant.as_ref().filter(|p| p.scope == "control").map(|p| 100_u16.saturating_sub(p.actions_used)),
        "targetAppBundle": state.grant.as_ref().and_then(|p| p.app_bundle.as_deref()),
        "expiresInSeconds": state.grant.as_ref().map(|g| g.until.saturating_duration_since(Instant::now()).as_secs()),
        "supported": cfg!(target_os = "macos"),
        "screenRecordingGranted": screen_recording_permission(),
        "accessibilityGranted": accessibility_permission()
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

pub fn local_apps() -> Value {
    native_apps()
        .unwrap_or_else(|message| json!({ "ok": false, "error": message, "applications": [] }))
}

pub fn local_approve(request_id: &str, target_bundle: Option<&str>) -> Result<Value, String> {
    let scope = {
        let mut state = locked();
        state.expire();
        state
            .pending
            .as_ref()
            .filter(|pending| pending.id == request_id)
            .ok_or("授权申请不存在或已过期")?
            .scope
            .clone()
    };
    if screen_recording_permission() != Some(true) {
        return Err("请先授予 macOS 屏幕录制权限".into());
    }
    let app_bundle = if scope == "control" {
        if accessibility_permission() != Some(true) {
            return Err("请先授予 macOS 辅助功能权限".into());
        }
        let target = target_bundle
            .filter(|s| !s.trim().is_empty())
            .ok_or("控制授权需要在本机明确选择目标应用")?;
        let apps = native_apps()?;
        let found = apps["applications"].as_array().is_some_and(|all| {
            all.iter()
                .any(|app| app["bundle_id"].as_str() == Some(target))
        });
        if !found {
            return Err("目标应用不在当前可选择的列表中".into());
        }
        Some(target.to_owned())
    } else {
        None
    };
    locked().approve(request_id, app_bundle)?;
    Ok(local_status())
}

pub fn local_revoke() -> Value {
    // Serialize revocation with pointer and keyboard actions, so no previously
    // authorized event can start after this command returns successfully.
    let _serialized = action_lock();
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
                "action": { "type": "string", "enum": ["request", "status", "stop"] },
                "scope": { "type": "string", "enum": ["observe", "control"], "default": "observe" }
            }, "required": ["action"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false }
        }),
        json!({
            "name": "computer_observe",
            "title": "Observe computer screen",
            "description": "Observe displays/windows or return a ScreenCaptureKit screenshot as an MCP image. Requires local approval. Coordinates are global macOS points, not image pixels.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["screenshot", "windows", "ui_tree"] },
                "display_index": { "type": "integer", "minimum": 1, "maximum": 16, "default": 1 },
                "display_id": { "type": "integer", "minimum": 1 },
                "window_id": { "type": "integer", "minimum": 1 },
                "region": { "type": "object", "properties": {
                    "x": { "type": "number" }, "y": { "type": "number" },
                    "width": { "type": "number", "exclusiveMinimum": 0 },
                    "height": { "type": "number", "exclusiveMinimum": 0 }
                }, "required": ["x", "y", "width", "height"], "additionalProperties": false }
            }, "required": ["action"], "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "openWorldHint": true }
        }),
        json!({
            "name": "computer_action",
            "title": "Control approved Mac application",
            "description": "Execute one bounded mouse/keyboard action. Requires an explicit local CONTROL grant pinned to the currently foreground app. All inputs are global screen points. Password fields and risky shortcuts are blocked.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["click", "double_click", "right_click", "move", "drag", "scroll", "type", "key", "press_element", "focus_app"] },
                "x": { "type": "number" }, "y": { "type": "number" },
                "end_x": { "type": "number" }, "end_y": { "type": "number" },
                "amount": { "type": "integer", "minimum": -15, "maximum": 15 },
                "text": { "type": "string", "maxLength": 2000 },
                "label": { "type": "string", "minLength": 1, "maxLength": 160 },
                "role": { "type": "string", "maxLength": 80 },
                "snapshot_id": { "type": "string", "description": "Required for all pointer actions; returned by computer_observe screenshot; valid for 45 seconds" },
                "coordinate_space": { "type": "string", "enum": ["screen_points", "image_pixels"], "default": "screen_points" },
                "key": { "type": "string", "minLength": 1, "maxLength": 12 },
                "modifiers": { "type": "array", "uniqueItems": true, "maxItems": 4,
                    "items": { "type": "string", "enum": ["command", "shift", "control", "option"] } }
            }, "required": ["action"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true }
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
        ("computer_session", "request") => {
            let scope = args
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("observe");
            if !matches!(scope, "observe" | "control") {
                return tool_error("INVALID_SCOPE: only observe or control allowed");
            }
            match locked().request(session, scope) {
                Ok(result) => tool_ok(result),
                Err(error) => tool_error(&error),
            }
        }
        ("computer_session", "status") => {
            let mut state = locked();
            let approved = state.allowed(session);
            let pending = state.pending.as_ref().is_some_and(|p| p.session == session);
            tool_ok(
                json!({ "status": if approved { "approved" } else if pending { "pending" } else { "disabled" },
                    "scope": state.grant.as_ref().filter(|g| g.session == session).map(|g| g.scope.as_str()) }),
            )
        }
        ("computer_session", "stop") => {
            let _serialized = action_lock();
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
        ("computer_observe", "windows" | "screenshot" | "ui_tree") => {
            let Some(lease) = locked().lease_token(session) else {
                return tool_error("COMPUTER_NOT_APPROVED: 请先通过 computer_session(request) 申请，并在本机桌面应用中批准");
            };
            if action == "windows" {
                return match native_windows() {
                    Ok(value) if locked().lease_matches(session, &lease) => tool_ok(value),
                    Ok(_) => tool_error("COMPUTER_REVOKED: 授权在读取窗口期间被撤销"),
                    Err(error) => tool_error(&error),
                };
            }
            if action == "ui_tree" {
                return match native_ui_tree() {
                    Ok(value) if locked().lease_matches(session, &lease) => tool_ok(value),
                    Ok(_) => tool_error("COMPUTER_REVOKED: 授权在读取控件期间被撤销"),
                    Err(error) => tool_error(&error),
                };
            }
            match native_capture(args) {
                Ok(mut value) => {
                    if !locked().lease_matches(session, &lease) {
                        return tool_error(
                            "COMPUTER_REVOKED: 授权在截图期间被撤销，拒绝返回屏幕内容",
                        );
                    }
                    let encoded = value
                        .as_object_mut()
                        .and_then(|map| map.remove("base64"))
                        .and_then(|v| v.as_str().map(str::to_string));
                    let Some(encoded) = encoded else {
                        return tool_error("SCREEN_CAPTURE_FAILED: native capture omitted image");
                    };
                    let snapshot_id = match locked().record_snapshot(session, &lease, &value) {
                        Ok(id) => id,
                        Err(error) => return tool_error(&error),
                    };
                    value["snapshot_id"] = json!(snapshot_id);
                    value["captured_at_unix_ms"] = json!(std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis());
                    let metadata = value;
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
        ("computer_action", _) => {
            let _serialized = action_lock();
            let (target, prepared) = match locked().prepare_action(session, args) {
                Ok(prepared) => prepared,
                Err(error) => return tool_error(&error),
            };
            match native_action(&target, &prepared) {
                Ok(result) => tool_ok(result),
                Err(error) => tool_error(&error),
            }
        }
        _ => tool_error("不支持的 computer action"),
    }
}

#[cfg(target_os = "macos")]
mod native {
    use serde_json::Value;
    use std::ffi::{c_char, CStr, CString};
    unsafe extern "C" {
        fn computer_native_windows() -> *mut c_char;
        fn computer_native_capture(input: *const c_char) -> *mut c_char;
        fn computer_native_free(pointer: *mut c_char);
        fn computer_native_frontmost() -> *mut c_char;
        fn computer_native_available_apps() -> *mut c_char;
        fn computer_native_ui_tree() -> *mut c_char;
        fn computer_native_action(input: *const c_char) -> *mut c_char;
        fn computer_native_accessibility_granted() -> bool;
        fn computer_native_request_accessibility() -> bool;
    }
    fn decode(pointer: *mut c_char) -> Result<Value, String> {
        if pointer.is_null() {
            return Err("COMPUTER_NATIVE_ERROR: native bridge returned null".into());
        }
        let data = unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec();
        unsafe { computer_native_free(pointer) };
        let value: Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
        if value["ok"] != true {
            return Err(format!(
                "{}: {}",
                value["error"].as_str().unwrap_or("NATIVE_ERROR"),
                value["message"]
                    .as_str()
                    .unwrap_or("Native operation failed")
            ));
        }
        Ok(value)
    }
    pub(super) fn windows() -> Result<Value, String> {
        decode(unsafe { computer_native_windows() })
    }
    pub(super) fn capture(args: &Value) -> Result<Value, String> {
        let input = CString::new(args.to_string()).map_err(|e| e.to_string())?;
        decode(unsafe { computer_native_capture(input.as_ptr()) })
    }
    pub(super) fn frontmost() -> Result<Value, String> {
        decode(unsafe { computer_native_frontmost() })
    }
    pub(super) fn apps() -> Result<Value, String> {
        decode(unsafe { computer_native_available_apps() })
    }
    pub(super) fn ui_tree() -> Result<Value, String> {
        decode(unsafe { computer_native_ui_tree() })
    }
    pub(super) fn action(args: &Value) -> Result<Value, String> {
        let input = CString::new(args.to_string()).map_err(|e| e.to_string())?;
        decode(unsafe { computer_native_action(input.as_ptr()) })
    }
    pub(super) fn accessibility_granted() -> bool {
        unsafe { computer_native_accessibility_granted() }
    }
    pub(super) fn request_accessibility() {
        unsafe {
            computer_native_request_accessibility();
        }
    }
}

#[cfg(target_os = "macos")]
fn native_windows() -> Result<Value, String> {
    native::windows()
}
#[cfg(target_os = "macos")]
fn native_capture(args: &Value) -> Result<Value, String> {
    let mut input = args.clone();
    if args.get("window_id").is_none() && args.get("display_id").is_none() {
        let index = args
            .get("display_index")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        let screens = native_windows()?;
        let Some(display) = screens["displays"]
            .as_array()
            .and_then(|all| index.checked_sub(1).and_then(|i| all.get(i as usize)))
            .and_then(|entry| entry["id"].as_u64())
        else {
            return Err("DISPLAY_NOT_FOUND: invalid display index".into());
        };
        input["display_id"] = json!(display);
    }
    native::capture(&input)
}
#[cfg(not(target_os = "macos"))]
fn native_windows() -> Result<Value, String> {
    Err("UNSUPPORTED_PLATFORM: macOS only".into())
}
#[cfg(not(target_os = "macos"))]
fn native_capture(_: &Value) -> Result<Value, String> {
    Err("UNSUPPORTED_PLATFORM: macOS only".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_is_opt_in_and_session_scoped() {
        let mut state = Controller::default();
        assert!(!state.allowed("a"));
        let result = state.request("a", "observe").unwrap();
        assert!(state.request("b", "control").is_err());
        assert!(state.approve("wrong", None).is_err());
        state
            .approve(result["request_id"].as_str().unwrap(), None)
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
                scope: "observe".into(),
                at: Instant::now() - REQUEST_TTL,
            }),
            grant: Some(Grant {
                id: "old-grant".into(),
                session: "b".into(),
                scope: "control".into(),
                app_bundle: Some("com.example".into()),
                until: Instant::now() - Duration::from_secs(1),
                actions_used: 0,
            }),
            snapshot: None,
        };
        state.expire();
        assert!(state.pending.is_none());
        assert!(state.grant.is_none());
    }

    #[test]
    fn read_only_grant_cannot_perform_control_actions() {
        let mut state = Controller::default();
        let req = state.request("session-a", "observe").unwrap();
        state
            .approve(req["request_id"].as_str().unwrap(), None)
            .unwrap();
        assert!(state.allowed("session-a"));
        assert!(state.action_target("session-a").is_none());
        assert!(state.request("session-a", "control").is_err());
    }

    #[test]
    fn control_grant_is_bound_to_approved_app_and_session() {
        let mut state = Controller::default();
        let req = state.request("session-a", "control").unwrap();
        state
            .approve(
                req["request_id"].as_str().unwrap(),
                Some("com.apple.Safari".into()),
            )
            .unwrap();
        assert_eq!(
            state.action_target("session-a").as_deref(),
            Some("com.apple.Safari")
        );
        assert!(state.action_target("session-b").is_none());
        assert!(state.request("session-b", "observe").is_err());
    }

    #[test]
    fn rejects_invalid_action_arguments_before_native_dispatch() {
        assert!(validate_action(&json!({"action":"click","x":20})).is_err());
        assert!(validate_action(&json!({"action":"scroll","amount":200})).is_err());
        assert!(validate_action(&json!({"action":"type","text":"a".repeat(2100)})).is_err());
        assert!(
            validate_action(&json!({"action":"key","key":"a","modifiers":["not-a-key"]})).is_err()
        );
        assert!(validate_action(&json!({"action":"press_element","label":""})).is_err());
        assert!(
            validate_action(&json!({"action":"drag","x":2,"y":2,"end_x":2,"end_y":12})).is_ok()
        );
    }

    #[test]
    fn lease_invalidation_prevents_old_observation() {
        Controller::revoked_or_replaced_grant_never_authorizes_in_flight_observation();
    }

    #[test]
    fn coordinate_actions_are_bound_to_recent_capture() {
        Controller::coordinate_actions_require_recent_matching_snapshot();
    }
}
