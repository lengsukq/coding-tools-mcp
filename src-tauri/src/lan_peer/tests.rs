use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::sync::Arc;

use serde_json::json;

use crate::mcp::gateway::GatewayState;
use crate::mcp::server::handle_gateway_request;
use crate::settings::{AppSettings, LanFederationConfig, PairedPeerConfig};
use crate::workspace::WorkspaceProfile;

use super::crypto::{
    compute_pair_ack_proof, compute_pair_request_proof, constant_time_eq,
    derive_peer_shared_secret, hmac_sha256_hex, random_pin_6, sign_peer_request, unix_time_ms,
    verify_peer_request_signature, ReplayGuard,
};
use super::identity::{
    parse_workspace_scope, WorkspaceTarget, LAN_NODE_SECRET_KEY, LAN_PEER_SECRETS_SCOPE,
    LAN_SECRET_SCOPE,
};
use super::pairing::{
    cancel_pairing_pin, generate_pairing_pin, PairCompleteRequest, PairCompleteResponse,
    PairPeerSummary,
};
use super::{
    lan_router, HOP_HEADER, LAN_PROTOCOL_VERSION, NONCE_HEADER, PEER_ID_HEADER, SIGNATURE_HEADER,
    TIMESTAMP_HEADER,
};

#[test]
fn hmac_sha256_matches_rfc4231_test_vectors() {
    // RFC 4231 Test Case 1
    let key1 = [0x0bu8; 20];
    let data1 = b"Hi There";
    assert_eq!(
        hmac_sha256_hex(&key1, data1),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );

    // RFC 4231 Test Case 2
    let key2 = b"Jefe";
    let data2 = b"what do ya want for nothing?";
    assert_eq!(
        hmac_sha256_hex(key2, data2),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
}

#[test]
fn six_digit_pin_and_replay_guard_enforce_bounds() {
    for _ in 0..32 {
        let pin = random_pin_6();
        assert_eq!(pin.len(), 6);
        assert!(pin.chars().all(|c| c.is_ascii_digit()));
        let numeric: u32 = pin.parse().expect("numeric pin");
        assert!((100_000..=999_999).contains(&numeric));
    }

    let guard = ReplayGuard::default();
    let now = unix_time_ms();
    assert!(guard.check_and_record("dev_alpha1", "nonce-12345678", now));
    assert!(!guard.check_and_record("dev_alpha1", "nonce-12345678", now + 50));
    assert!(guard.check_and_record("dev_beta02", "nonce-12345678", now + 50));
}

#[test]
fn replay_guard_at_capacity_keeps_rejecting_unexpired_replays() {
    let guard = ReplayGuard::default();
    let now = unix_time_ms();
    let cap = 8usize;

    for i in 0..cap {
        assert!(guard.check_and_record_with_capacity(
            "dev_alpha1",
            &format!("nonce-{i:08}"),
            now + i as u64,
            cap
        ));
    }
    // Capacity is saturated with unexpired nonces: fail closed rather than clearing the guard.
    assert!(!guard.check_and_record_with_capacity("dev_alpha1", "nonce-overflow", now + 100, cap));
    // Earlier unexpired nonces MUST still be rejected on replay!
    assert!(!guard.check_and_record_with_capacity("dev_alpha1", "nonce-00000000", now + 100, cap));
    assert!(!guard.check_and_record_with_capacity("dev_alpha1", "nonce-00000007", now + 100, cap));

    // Once past TTL, expired entries are pruned and fresh nonces are accepted again.
    let after_ttl = now + super::crypto::REPLAY_TTL_MS + 100;
    assert!(guard.check_and_record_with_capacity("dev_alpha1", "nonce-after-ttl", after_ttl, cap));
}

#[test]
fn valid_local_identity_fast_path_avoids_rewrite_until_fields_are_invalid() {
    let mut data = crate::data::AppData::default();
    assert!(super::identity::extract_valid_local_identity(&data).is_none());

    data.app_secrets.insert(
        LAN_SECRET_SCOPE.to_string(),
        HashMap::from([(
            LAN_NODE_SECRET_KEY.to_string(),
            "0123456789abcdef0123456789abcdef".to_string(),
        )]),
    );
    data.lan_federation.device_id = "dev_1234567890ab".into();
    data.lan_federation.device_name = "MacBook-Test".into();
    data.lan_federation.share_mode = "all".into();
    data.lan_federation.trust_level = "full".into();

    let extracted = super::identity::extract_valid_local_identity(&data);
    assert!(extracted.is_some());

    // Invalid share_mode forces fallback to update path.
    data.lan_federation.share_mode = "invalid".into();
    assert!(super::identity::extract_valid_local_identity(&data).is_none());
}

#[test]
fn workspace_scope_parser_distinguishes_local_and_remote_namespaces() {
    assert_eq!(
        parse_workspace_scope("ws_alpha", "dev_self0001").unwrap(),
        WorkspaceTarget::Local("ws_alpha".into())
    );
    assert_eq!(
        parse_workspace_scope("local:ws_alpha", "dev_self0001").unwrap(),
        WorkspaceTarget::Local("ws_alpha".into())
    );
    assert_eq!(
        parse_workspace_scope("dev_self0001:ws_alpha", "dev_self0001").unwrap(),
        WorkspaceTarget::Local("ws_alpha".into())
    );
    assert_eq!(
        parse_workspace_scope("dev_macmini02:ws_alpha", "dev_self0001").unwrap(),
        WorkspaceTarget::Remote {
            device_id: "dev_macmini02".into(),
            workspace_id: "ws_alpha".into(),
        }
    );

    assert!(parse_workspace_scope("", "dev_self0001").is_err());
    assert!(parse_workspace_scope("dev_macmini02:../escape", "dev_self0001").is_err());
    assert!(parse_workspace_scope("dev_macmini02:a:b", "dev_self0001").is_err());
    assert!(parse_workspace_scope("bad!:ws_alpha", "dev_self0001").is_err());
}

#[test]
fn peer_signature_rejects_tampering_and_clock_skew() {
    let secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let now = unix_time_ms();
    let body = br#"{"kind":"select_probe"}"#;
    let sig = sign_peer_request(
        secret,
        "dev_caller01",
        "POST",
        "/api/lan/v1/rpc",
        now,
        "nonce_abcdef12",
        body,
    );
    assert!(verify_peer_request_signature(
        secret,
        "dev_caller01",
        "POST",
        "/api/lan/v1/rpc",
        now,
        "nonce_abcdef12",
        body,
        &sig,
        now + 1000,
    )
    .is_ok());

    assert_eq!(
        verify_peer_request_signature(
            secret,
            "dev_caller01",
            "POST",
            "/api/lan/v1/rpc",
            now,
            "nonce_abcdef12",
            br#"{"kind":"tampered"}"#,
            &sig,
            now,
        ),
        Err("INVALID_SIGNATURE")
    );

    assert_eq!(
        verify_peer_request_signature(
            secret,
            "dev_caller01",
            "POST",
            "/api/lan/v1/rpc",
            now - 90_000,
            "nonce_abcdef12",
            body,
            &sig,
            now,
        ),
        Err("TIMESTAMP_SKEW_EXCEEDED")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_peers_pair_and_federate_workspaces_while_blocking_tunnel_and_loops() {
    super::invalidate_peer_cache("");
    let dir_a = tempfile::tempdir().expect("dir a");
    let dir_b = tempfile::tempdir().expect("dir b");
    fs::write(dir_a.path().join("readme.txt"), "FROM-MACBOOK-A").unwrap();
    fs::write(dir_b.path().join("readme.txt"), "FROM-WINDOWS-B").unwrap();

    // Intentionally give BOTH machines the exact same workspace name AND raw workspace ID
    // ("same-ws-id") to prove composite namespace isolation prevents collisions!
    let mut ws_a = WorkspaceProfile::new(
        dir_a.path().display().to_string(),
        Some("coding-tools-mcp".into()),
    );
    ws_a.id = "same-ws-id".into();
    let mut ws_b = WorkspaceProfile::new(
        dir_b.path().display().to_string(),
        Some("coding-tools-mcp".into()),
    );
    ws_b.id = "same-ws-id".into();

    let listener_b = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port_b = listener_b.local_addr().unwrap().port();

    let mut secrets_b = HashMap::new();
    secrets_b.insert(
        LAN_SECRET_SCOPE.to_string(),
        HashMap::from([(
            LAN_NODE_SECRET_KEY.to_string(),
            "secret-node-b-00000000000000000000".to_string(),
        )]),
    );
    let mut settings_b = AppSettings {
        allow_lan_access: true,
        global_permission_mode: "dangerous".into(),
        lan_federation: LanFederationConfig {
            enabled: true,
            device_id: "dev_windows_b".into(),
            device_name: "Windows Workstation B".into(),
            share_mode: "all".into(),
            trust_level: "full".into(),
            allow_computer_use: false,
            extra_subnets: vec!["10.144.144.0/24".into()],
            paired_peers: Vec::new(),
            shared_workspace_ids: Vec::new(),
        },
        app_secrets: secrets_b,
        ..AppSettings::default()
    };
    settings_b.global_gateway.local_port = port_b;

    let gateway_b = Arc::new(GatewayState::for_test_profiles(vec![ws_b], settings_b));
    let app_b = lan_router(gateway_b.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let server_b = tokio::spawn(async move {
        let _ = axum::serve(listener_b, app_b).await;
    });

    let client = reqwest::Client::builder().no_proxy().build().unwrap();

    // 1. Verify public tunnel / reverse proxy headers (Cloudflare / FRP / Via / True-Client-IP) are hard-blocked with 403 Forbidden.
    for (header_name, header_val) in [
        ("cf-connecting-ip", "203.0.113.9"),
        ("via", "1.1 proxy.example"),
        ("true-client-ip", "198.51.100.7"),
    ] {
        let tunnel_attempt = client
            .get(format!("http://127.0.0.1:{port_b}/api/lan/v1/hello"))
            .header(header_name, header_val)
            .send()
            .await
            .unwrap();
        assert_eq!(
            tunnel_attempt.status().as_u16(),
            403,
            "header {header_name} must be rejected"
        );
    }

    // 2. Mutual 6-digit PIN pairing handshake (wrong PIN rejected, valid PIN pairs both sides).
    let pin_dto = generate_pairing_pin();
    let now = unix_time_ms();
    let nonce = "pairnonce123456";
    let part_a = "aaaa1111bbbb2222cccc3333dddd4444eeee5555ffff6666aaaa1111bbbb2222";

    let bad_req = PairCompleteRequest {
        protocol: LAN_PROTOCOL_VERSION.into(),
        target_device_id: "dev_windows_b".into(),
        initiator: PairPeerSummary {
            device_id: "dev_macbook_a".into(),
            device_name: "MacBook Pro A".into(),
            fingerprint: "1111-2222-3333-4444".into(),
            os: "macos".into(),
            arch: "aarch64".into(),
            version: "0.3.8".into(),
            mcp_port: 28765,
            addresses: vec!["127.0.0.1".into()],
        },
        timestamp_ms: now,
        nonce: nonce.into(),
        secret_part_a: part_a.into(),
        pin_proof: compute_pair_request_proof(
            "000000",
            "dev_macbook_a",
            "dev_windows_b",
            now,
            nonce,
            part_a,
        ),
    };
    let bad_resp = client
        .post(format!(
            "http://127.0.0.1:{port_b}/api/lan/v1/pair/complete"
        ))
        .json(&bad_req)
        .send()
        .await
        .unwrap();
    assert_eq!(bad_resp.status().as_u16(), 403);

    let good_req = PairCompleteRequest {
        pin_proof: compute_pair_request_proof(
            &pin_dto.pin,
            "dev_macbook_a",
            "dev_windows_b",
            now,
            nonce,
            part_a,
        ),
        ..bad_req
    };
    let good_resp = client
        .post(format!(
            "http://127.0.0.1:{port_b}/api/lan/v1/pair/complete"
        ))
        .json(&good_req)
        .send()
        .await
        .unwrap();
    assert_eq!(good_resp.status().as_u16(), 200);
    let ack: PairCompleteResponse = good_resp.json().await.unwrap();
    let expected_ack = compute_pair_ack_proof(
        &pin_dto.pin,
        "dev_windows_b",
        "dev_macbook_a",
        nonce,
        &ack.secret_part_b,
    );
    assert!(constant_time_eq(
        expected_ack.as_bytes(),
        ack.response_proof.as_bytes()
    ));
    cancel_pairing_pin();

    let shared_secret =
        derive_peer_shared_secret("dev_macbook_a", "dev_windows_b", part_a, &ack.secret_part_b);

    // 3. Verify Hop > 1 is rejected with 400 FEDERATION_LOOP_DENIED.
    let loop_ts = unix_time_ms();
    let loop_nonce = "loopnonce123456";
    let loop_sig = sign_peer_request(
        &shared_secret,
        "dev_macbook_a",
        "GET",
        "/api/lan/v1/catalog",
        loop_ts,
        loop_nonce,
        &[],
    );
    let loop_resp = client
        .get(format!("http://127.0.0.1:{port_b}/api/lan/v1/catalog"))
        .header(PEER_ID_HEADER, "dev_macbook_a")
        .header(TIMESTAMP_HEADER, loop_ts.to_string())
        .header(NONCE_HEADER, loop_nonce)
        .header(HOP_HEADER, "2")
        .header(SIGNATURE_HEADER, loop_sig)
        .send()
        .await
        .unwrap();
    assert_eq!(loop_resp.status().as_u16(), 400);

    // 4. Configure Node A with paired Node B and test full single-entrypoint MCP federation!
    let mut secrets_a = HashMap::new();
    secrets_a.insert(
        LAN_SECRET_SCOPE.to_string(),
        HashMap::from([(
            LAN_NODE_SECRET_KEY.to_string(),
            "secret-node-a-00000000000000000000".to_string(),
        )]),
    );
    secrets_a.insert(
        LAN_PEER_SECRETS_SCOPE.to_string(),
        HashMap::from([("dev_windows_b".to_string(), shared_secret)]),
    );
    let mut settings_a = AppSettings {
        allow_lan_access: true,
        global_permission_mode: "dangerous".into(),
        lan_federation: LanFederationConfig {
            enabled: true,
            device_id: "dev_macbook_a".into(),
            device_name: "MacBook Pro A".into(),
            share_mode: "all".into(),
            trust_level: "full".into(),
            allow_computer_use: true,
            extra_subnets: vec!["10.144.144.0/24".into()],
            paired_peers: vec![PairedPeerConfig {
                device_id: "dev_windows_b".into(),
                device_name: "Windows Workstation B".into(),
                fingerprint: ack.responder.fingerprint,
                os: "windows".into(),
                arch: "x86_64".into(),
                version: "0.3.8".into(),
                host: "127.0.0.1".into(),
                port: port_b,
                addresses: vec!["127.0.0.1".into()],
                enabled: true,
                paired_at_ms: now,
            }],
            shared_workspace_ids: Vec::new(),
        },
        app_secrets: secrets_a,
        ..AppSettings::default()
    };
    settings_a.global_gateway.public_url = "https://entry-a.example.test".into();

    let gateway_a = GatewayState::for_test_profiles(vec![ws_a], settings_a);
    gateway_a.sessions.register_for_test("sess-1");

    // 4a. workspace_list aggregates both machines with distinct IDs despite identical raw workspace IDs!
    let list_resp = tokio::task::spawn_blocking({
        let gw = gateway_a.clone_for_test();
        move || {
            handle_gateway_request(
                &gw,
                Some("sess-1"),
                &json!({
                    "jsonrpc": "2.0",
                    "id": 10,
                    "method": "tools/call",
                    "params": { "name": "workspace_list", "arguments": {} }
                }),
            )
            .body
        }
    })
    .await
    .unwrap();
    let workspaces = list_resp["result"]["structuredContent"]["workspaces"]
        .as_array()
        .expect("workspaces array");
    assert_eq!(workspaces.len(), 2);
    assert!(workspaces
        .iter()
        .any(|w| w["id"] == "same-ws-id" && w["device"]["isLocal"] == true));
    assert!(workspaces
        .iter()
        .any(|w| w["id"] == "dev_windows_b:same-ws-id" && w["device"]["isLocal"] == false));

    // 4b. Unscoped tool call MUST NOT auto-select local workspace when paired peer workspaces exist!
    let unscoped = tokio::task::spawn_blocking({
        let gw = gateway_a.clone_for_test();
        move || {
            handle_gateway_request(
                &gw,
                Some("sess-1"),
                &json!({
                    "jsonrpc": "2.0",
                    "id": 11,
                    "method": "tools/call",
                    "params": { "name": "read_file", "arguments": { "path": "readme.txt" } }
                }),
            )
            .body
        }
    })
    .await
    .unwrap();
    assert_eq!(
        unscoped["error"]["data"]["reason"],
        "WORKSPACE_NOT_SELECTED"
    );

    // 4c. Selecting remote workspace routes subsequent session calls to Node B while keeping Node A untouched!
    let select_remote = tokio::task::spawn_blocking({
        let gw = gateway_a.clone_for_test();
        move || {
            handle_gateway_request(
                &gw,
                Some("sess-1"),
                &json!({
                    "jsonrpc": "2.0",
                    "id": 12,
                    "method": "tools/call",
                    "params": {
                        "name": "workspace_select",
                        "arguments": { "workspace_id": "dev_windows_b:same-ws-id" }
                    }
                }),
            )
            .body
        }
    })
    .await
    .unwrap();
    assert_eq!(
        select_remote["result"]["structuredContent"]["workspace"]["id"],
        "dev_windows_b:same-ws-id"
    );

    let read_remote = tokio::task::spawn_blocking({
        let gw = gateway_a.clone_for_test();
        move || {
            handle_gateway_request(
                &gw,
                Some("sess-1"),
                &json!({
                    "jsonrpc": "2.0",
                    "id": 13,
                    "method": "tools/call",
                    "params": { "name": "read_file", "arguments": { "path": "readme.txt" } }
                }),
            )
            .body
        }
    })
    .await
    .unwrap();
    assert_eq!(
        read_remote["result"]["structuredContent"]["content"],
        "FROM-WINDOWS-B"
    );
    assert_eq!(
        read_remote["result"]["structuredContent"]["gateway_device"]["deviceId"],
        "dev_windows_b"
    );

    // Explicit local workspace_id still reads Node A without changing session's remote selection!
    let read_local = tokio::task::spawn_blocking({
        let gw = gateway_a.clone_for_test();
        move || {
            handle_gateway_request(
                &gw,
                Some("sess-1"),
                &json!({
                    "jsonrpc": "2.0",
                    "id": 14,
                    "method": "tools/call",
                    "params": {
                        "name": "read_file",
                        "arguments": { "workspace_id": "same-ws-id", "path": "readme.txt" }
                    }
                }),
            )
            .body
        }
    })
    .await
    .unwrap();
    assert_eq!(
        read_local["result"]["structuredContent"]["content"],
        "FROM-MACBOOK-A"
    );

    server_b.abort();
}
