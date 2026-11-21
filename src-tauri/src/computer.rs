//! Explicitly approved, transport-session-scoped macOS screen observation.
//!
//! This module is deliberately host-scoped, not Workspace-scoped. In particular,
//! neither the Workspace "trusted" setting nor command execution can grant it.
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::data::DataStore;

mod action_policy;
mod geometry;
mod local_api;
mod mcp_dispatch;
#[cfg(target_os = "macos")]
mod native;
mod schema;
#[cfg(test)]
mod tests;

pub(crate) use local_api::local_policy_present;
pub use local_api::{
    local_approve, local_apps, local_decide_sensitive_action, local_enable, local_pause,
    local_request_accessibility_permission, local_request_screen_permission, local_resume,
    local_revoke, local_status,
};
pub use mcp_dispatch::call_tool;
pub use schema::tool_definitions;

use action_policy::{sensitive_action_description, validate_action};
use geometry::CaptureFrame;

const REQUEST_TTL: Duration = Duration::from_secs(120);
const GRANT_TTL: Duration = Duration::from_secs(300);
const SNAPSHOT_TTL: Duration = Duration::from_secs(45);
const SENSITIVE_ACTION_TTL: Duration = Duration::from_secs(60);
const MAX_CONTROL_ACTIONS: u16 = 100;
const PERMANENT_GRANT_FALLBACK: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);
const ALLOWED_MANUAL_DURATIONS_MINUTES: &[u64] = &[0, 5, 15, 60, 480, 1440];
const COMPUTER_AUTO_SESSION_PREFIX: &str = "computer-auto-";

/// Persisted opt-in from the local desktop UI. No MCP tool may modify it.
/// None for expiry means the user explicitly chose permanent access.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ManualComputerAccess {
    pub scope: String,
    pub app_bundle: Option<String>,
    pub expires_at_ms: Option<u64>,
    #[serde(default)]
    pub paused: bool,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl ManualComputerAccess {
    fn active(&self) -> bool {
        self.expires_at_ms.is_none_or(|expiry| now_ms() < expiry)
            && matches!(self.scope.as_str(), "observe" | "control")
            && (self.scope == "observe"
                || self.app_bundle.as_deref().is_some_and(|s| !s.is_empty()))
    }
}

#[derive(Debug)]
struct Pending {
    id: String,
    session: String,
    scope: String,
    at: Instant,
}

#[derive(Debug)]
struct PendingSensitiveAction {
    id: String,
    lease_id: String,
    session: String,
    target_app: String,
    original_args: Value,
    description: String,
    created_at: Instant,
    approved: bool,
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
    /// A native ScreenCaptureKit frame was returned during this grant.
    /// This does not replace OS authorization checks on future captures.
    capture_succeeded: bool,
    paused: bool,
}

#[derive(Debug, Default)]
struct Controller {
    pending: Option<Pending>,
    grant: Option<Grant>,
    snapshot: Option<Snapshot>,
    sensitive_action: Option<PendingSensitiveAction>,
    manual: Option<ManualComputerAccess>,
}

impl Controller {
    /// Only authenticated, server-derived principal identities can enter this
    /// path (listener.rs creates the computer-auto-* session after auth).
    fn attach_manual_grant(&mut self, session: &str) {
        if !session.starts_with(COMPUTER_AUTO_SESSION_PREFIX) {
            return;
        }
        self.expire();
        let Some(policy) = self.manual.as_ref().filter(|p| p.active() && !p.paused) else {
            return;
        };
        if self.grant.as_ref().is_some_and(|g| {
            g.session == session && !g.paused && g.actions_used < MAX_CONTROL_ACTIONS
        }) {
            return;
        }
        // A principal switch invalidates any previously issued screenshot
        // or sensitive action confirmation; never share them across clients.
        let remaining = policy
            .expires_at_ms
            .map(|expiry| Duration::from_millis(expiry.saturating_sub(now_ms())))
            .unwrap_or(PERMANENT_GRANT_FALLBACK);
        self.grant = Some(Grant {
            id: uuid::Uuid::new_v4().to_string(),
            session: session.to_owned(),
            scope: policy.scope.clone(),
            app_bundle: policy.app_bundle.clone(),
            until: Instant::now() + remaining,
            actions_used: 0,
            capture_succeeded: false,
            paused: false,
        });
        self.snapshot = None;
        self.sensitive_action = None;
    }

