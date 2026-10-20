//! Explicitly approved, transport-session-scoped macOS screen observation.
//!
//! This module is deliberately host-scoped, not Workspace-scoped. In particular,
//! neither the Workspace "trusted" setting nor command execution can grant it.
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::computer_geometry::CaptureFrame;
use crate::data::DataStore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const REQUEST_TTL: Duration = Duration::from_secs(120);
const GRANT_TTL: Duration = Duration::from_secs(300);
const SNAPSHOT_TTL: Duration = Duration::from_secs(45);
const SENSITIVE_ACTION_TTL: Duration = Duration::from_secs(60);

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

/// The listener may use this only AFTER authenticating the MCP request.
/// An inactive, paused or expired opt-in never bypasses the old session flow.
pub(crate) fn local_policy_present() -> bool {
    let mut state = locked();
    state.expire();
    state
        .manual
        .as_ref()
        .is_some_and(ManualComputerAccess::active)
}

/// Called only by a locally invoked Tauri command. No remote tool can change
/// the policy. Duration 0 is an explicit permanent opt-in.
pub fn local_enable(
    scope: &str,
    duration_minutes: u64,
    target: Option<&str>,
) -> Result<Value, String> {
    if !matches!(scope, "observe" | "control") {
        return Err("INVALID_SCOPE: 仅允许查看或控制".into());
    }
    if ![0, 5, 15, 60, 480, 1440].contains(&duration_minutes) {
        return Err("INVALID_DURATION: 可选择 5/15/60 分钟、8/24 小时或永久".into());
    }
    let app_bundle = if scope == "control" {
        if accessibility_permission() != Some(true) {
            return Err("启用控制前，请先授予 macOS 辅助功能权限".into());
        }
        let target = target
            .filter(|value| !value.is_empty())
            .ok_or("启用控制必须在本机选择目标应用")?;
        let apps = native_apps()?;
        let exists = apps["applications"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["bundle_id"].as_str() == Some(target))
        });
        if !exists {
            return Err("选择的应用当前不可用，请刷新应用列表".into());
        }
        Some(target.to_owned())
    } else {
        None
    };
    let policy = ManualComputerAccess {
        scope: scope.to_owned(),
        app_bundle,
        expires_at_ms: (duration_minutes != 0)
            .then(|| now_ms().saturating_add(duration_minutes * 60_000)),
        paused: false,
    };
    let _serialized = action_lock();
    DataStore::update_file(|data| {
        data.manual_computer_access = Some(policy.clone());
        Ok(())
    })
    .map_err(|error| error.to_string())?;
    let mut state = locked();
    *state = Controller {
        manual: Some(policy),
        ..Controller::default()
    };
    drop(state);
    Ok(local_status())
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

