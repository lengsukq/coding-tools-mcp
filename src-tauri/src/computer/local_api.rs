use std::time::Instant;

use serde_json::{json, Value};

use crate::data::DataStore;

use super::{
    accessibility_permission, action_lock, locked, native_apps, now_ms,
    screen_permission_diagnostics, screen_recording_permission, Controller, ManualComputerAccess,
    ALLOWED_MANUAL_DURATIONS_MINUTES, MAX_CONTROL_ACTIONS, REQUEST_TTL, SENSITIVE_ACTION_TTL,
};

#[cfg(target_os = "macos")]
use super::native;

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGRequestScreenCaptureAccess() -> bool;
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
    if !ALLOWED_MANUAL_DURATIONS_MINUTES.contains(&duration_minutes) {
        return Err("INVALID_DURATION: 可选择 5/15/60 分钟、8/24 小时或永久".into());
    }
    let app_bundle =
        validate_control_target(scope, target, "启用控制前，请先授予 macOS 辅助功能权限")?;
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

fn validate_control_target(
    scope: &str,
    target: Option<&str>,
    missing_ax_message: &str,
) -> Result<Option<String>, String> {
    if scope != "control" {
        return Ok(None);
    }
    if accessibility_permission() != Some(true) {
        return Err(missing_ax_message.into());
    }
    let target = target
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("控制授权需要在本机明确选择目标应用")?;
    let apps = native_apps()?;
    let exists = apps["applications"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["bundle_id"].as_str() == Some(target))
    });
    if !exists {
        return Err("目标应用不在当前可选择的列表中".into());
    }
    Ok(Some(target.to_owned()))
}