    fn expire(&mut self) {
        if self.manual.as_ref().is_some_and(|policy| !policy.active()) {
            self.manual = None;
            self.grant = None;
            self.snapshot = None;
            self.sensitive_action = None;
        }
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
        if self.sensitive_action.as_ref().is_some_and(|p| {
            p.created_at.elapsed() >= SENSITIVE_ACTION_TTL
                || self
                    .grant
                    .as_ref()
                    .is_none_or(|g| g.id != p.lease_id || g.paused)
        }) {
            self.sensitive_action = None;
        }
    }

    fn stop_session(&mut self, session: &str) -> Result<Value, String> {
        self.expire();
        if session.starts_with(COMPUTER_AUTO_SESSION_PREFIX) {
            if self
                .grant
                .as_ref()
                .is_some_and(|grant| grant.session == session)
            {
                self.grant = None;
                self.snapshot = None;
                self.sensitive_action = None;
            }
            return Ok(json!({
                "status": "stopped",
                "note": "本机主动授权仍然开启，仅停止当前客户端操作；需在桌面设置中关闭全局授权"
            }));
        }
        if self.grant.as_ref().is_some_and(|p| p.session == session)
            || self.pending.as_ref().is_some_and(|p| p.session == session)
        {
            // Never wipe `self.manual`: remote MCP sessions must never revoke
            // a local desktop opt-in policy held in memory.
            self.grant = None;
            self.pending = None;
            self.snapshot = None;
            self.sensitive_action = None;
            Ok(json!({ "status": "stopped" }))
        } else {
            Err("会话无权终止其他客户端的 Computer Use 授权".into())
        }
    }

