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
    assert!(validate_action(&json!({"action":"key","key":"a","modifiers":["not-a-key"]})).is_err());
    assert!(validate_action(&json!({"action":"press_element","label":""})).is_err());
    assert!(validate_action(&json!({"action":"drag","x":2,"y":2,"end_x":2,"end_y":12})).is_ok());
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