/// Only the local desktop UI can approve a specific action. This does not
/// execute anything: the originating MCP transport must retry the exact action.
pub fn local_decide_sensitive_action(id: &str, approve: bool) -> Result<Value, String> {
    let _serialized = action_lock();
    locked().decide_sensitive_action(id, approve)?;
    Ok(local_status())
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

/// Pausing is a local-only emergency gate. Resume requires a fresh screenshot
/// for pointer actions; a paused lease cannot observe or inject events.
pub fn local_pause() -> Result<Value, String> {
    let _serialized = action_lock();
    if let Some(mut policy) = locked().manual.clone() {
        policy.paused = true;
        DataStore::update_file(|data| {
            data.manual_computer_access = Some(policy.clone());
            Ok(())
        })
        .map_err(|error| error.to_string())?;
    }
    let mut state = locked();
    state.expire();
    if let Some(policy) = state.manual.as_mut() {
        policy.paused = true;
    }
    if let Some(grant) = state.grant.as_mut() {
        grant.paused = true;
        state.snapshot = None;
    }
    state.sensitive_action = None;
    drop(state);
    Ok(local_status())
}

pub fn local_resume() -> Result<Value, String> {
    // CGPreflightScreenCaptureAccess can disagree with ScreenCaptureKit for
    // a correctly user-authorized app (notably across ad-hoc rebuilds).
    // The approved, scoped ScreenCaptureKit call enforces macOS TCC itself.
    let _serialized = action_lock();
    if let Some(mut policy) = locked().manual.clone() {
        if !policy.active() {
            return Err("Computer Use 授权已过期，需在本机重新开启".into());
        }
        if policy.scope == "control" && accessibility_permission() != Some(true) {
            return Err("ACCESSIBILITY_PERMISSION_REQUIRED: 辅助功能权限已撤销".into());
        }
        policy.paused = false;
        DataStore::update_file(|data| {
            data.manual_computer_access = Some(policy.clone());
            Ok(())
        })
        .map_err(|error| error.to_string())?;
    }
    let mut state = locked();
    state.expire();
    if let Some(policy) = state.manual.as_mut() {
        policy.paused = false;
    }
    if let Some(grant) = state.grant.as_mut() {
        if grant.scope == "control" && accessibility_permission() != Some(true) {
            return Err("ACCESSIBILITY_PERMISSION_REQUIRED: 辅助功能权限已撤销".into());
        }
        grant.paused = false;
        state.snapshot = None;
    }
    drop(state);
    Ok(local_status())
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
        if !session.starts_with("computer-auto-") {
            return;
        }
        self.expire();
        let Some(policy) = self.manual.as_ref().filter(|p| p.active() && !p.paused) else {
            return;
        };
        if self
            .grant
            .as_ref()
            .is_some_and(|g| g.session == session && !g.paused && g.actions_used < 100)
        {
            return;
        }
        // A principal switch invalidates any previously issued screenshot
        // or sensitive action confirmation; never share them across clients.
        let remaining = policy
            .expires_at_ms
            .map(|expiry| Duration::from_millis(expiry.saturating_sub(now_ms())))
            .unwrap_or_else(|| Duration::from_secs(100 * 365 * 24 * 60 * 60));
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
        assert!(!state.grant.as_ref().unwrap().capture_succeeded);
        let geometry = json!({
            "origin_x": -1440.0, "origin_y": 0.0, "point_width": 1440.0,
            "point_height": 900.0, "pixel_width": 1920, "pixel_height": 1200
        });
        let snap = state.record_snapshot("s", &lease, &geometry).unwrap();
        assert!(state.grant.as_ref().unwrap().capture_succeeded);
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
            grant.session == session
                && grant.scope == "control"
                && !grant.paused
                && grant.actions_used < 100
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
                    json!({ "status": if grant.paused { "paused" } else { "approved" },
                        "expires_in_seconds": grant.until.saturating_duration_since(Instant::now()).as_secs() }),
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

    fn prepare_action(&mut self, session: &str, args: &Value) -> Result<(String, Value), String> {
        self.expire();
        validate_action(args)?;
        let grant = self
            .grant
            .as_ref()
            .filter(|g| {
                g.session == session && g.scope == "control" && !g.paused && g.actions_used < 100
            })
            .ok_or(
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
        if let Some(object) = prepared.as_object_mut() {
            object.remove("high_risk_confirmed");
            object.remove("confirmation_id");
            object.remove("bundle_id");
        }
        self.grant.as_mut().expect("checked grant").actions_used += 1;
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
            return self.prepare_action(session, args);
        };
        let grant = self
            .grant
            .as_ref()
            .filter(|g| {
                g.session == session && g.scope == "control" && !g.paused && g.actions_used < 100
            })
            .ok_or("CONTROL_NOT_APPROVED: 请先在本机批准控制会话")?;
        let target = grant
            .app_bundle
            .as_ref()
            .ok_or("CONTROL_TARGET_REQUIRED")?
            .clone();
        let lease = grant.id.clone();
        let mut canonical = args.clone();
        if let Some(object) = canonical.as_object_mut() {
            object.remove("confirmation_id");
            object.remove("high_risk_confirmed");
            object.remove("bundle_id");
        }
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
                let (target, mut prepared) = self.prepare_action(session, &canonical)?;
                prepared["high_risk_confirmed"] = json!(true);
                return Ok((target, prepared));
            }
            return Err(format!("CONFIRMATION_PENDING: 请在本机明确确认操作（ID: {}）后携带 confirmation_id 原样重试", existing.id));
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
        Err(format!("CONFIRMATION_PENDING: 需要在本机确认敏感操作（ID: {id}），批准后携带 confirmation_id 原样重试"))
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

fn sensitive_action_description(args: &Value) -> Option<String> {
    match args.get("action").and_then(Value::as_str)? {
        "press_element" => {
            let label = args["label"].as_str()?.trim();
            let normalized = label.to_lowercase();
            let sensitive = [
                "delete",
                "remove",
                "pay",
                "purchase",
                "submit",
                "send",
                "erase",
                "delete all",
                "transfer",
                "删除",
                "支付",
                "付款",
                "购买",
                "提交",
                "发送",
                "转账",
                "移除",
                "清空",
                "确认订单",
                "卸载",
                "永久",
            ];
            sensitive
                .iter()
                .any(|needle| normalized.contains(needle))
                .then(|| format!("语义按钮：{}", label.chars().take(100).collect::<String>()))
        }
        "key" => {
            let modifiers = args.get("modifiers")?.as_array()?;
            let command = modifiers.iter().any(|v| v == "command");
            let control = modifiers.iter().any(|v| v == "control");
            let key = args["key"].as_str()?.to_ascii_lowercase();
            ((command && ["q", "w", "v", "delete", "backspace"].contains(&key.as_str()))
                || (control && ["delete", "backspace"].contains(&key.as_str())))
            .then(|| {
                format!(
                    "高风险快捷键：{} + {}",
                    if command { "Command" } else { "Control" },
                    key
                )
            })
        }
        _ => None,
    }
}

fn validate_action(args: &Value) -> Result<(), String> {
    let action = args.get("action").and_then(Value::as_str).unwrap_or("");
    let required = match action {
        "click" | "double_click" | "right_click" | "move" => &["x", "y"][..],
        "drag" => &["x", "y", "end_x", "end_y"][..],
        "scroll" => &["amount"][..],
        "type" => &["text"][..],
        "open_url" => &["url"][..],
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
        if matches!(*field, "text" | "key" | "label" | "url") && !value.is_string() {
            return Err(format!("INVALID_ARGUMENT: {field} must be string"));
        }
    }
    if action == "type" && args["text"].as_str().unwrap_or("").encode_utf16().count() > 2000 {
        return Err("INVALID_TEXT: too long".into());
    }
    if action == "open_url" {
        let url = args["url"].as_str().unwrap_or("");
        if url.len() > 2048
            || url.chars().any(char::is_whitespace)
            || url.chars().any(char::is_control)
            || !(url.starts_with("https://") || url.starts_with("http://"))
        {
            return Err("INVALID_BROWSER_URL: only bounded HTTP(S) URLs are supported".into());
        }
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

/// Local Tauri UI only; never expose local_approve/local_revoke as MCP tools.
pub fn local_status() -> Value {
    let mut state = locked();
    state.expire();
    let manual = state.manual.as_ref();
    let manual_enabled = manual.is_some_and(ManualComputerAccess::active);
    let manually_paused = manual.is_some_and(|p| p.paused);
    let diagnostics = screen_permission_diagnostics();
    // Use the same native preflight result for the main status and the
    // diagnostics; two separate checks in one UI poll can disagree.
    let screen_granted = diagnostics["preflight_granted"]
        .as_bool()
        .or_else(screen_recording_permission);
    json!({
        "pendingRequestId": state.pending.as_ref().map(|p| p.id.as_str()),
        "pendingExpiresInSeconds": state.pending.as_ref().map(|p| REQUEST_TTL.saturating_sub(p.at.elapsed()).as_secs()),
        "active": manual_enabled || state.grant.is_some(),
        "manualEnabled": manual_enabled,
        "manualScope": manual.map(|p| p.scope.as_str()),
        "manualTargetApp": manual.and_then(|p| p.app_bundle.as_deref()),
        "manualExpiresAtMs": manual.and_then(|p| p.expires_at_ms),
        "manualPaused": manually_paused,
        "pendingScope": state.pending.as_ref().map(|p| p.scope.as_str()),
        "activeScope": manual.map(|p| p.scope.as_str()).or_else(|| state.grant.as_ref().map(|p| p.scope.as_str())),
        "paused": manually_paused || state.grant.as_ref().is_some_and(|g| g.paused),
        "pendingSensitiveActionId": state.sensitive_action.as_ref().map(|p| p.id.as_str()),
        "pendingSensitiveActionDescription": state.sensitive_action.as_ref().map(|p| p.description.as_str()),
        "pendingSensitiveActionApp": state.sensitive_action.as_ref().map(|p| p.target_app.as_str()),
        "pendingSensitiveActionApproved": state.sensitive_action.as_ref().is_some_and(|p| p.approved),
        "pendingSensitiveActionExpiresInSeconds": state.sensitive_action.as_ref().map(|p|
            SENSITIVE_ACTION_TTL.saturating_sub(p.created_at.elapsed()).as_secs()),
        "remainingControlActions": state.grant.as_ref().filter(|p| p.scope == "control").map(|p| 100_u16.saturating_sub(p.actions_used)),
        "targetAppBundle": manual.and_then(|p| p.app_bundle.as_deref()).or_else(|| state.grant.as_ref().and_then(|p| p.app_bundle.as_deref())),
        "expiresInSeconds": if manual_enabled { manual.and_then(|p| p.expires_at_ms).map(|ms| ms.saturating_sub(now_ms()) / 1000) } else { state.grant.as_ref().map(|g| g.until.saturating_duration_since(Instant::now()).as_secs()) },
        "supported": cfg!(target_os = "macos"),
        "screenRecordingGranted": screen_granted,
        "screenCaptureSucceededInSession": state.grant.as_ref().is_some_and(|grant| grant.capture_succeeded),
        "screenPermissionDiagnostic": diagnostics,
        "accessibilityGranted": accessibility_permission()
    })
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
    // A false CGPreflightScreenCaptureAccess is diagnostic only. This function
    // approves an in-memory MCP lease after a LOCAL user gesture; it does not
    // grant macOS Screen Recording. The native ScreenCaptureKit API remains
    // responsible for refusing any actual unauthorised observation.
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

pub fn local_revoke() -> Result<Value, String> {
    // Serialize revocation with pointer and keyboard actions, so no previously
    // authorized event can start after this command returns successfully.
    let _serialized = action_lock();
    DataStore::update_file(|data| {
        data.manual_computer_access = None;
        Ok(())
    })
    .map_err(|error| error.to_string())?;
    *locked() = Controller::default();
    Ok(local_status())
}

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "computer_session",
            "title": "Computer session",
            "description": "Legacy temporary Computer Use session (request/status/stop). If the Mac owner already enabled Computer Use in local settings, DO NOT request a session: use computer_observe / computer_action directly, without session_token. For legacy temporary approvals only, call request and retain the private session_token across calls. Never expose tokens. Only local desktop UI can enable persistent access.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["request", "status", "stop"] },
                "scope": { "type": "string", "enum": ["observe", "control"], "default": "observe" },
                "session_token": { "type": "string", "description": "Needed only when using legacy per-session approval without a persistent MCP header; NOT required for locally enabled Computer Use." }
            }, "required": ["action"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false }
        }),
        json!({
            "name": "computer_observe",
            "title": "Observe computer screen",
            "description": "Read windows or a ScreenCaptureKit screenshot. If the Mac user manually enabled observe/control in Coding Tools MCP settings, call directly with NO computer_session(request) and NO session_token. Each OAuth/Bearer-authenticated request is bound to that client's identity and audited. If the user has not enabled it, use legacy per-session approval. Coordinates are global macOS points.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["screenshot", "windows", "ui_tree"] },
                "session_token": { "type": "string", "description": "Legacy approvals only; omit for locally enabled Computer Use." },
                "display_index": { "type": "integer", "minimum": 1, "maximum": 16, "default": 1 },
                "display_id": { "type": "integer", "minimum": 1 },
                "window_id": { "type": "integer", "minimum": 1 },
                "quality": { "type": "string", "enum": ["fast", "detailed"], "default": "fast", "description": "Fast 1280x850 JPEG by default; detailed up to 1920x1200." },
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
            "description": "Execute mouse/keyboard action ONLY while Mac owner enabled local CONTROL permission for one selected application. When local policy is enabled, call directly without computer_session(request) or session_token; requests are authenticated and audited. Otherwise legacy per-session approval applies. Pointer actions require a recent matching snapshot_id; sensitive shortcuts/semantic send/delete/payment require separate 60-second local one-time confirmation.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["click", "double_click", "right_click", "move", "drag", "scroll", "type", "key", "press_element", "focus_app", "open_url"] },
                "session_token": { "type": "string", "description": "Legacy approvals only; omit for locally enabled Computer Use." },
                "x": { "type": "number" }, "y": { "type": "number" },
                "end_x": { "type": "number" }, "end_y": { "type": "number" },
                "amount": { "type": "integer", "minimum": -15, "maximum": 15 },
                "text": { "type": "string", "maxLength": 2000 },
                "url": { "type": "string", "maxLength": 2048, "description": "HTTP(S) navigation of the locally approved web browser; no file or script schemes." },
                "label": { "type": "string", "minLength": 1, "maxLength": 160 },
                "role": { "type": "string", "maxLength": 80 },
                "snapshot_id": { "type": "string", "description": "Required for all pointer actions; returned by computer_observe screenshot; valid for 45 seconds" },
                "confirmation_id": { "type": "string", "description": "For locally approved sensitive actions only. After approval retry the exact same request with this ID; one-time consumption, 60 second TTL." },
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
    // Only verified principals received a server-created computer-auto-*
    // session in listener.rs. The local policy controls the permission.
    locked().attach_manual_grant(session);
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
            state.expire();
            let approved = state.allowed(session);
            let pending = state.pending.as_ref().is_some_and(|p| p.session == session);
            let paused = state
                .grant
                .as_ref()
                .is_some_and(|g| g.session == session && g.paused)
                || state
                    .manual
                    .as_ref()
                    .is_some_and(|policy| policy.paused && session.starts_with("computer-auto-"));
            tool_ok(
                json!({ "status": if paused { "paused" } else if approved { "approved" } else if pending { "pending" } else { "disabled" },
                    "scope": state.grant.as_ref().filter(|g| g.session == session).map(|g| g.scope.as_str()),
                    "authorization": if session.starts_with("computer-auto-") { "local_policy" } else { "session" } }),
            )
        }
        ("computer_session", "stop") => {
            let _serialized = action_lock();
            let mut state = locked();
            state.expire();
            if session.starts_with("computer-auto-") {
                if state
                    .grant
                    .as_ref()
                    .is_some_and(|grant| grant.session == session)
                {
                    state.grant = None;
                    state.snapshot = None;
                    state.sensitive_action = None;
                }
                return tool_ok(
                    json!({"status":"stopped", "note":"本机主动授权仍然开启，仅停止当前客户端操作；需在桌面设置中关闭全局授权"}),
                );
            }
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
            let Some((lease, target_app)) = locked().observation_context(session) else {
                return tool_error("COMPUTER_NOT_APPROVED: 请先通过 computer_session(request) 申请，并在本机桌面应用中批准");
            };
            if action == "windows" {
                return match native_windows() {
                    Ok(mut value) if locked().lease_matches(session, &lease) => {
                        if let Some(target) = target_app.as_deref() {
                            if let Some(windows) = value["windows"].as_array_mut() {
                                windows.retain(|w| {
                                    w["bundle_id"].as_str() == Some(target)
                                        && w["on_screen"] == true
                                        && w["layer"] == 0
                                });
                            }
                        }
                        tool_ok(value)
                    }
                    Ok(_) => tool_error("COMPUTER_REVOKED: 授权在读取窗口期间被撤销"),
                    Err(error) => tool_error(&error),
                };
            }
            if action == "ui_tree" {
                return match native_ui_tree(target_app.as_deref()) {
                    Ok(value) if locked().lease_matches(session, &lease) => tool_ok(value),
                    Ok(_) => tool_error("COMPUTER_REVOKED: 授权在读取控件期间被撤销"),
                    Err(error) => tool_error(&error),
                };
            }
            match native_capture(args, target_app.as_deref()) {
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
            let (target, prepared) =
                match locked().prepare_or_request_sensitive_action(session, args) {
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
        fn computer_native_screen_diagnostics() -> *mut c_char;
        fn computer_native_windows() -> *mut c_char;
        fn computer_native_capture(input: *const c_char) -> *mut c_char;
        fn computer_native_free(pointer: *mut c_char);
        fn computer_native_frontmost() -> *mut c_char;
        fn computer_native_available_apps() -> *mut c_char;
        fn computer_native_ui_tree(expected_bundle: *const c_char) -> *mut c_char;
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
    pub(super) fn screen_diagnostics() -> Result<Value, String> {
        decode(unsafe { computer_native_screen_diagnostics() })
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
    pub(super) fn ui_tree(expected: Option<&str>) -> Result<Value, String> {
        let input = expected
            .map(CString::new)
            .transpose()
            .map_err(|e| e.to_string())?;
        decode(unsafe {
            computer_native_ui_tree(input.as_ref().map_or(std::ptr::null(), |x| x.as_ptr()))
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_browser_navigation_validates_scheme_and_does_not_require_a_snapshot() {
        let mut state = Controller {
            manual: Some(ManualComputerAccess {
                scope: "control".into(),
                app_bundle: Some("com.microsoft.edgemac".into()),
                expires_at_ms: None,
                paused: false,
            }),
            ..Controller::default()
        };
        let client = "computer-auto-verified-client";
        state.attach_manual_grant(client);
        let (target, prepared) = state
            .prepare_action(
                client,
                &json!({
                    "action": "open_url", "url": "https://github.com"
                }),
            )
            .expect("one-call navigation should not need a screenshot");
        assert_eq!(target, "com.microsoft.edgemac");
        assert_eq!(prepared["url"], "https://github.com");

        for address in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://github.com\nunsafe",
            "https://github.com space",
            "https://github.com".repeat(150).as_str(),
        ] {
            assert!(validate_action(&json!({"action":"open_url","url":address})).is_err());
        }
        let mut observation_only = Controller {
            manual: Some(ManualComputerAccess {
                scope: "observe".into(),
                app_bundle: None,
                expires_at_ms: None,
                paused: false,
            }),
            ..Controller::default()
        };
        observation_only.attach_manual_grant(client);
        assert!(observation_only
            .prepare_action(
                client,
                &json!({
                    "action":"open_url", "url":"https://github.com"
                })
            )
            .is_err());
    }

    fn manual_policy(scope: &str, expires_at_ms: Option<u64>) -> ManualComputerAccess {
        ManualComputerAccess {
            scope: scope.to_owned(),
            app_bundle: (scope == "control").then(|| "com.apple.Safari".to_owned()),
            expires_at_ms,
            paused: false,
        }
    }

    #[test]
    fn manual_observe_allows_authenticated_principal_only_and_never_control() {
        let mut state = Controller {
            manual: Some(manual_policy("observe", Some(now_ms() + 30_000))),
            ..Controller::default()
        };
        assert!(!state.allowed("computer-auto-client-a"));
        state.attach_manual_grant("client-supplied-session");
        assert!(!state.allowed("client-supplied-session"));
        state.attach_manual_grant("computer-auto-client-a");
        assert!(state.allowed("computer-auto-client-a"));
        assert!(state
            .observation_context("computer-auto-client-a")
            .is_some());
        assert!(state
            .prepare_action("computer-auto-client-a", &json!({"action":"focus_app"}))
            .is_err());
        assert!(!state.allowed("computer-auto-client-b"));
    }

    #[test]
    fn manual_permanent_survives_serialization_but_pause_and_expiry_deny() {
        let permanent = manual_policy("observe", None);
        let restored: ManualComputerAccess =
            serde_json::from_str(&serde_json::to_string(&permanent).unwrap()).unwrap();
        assert!(restored.active());
        let mut state = Controller {
            manual: Some(restored),
            ..Controller::default()
        };
        state.attach_manual_grant("computer-auto-client");
        let previous_lease = state.lease_token("computer-auto-client").unwrap();
        state.manual.as_mut().unwrap().paused = true;
        state.grant.as_mut().unwrap().paused = true;
        assert!(state.lease_token("computer-auto-client").is_none());
        state.manual.as_mut().unwrap().paused = false;
        state.grant.as_mut().unwrap().paused = false;
        assert!(state.lease_matches("computer-auto-client", &previous_lease));
        state.manual.as_mut().unwrap().expires_at_ms = Some(now_ms().saturating_sub(1));
        state.expire();
        assert!(state.manual.is_none());
        assert!(!state.allowed("computer-auto-client"));
    }

    #[test]
    fn different_remote_principals_cannot_reuse_snapshots_or_action_lease() {
        let mut state = Controller {
            manual: Some(manual_policy("control", None)),
            ..Controller::default()
        };
        state.attach_manual_grant("computer-auto-client-a");
        let token_a = state.lease_token("computer-auto-client-a").unwrap();
        state.attach_manual_grant("computer-auto-client-b");
        assert!(!state.lease_matches("computer-auto-client-a", &token_a));
        assert!(state.lease_token("computer-auto-client-b").is_some());
        assert!(state.snapshot.is_none());
    }

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
                capture_succeeded: false,
                paused: false,
            }),
            snapshot: None,
            sensitive_action: None,
            manual: None,
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

    #[test]
    fn paused_control_lease_denies_observe_and_input_until_resumed() {
        let mut state = Controller::default();
        let req = state.request("client-a", "control").unwrap();
        state
            .approve(
                req["request_id"].as_str().unwrap(),
                Some("com.apple.Safari".into()),
            )
            .unwrap();
        assert!(state.lease_token("client-a").is_some());
        state.grant.as_mut().unwrap().paused = true;
        assert!(state.lease_token("client-a").is_none());
        assert!(state
            .prepare_action("client-a", &json!({"action": "focus_app"}))
            .is_err());
        assert_eq!(
            state.request("client-a", "control").unwrap()["status"],
            "paused"
        );
        state.grant.as_mut().unwrap().paused = false;
        assert!(state.lease_token("client-a").is_some());
        assert!(state
            .prepare_action("client-a", &json!({"action": "focus_app"}))
            .is_ok());
    }
    fn control_session() -> Controller {
        let mut state = Controller::default();
        let req = state.request("client-a", "control").unwrap();
        state
            .approve(
                req["request_id"].as_str().unwrap(),
                Some("com.apple.Safari".into()),
            )
            .unwrap();
        state
    }

    #[test]
    fn high_risk_confirmation_is_local_exact_and_single_use() {
        let mut state = control_session();
        let action = json!({"action":"press_element","label":"Delete all"});
        let pending = state
            .prepare_or_request_sensitive_action("client-a", &action)
            .unwrap_err();
        assert!(pending.contains("CONFIRMATION_PENDING"));
        assert_eq!(state.grant.as_ref().unwrap().actions_used, 0);
        let id = state.sensitive_action.as_ref().unwrap().id.clone();
        assert!(state
            .prepare_or_request_sensitive_action("client-b", &action)
            .is_err());
        assert!(state
            .prepare_or_request_sensitive_action(
                "client-a",
                &json!({
                    "action":"press_element","label":"Pay now","confirmation_id":id
                })
            )
            .is_err());
        assert!(state
            .prepare_or_request_sensitive_action(
                "client-a",
                &json!({
                    "action":"press_element","label":"Delete all","confirmation_id":id
                })
            )
            .is_err());
        assert!(state.decide_sensitive_action("bad-id", true).is_err());
        state.decide_sensitive_action(&id, true).unwrap();
        let (_, prepared) = state
            .prepare_or_request_sensitive_action(
                "client-a",
                &json!({
                    "action":"press_element","label":"Delete all","confirmation_id":id
                }),
            )
            .unwrap();
        assert_eq!(prepared["high_risk_confirmed"], true);
        assert!(prepared.get("confirmation_id").is_none());
        assert!(state.sensitive_action.is_none());
        assert_eq!(state.grant.as_ref().unwrap().actions_used, 1);
        assert!(state
            .prepare_or_request_sensitive_action(
                "client-a",
                &json!({
                    "action":"press_element","label":"Delete all","confirmation_id":id
                })
            )
            .is_err());
    }

    #[test]
    fn rejected_expired_or_paused_sensitive_action_never_executes() {
        let action = json!({"action":"key","key":"q","modifiers":["command"]});
        let mut state = control_session();
        state
            .prepare_or_request_sensitive_action("client-a", &action)
            .unwrap_err();
        let id = state.sensitive_action.as_ref().unwrap().id.clone();
        state.decide_sensitive_action(&id, false).unwrap();
        assert!(state.sensitive_action.is_none());
        assert!(state
            .prepare_or_request_sensitive_action(
                "client-a",
                &json!({
                    "action":"key","key":"q","modifiers":["command"],"confirmation_id":id
                })
            )
            .is_err());

        state
            .prepare_or_request_sensitive_action("client-a", &action)
            .unwrap_err();
        let id = state.sensitive_action.as_ref().unwrap().id.clone();
        state.sensitive_action.as_mut().unwrap().created_at = Instant::now() - SENSITIVE_ACTION_TTL;
        assert!(state.decide_sensitive_action(&id, true).is_err());
        state
            .prepare_or_request_sensitive_action("client-a", &action)
            .unwrap_err();
        let id = state.sensitive_action.as_ref().unwrap().id.clone();
        state.grant.as_mut().unwrap().paused = true;
        assert!(state.decide_sensitive_action(&id, true).is_err());
        assert!(state.sensitive_action.is_none());
    }

    #[test]
    fn remote_cannot_fake_native_high_risk_approval_flag() {
        let mut state = control_session();
        let (_, prepared) = state
            .prepare_or_request_sensitive_action(
                "client-a",
                &json!({
                    "action":"focus_app","high_risk_confirmed":true,"bundle_id":"attacker"
                }),
            )
            .unwrap();
        assert!(prepared.get("high_risk_confirmed").is_none());
        assert!(prepared.get("bundle_id").is_none());
        let forged = json!({
            "action":"press_element", "label":"Pay now", "high_risk_confirmed":true
        });
        assert!(state
            .prepare_or_request_sensitive_action("client-a", &forged)
            .is_err());
        assert_eq!(state.grant.as_ref().unwrap().actions_used, 1);
    }
}