    fn request(&mut self, session: &str, scope: &str) -> Result<Value, String> {
        self.expire();
        if let Some(grant) = &self.grant {
            if grant.session == session {
                if grant.scope != scope {
                    return Err("切换读取/控制权限时请先撤销现有授权，再重新申请".into());
                }
                return Ok(json!({
                    "status": if grant.paused { "paused" } else { "approved" },
                    "expires_in_seconds": grant.until.saturating_duration_since(Instant::now()).as_secs()
                }));
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
        Ok(json!({
            "status": "pending",
            "scope": scope,
            "request_id": id,
            "message": "请用户在 Coding Tools MCP 本机设置中审批 Computer Use；未经批准不能查看或控制屏幕"
        }))
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
        self.sensitive_action = None;
        self.grant = Some(Grant {
            id: uuid::Uuid::new_v4().to_string(),
            session,
            scope,
            app_bundle,
            until: Instant::now() + GRANT_TTL,
            actions_used: 0,
            capture_succeeded: false,
            paused: false,
        });
        Ok(())
    }

    fn allowed(&mut self, session: &str) -> bool {
        self.expire();
        self.grant
            .as_ref()
            .is_some_and(|grant| grant.session == session && !grant.paused)
    }

    /// Snapshot the exact lease, not just the session identifier: a revoked
    /// lease must not release a screenshot after another grant is approved.
    fn lease_token(&mut self, session: &str) -> Option<String> {
        self.expire();
        self.grant
            .as_ref()
            .filter(|grant| grant.session == session && !grant.paused)
            .map(|grant| grant.id.clone())
    }

    fn lease_matches(&mut self, session: &str, token: &str) -> bool {
        self.lease_token(session).as_deref() == Some(token)
    }

    fn observation_context(&mut self, session: &str) -> Option<(String, Option<String>)> {
        self.expire();
        let grant = self
            .grant
            .as_ref()
            .filter(|g| g.session == session && !g.paused)?;
        Some((grant.id.clone(), grant.app_bundle.clone()))
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
        if let Some(grant) = self.grant.as_mut() {
            grant.capture_succeeded = true;
        }
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

    fn active_control_grant(&self, session: &str, denied_message: &str) -> Result<&Grant, String> {
        self.grant
            .as_ref()
            .filter(|g| {
                g.session == session
                    && g.scope == "control"
                    && !g.paused
                    && g.actions_used < MAX_CONTROL_ACTIONS
            })
            .ok_or_else(|| denied_message.to_owned())
    }

    #[cfg(test)]
    fn prepare_action(&mut self, session: &str, args: &Value) -> Result<(String, Value), String> {
        self.expire();
        validate_action(args)?;
        self.prepare_validated_action(session, args)
    }

    fn prepare_validated_action(
        &mut self,
        session: &str,
        args: &Value,
    ) -> Result<(String, Value), String> {
        let grant = self.active_control_grant(
            session,
            "CONTROL_NOT_APPROVED: 请用户在本机以 control 模式授权此会话，或重新批准操作额度",
        )?;
        let target = grant.app_bundle.clone().ok_or("CONTROL_TARGET_REQUIRED")?;
        let action = args["action"].as_str().unwrap_or("");
        let mut prepared = if matches!(
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
        // Never forward untrusted values of these internal privileged fields
        // to the Objective-C action bridge.
        strip_internal_action_fields(&mut prepared);
        if let Some(grant) = self.grant.as_mut() {
            grant.actions_used += 1;
        }
        Ok((target, prepared))
    }

    fn prepare_or_request_sensitive_action(
        &mut self,
        session: &str,
        args: &Value,
    ) -> Result<(String, Value), String> {
        self.expire();
        validate_action(args)?;
        let Some(description) = sensitive_action_description(args) else {
            return self.prepare_validated_action(session, args);
        };
        let grant =
            self.active_control_grant(session, "CONTROL_NOT_APPROVED: 请先在本机批准控制会话")?;
        let target = grant
            .app_bundle
            .as_ref()
            .ok_or("CONTROL_TARGET_REQUIRED")?
            .clone();
        let lease = grant.id.clone();
        let mut canonical = args.clone();
        strip_internal_action_fields(&mut canonical);
        let confirmation = args.get("confirmation_id").and_then(Value::as_str);
        if let Some(existing) = &self.sensitive_action {
            if existing.session != session
                || existing.lease_id != lease
                || existing.original_args != canonical
                || confirmation.is_some_and(|id| id != existing.id)
            {
                return Err("CONFIRMATION_MISMATCH: 还有其他待确认动作或调用参数已改变".into());
            }
            if existing.approved && confirmation == Some(existing.id.as_str()) {
                self.sensitive_action = None; // one-time permit, consumed before dispatch
                let (target, mut prepared) = self.prepare_validated_action(session, &canonical)?;
                prepared["high_risk_confirmed"] = json!(true);
                return Ok((target, prepared));
            }
            return Err(format!(
                "CONFIRMATION_PENDING: 请在本机明确确认操作（ID: {}）后携带 confirmation_id 原样重试",
                existing.id
            ));
        }
        if confirmation.is_some() {
            return Err("CONFIRMATION_INVALID: 未找到对应待批准动作".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.sensitive_action = Some(PendingSensitiveAction {
            id: id.clone(),
            lease_id: lease,
            session: session.to_owned(),
            target_app: target,
            original_args: canonical,
            description,
            created_at: Instant::now(),
            approved: false,
        });
        Err(format!(
            "CONFIRMATION_PENDING: 需要在本机确认敏感操作（ID: {id}），批准后携带 confirmation_id 原样重试"
        ))
    }

    fn decide_sensitive_action(&mut self, id: &str, approve: bool) -> Result<(), String> {
        self.expire();
        let pending = self
            .sensitive_action
            .as_mut()
            .filter(|p| p.id == id)
            .ok_or("敏感操作确认不存在或已过期")?;
        if !approve {
            self.sensitive_action = None;
            return Ok(());
        }
        let grant = self
            .grant
            .as_ref()
            .filter(|g| g.id == pending.lease_id && !g.paused && g.scope == "control")
            .ok_or("控制授权已失效或暂停")?;
        if grant.app_bundle.as_deref() != Some(pending.target_app.as_str()) {
            return Err("授权目标应用已改变".into());
        }
        pending.approved = true;
        Ok(())
    }
}

fn strip_internal_action_fields(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("high_risk_confirmed");
        object.remove("confirmation_id");
        object.remove("bundle_id");
    }
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
fn native_ui_tree(expected: Option<&str>) -> Result<Value, String> {
    native::ui_tree(expected)
}
#[cfg(target_os = "macos")]
fn native_action(target: &str, args: &Value) -> Result<Value, String> {
    if !matches!(args["action"].as_str(), Some("focus_app" | "open_url"))
        && native_frontmost()?["bundle_id"].as_str() != Some(target)
    {
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
fn native_ui_tree(_: Option<&str>) -> Result<Value, String> {
    Err("UNSUPPORTED_PLATFORM".into())
}
#[cfg(not(target_os = "macos"))]
fn native_action(_: &str, _: &Value) -> Result<Value, String> {
    Err("UNSUPPORTED_PLATFORM".into())
}

fn control() -> &'static Mutex<Controller> {
    static CONTROL: OnceLock<Mutex<Controller>> = OnceLock::new();
    CONTROL.get_or_init(|| {
        let manual = DataStore::read_file(|data| Ok(data.manual_computer_access.clone()))
            .unwrap_or_default()
            .filter(ManualComputerAccess::active);
        Mutex::new(Controller {
            manual,
            ..Controller::default()
        })
    })
}

fn locked() -> std::sync::MutexGuard<'static, Controller> {
    control()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Read-only macOS TCC diagnostics. TCC permissions are tied to the running
/// binary/bundle, not to the display name visible in System Settings.
#[cfg(target_os = "macos")]
fn screen_permission_diagnostics() -> Value {
    native::screen_diagnostics().unwrap_or_else(|message| json!({"ok": false, "error": message}))
}

#[cfg(not(target_os = "macos"))]
fn screen_permission_diagnostics() -> Value {
    json!({"ok": false, "error": "UNSUPPORTED_PLATFORM"})
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
}

#[cfg(target_os = "macos")]
fn screen_recording_permission() -> Option<bool> {
    Some(unsafe { CGPreflightScreenCaptureAccess() })
}

#[cfg(not(target_os = "macos"))]
fn screen_recording_permission() -> Option<bool> {
    None
}

#[cfg(target_os = "macos")]
fn native_windows() -> Result<Value, String> {
    native::windows()
}
#[cfg(target_os = "macos")]
fn native_capture(args: &Value, expected_bundle: Option<&str>) -> Result<Value, String> {
    let mut input = args.clone();
    if let Some(obj) = input.as_object_mut() {
        obj.remove("expected_bundle_id");
    }
    if let Some(bundle) = expected_bundle {
        if args.get("window_id").and_then(Value::as_u64).is_none() {
            return Err("TARGET_WINDOW_REQUIRED: control 模式必须指定授权应用的 window_id，先调用 computer_observe(windows)".into());
        }
        input["expected_bundle_id"] = json!(bundle);
    }
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
fn native_capture(_: &Value, _: Option<&str>) -> Result<Value, String> {
    Err("UNSUPPORTED_PLATFORM: macOS only".into())
}