/// Only the local desktop UI can approve a specific action. This does not
/// execute anything: the originating MCP transport must retry the exact action.
pub fn local_decide_sensitive_action(id: &str, approve: bool) -> Result<Value, String> {
    let _serialized = action_lock();
    locked().decide_sensitive_action(id, approve)?;
    Ok(local_status())
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

pub fn local_request_accessibility_permission() -> Value {
    #[cfg(target_os = "macos")]
    native::request_accessibility();
    local_status()
}

struct StatusSnapshot {
    pending_request_id: Option<String>,
    pending_expires_in_seconds: Option<u64>,
    pending_scope: Option<String>,
    active: bool,
    manual_enabled: bool,
    manual_scope: Option<String>,
    manual_target_app: Option<String>,
    manual_expires_at_ms: Option<u64>,
    manual_paused: bool,
    active_scope: Option<String>,
    paused: bool,
    pending_sensitive_action_id: Option<String>,
    pending_sensitive_action_description: Option<String>,
    pending_sensitive_action_app: Option<String>,
    pending_sensitive_action_approved: bool,
    pending_sensitive_action_expires_in_seconds: Option<u64>,
    remaining_control_actions: Option<u16>,
    target_app_bundle: Option<String>,
    expires_in_seconds: Option<u64>,
    screen_capture_succeeded_in_session: bool,
}

fn snapshot_controller_status() -> StatusSnapshot {
    let mut state = locked();
    state.expire();
    let manual = state.manual.as_ref();
    let manual_enabled = manual.is_some_and(ManualComputerAccess::active);
    let manual_paused = manual.is_some_and(|p| p.paused);
    let now_instant = Instant::now();

    StatusSnapshot {
        pending_request_id: state.pending.as_ref().map(|p| p.id.clone()),
        pending_expires_in_seconds: state
            .pending
            .as_ref()
            .map(|p| REQUEST_TTL.saturating_sub(p.at.elapsed()).as_secs()),
        pending_scope: state.pending.as_ref().map(|p| p.scope.clone()),
        active: manual_enabled || state.grant.is_some(),
        manual_enabled,
        manual_scope: manual.map(|p| p.scope.clone()),
        manual_target_app: manual.and_then(|p| p.app_bundle.clone()),
        manual_expires_at_ms: manual.and_then(|p| p.expires_at_ms),
        manual_paused,
        active_scope: manual
            .map(|p| p.scope.clone())
            .or_else(|| state.grant.as_ref().map(|p| p.scope.clone())),
        paused: manual_paused || state.grant.as_ref().is_some_and(|g| g.paused),
        pending_sensitive_action_id: state.sensitive_action.as_ref().map(|p| p.id.clone()),
        pending_sensitive_action_description: state
            .sensitive_action
            .as_ref()
            .map(|p| p.description.clone()),
        pending_sensitive_action_app: state
            .sensitive_action
            .as_ref()
            .map(|p| p.target_app.clone()),
        pending_sensitive_action_approved: state
            .sensitive_action
            .as_ref()
            .is_some_and(|p| p.approved),
        pending_sensitive_action_expires_in_seconds: state.sensitive_action.as_ref().map(|p| {
            SENSITIVE_ACTION_TTL
                .saturating_sub(p.created_at.elapsed())
                .as_secs()
        }),
        remaining_control_actions: state
            .grant
            .as_ref()
            .filter(|p| p.scope == "control")
            .map(|p| MAX_CONTROL_ACTIONS.saturating_sub(p.actions_used)),
        target_app_bundle: manual
            .and_then(|p| p.app_bundle.clone())
            .or_else(|| state.grant.as_ref().and_then(|p| p.app_bundle.clone())),
        expires_in_seconds: if manual_enabled {
            manual
                .and_then(|p| p.expires_at_ms)
                .map(|ms| ms.saturating_sub(now_ms()) / 1000)
        } else {
            state
                .grant
                .as_ref()
                .map(|g| g.until.saturating_duration_since(now_instant).as_secs())
        },
        screen_capture_succeeded_in_session: state
            .grant
            .as_ref()
            .is_some_and(|grant| grant.capture_succeeded),
    }
}

/// Local Tauri UI only; never expose local_approve/local_revoke as MCP tools.
pub fn local_status() -> Value {
    // Release the CONTROL mutex before invoking macOS TCC / Security FFI calls
    // so UI polling never stalls concurrent MCP capture/action threads.
    let snap = snapshot_controller_status();
    let diagnostics = screen_permission_diagnostics();
    // Use the same native preflight result for the main status and the
    // diagnostics; two separate checks in one UI poll can disagree.
    let screen_granted = diagnostics["preflight_granted"]
        .as_bool()
        .or_else(screen_recording_permission);
    let accessibility = accessibility_permission();

    json!({
        "pendingRequestId": snap.pending_request_id,
        "pendingExpiresInSeconds": snap.pending_expires_in_seconds,
        "active": snap.active,
        "manualEnabled": snap.manual_enabled,
        "manualScope": snap.manual_scope,
        "manualTargetApp": snap.manual_target_app,
        "manualExpiresAtMs": snap.manual_expires_at_ms,
        "manualPaused": snap.manual_paused,
        "pendingScope": snap.pending_scope,
        "activeScope": snap.active_scope,
        "paused": snap.paused,
        "pendingSensitiveActionId": snap.pending_sensitive_action_id,
        "pendingSensitiveActionDescription": snap.pending_sensitive_action_description,
        "pendingSensitiveActionApp": snap.pending_sensitive_action_app,
        "pendingSensitiveActionApproved": snap.pending_sensitive_action_approved,
        "pendingSensitiveActionExpiresInSeconds": snap.pending_sensitive_action_expires_in_seconds,
        "remainingControlActions": snap.remaining_control_actions,
        "targetAppBundle": snap.target_app_bundle,
        "expiresInSeconds": snap.expires_in_seconds,
        "supported": cfg!(target_os = "macos"),
        "screenRecordingGranted": screen_granted,
        "screenCaptureSucceededInSession": snap.screen_capture_succeeded_in_session,
        "screenPermissionDiagnostic": diagnostics,
        "accessibilityGranted": accessibility
    })
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
    let app_bundle = validate_control_target(&scope, target_bundle, "请先授予 macOS 辅助功能权限")?;
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
