use std::collections::HashMap;
use std::sync::Mutex;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::constant_time_eq;
use crate::util::hash::sha256_hex;
use crate::util::time::unix_now_ms;

const BLOCK_SIZE: usize = 64;
pub(super) const REPLAY_TTL_MS: u64 = 120_000;
const REQUEST_MAX_SKEW_MS: u64 = 60_000;
pub(super) const MAX_REPLAY_ENTRIES: usize = 4096;

/// RFC 2104 standard HMAC-SHA256 using only `sha2::Sha256`.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let hashed = Sha256::digest(key);
        key_block[..32].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK_SIZE];
    let mut opad = [0x5cu8; BLOCK_SIZE];
    for ((ip, op), &kb) in ipad.iter_mut().zip(opad.iter_mut()).zip(key_block.iter()) {
        *ip ^= kb;
        *op ^= kb;
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    let final_digest = outer.finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(&final_digest);
    out
}

pub fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    let mac = hmac_sha256(key, message);
    crate::util::hash::encode_hex_lower(&mac)
}

/// Generate 32 bytes (64 hex chars) of OS CSPRNG entropy via v4 UUIDs + domain separation.
pub fn random_hex_32() -> String {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();
    let mut hasher = Sha256::new();
    hasher.update(b"ctm-csprng-v1:");
    hasher.update(a.as_bytes());
    hasher.update(b.as_bytes());
    hasher.update(c.as_bytes());
    hasher.update(unix_now_ms().to_le_bytes());
    crate::util::hash::encode_hex_lower(&hasher.finalize())
}

/// Generate a uniform 6-digit numeric pairing PIN (`100000`..=`999999`) with rejection sampling.
pub fn random_pin_6() -> String {
    loop {
        // Mask out UUID v4 version/variant bits by hashing two v4 UUIDs without heap allocation.
        let mut hasher = Sha256::new();
        hasher.update(Uuid::new_v4().as_bytes());
        hasher.update(Uuid::new_v4().as_bytes());
        let digest = hasher.finalize();
        let raw = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
        // Reject above largest multiple of 900_000 fitting in u32 (4_294_800_000) to avoid bias.
        if raw < 4_294_800_000 {
            let code = 100_000 + (raw % 900_000);
            return format!("{code:06}");
        }
    }
}

pub fn derive_fingerprint(device_id: &str, node_secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ctm-device-fp-v1:");
    hasher.update(device_id.as_bytes());
    hasher.update(b":");
    hasher.update(node_secret.as_bytes());
    let digest = hasher.finalize();
    let hex = crate::util::hash::encode_hex_lower(&digest[..8]).to_ascii_uppercase();
    format!(
        "{}-{}-{}-{}",
        &hex[0..4],
        &hex[4..8],
        &hex[8..12],
        &hex[12..16]
    )
}

pub fn derive_peer_shared_secret(
    initiator_id: &str,
    responder_id: &str,
    secret_part_a: &str,
    secret_part_b: &str,
) -> String {
    hmac_sha256_hex(
        format!("{secret_part_a}:{secret_part_b}").as_bytes(),
        format!("ctm-peer-secret-v1|{initiator_id}|{responder_id}").as_bytes(),
    )
}

pub fn compute_pair_request_proof(
    pin: &str,
    initiator_id: &str,
    target_id: &str,
    timestamp_ms: u64,
    nonce: &str,
    secret_part_a: &str,
) -> String {
    let msg = format!(
        "ctm-pair-req-v1|{initiator_id}|{target_id}|{timestamp_ms}|{nonce}|{secret_part_a}"
    );
    hmac_sha256_hex(pin.trim().as_bytes(), msg.as_bytes())
}

pub fn compute_pair_ack_proof(
    pin: &str,
    responder_id: &str,
    initiator_id: &str,
    nonce: &str,
    secret_part_b: &str,
) -> String {
    let msg = format!("ctm-pair-ack-v1|{responder_id}|{initiator_id}|{nonce}|{secret_part_b}");
    hmac_sha256_hex(pin.trim().as_bytes(), msg.as_bytes())
}

pub fn sign_peer_request(
    peer_secret: &str,
    caller_device_id: &str,
    method: &str,
    path_and_query: &str,
    timestamp_ms: u64,
    nonce: &str,
    body: &[u8],
) -> String {
    let body_hash = sha256_hex(body);
    let canonical = format!(
        "ctm-lan-rpc-v1\n{caller_device_id}\n{}\n{path_and_query}\n{timestamp_ms}\n{nonce}\n{body_hash}",
        method.to_ascii_uppercase()
    );
    hmac_sha256_hex(peer_secret.as_bytes(), canonical.as_bytes())
}

pub struct PeerSignatureCheck<'a> {
    pub peer_secret: &'a str,
    pub caller_device_id: &'a str,
    pub method: &'a str,
    pub path_and_query: &'a str,
    pub timestamp_ms: u64,
    pub nonce: &'a str,
    pub body: &'a [u8],
    pub provided_signature: &'a str,
    pub now_ms: u64,
}

pub fn verify_peer_request_signature(check: &PeerSignatureCheck<'_>) -> Result<(), &'static str> {
    if check.now_ms.abs_diff(check.timestamp_ms) > REQUEST_MAX_SKEW_MS {
        return Err("TIMESTAMP_SKEW_EXCEEDED");
    }
    if check.nonce.len() < 8 || check.nonce.len() > 128 || check.nonce.chars().any(char::is_control)
    {
        return Err("INVALID_NONCE");
    }
    let expected = sign_peer_request(
        check.peer_secret,
        check.caller_device_id,
        check.method,
        check.path_and_query,
        check.timestamp_ms,
        check.nonce,
        check.body,
    );
    if !constant_time_eq(
        expected.as_bytes(),
        check
            .provided_signature
            .trim()
            .to_ascii_lowercase()
            .as_bytes(),
    ) {
        return Err("INVALID_SIGNATURE");
    }
    Ok(())
}

/// Bounded anti-replay cache keyed by `"{peer_id}:{nonce}"`.
#[derive(Default)]
pub struct ReplayGuard {
    seen: Mutex<HashMap<String, u64>>,
}

impl ReplayGuard {
    pub fn check_and_record(&self, peer_id: &str, nonce: &str, now_ms: u64) -> bool {
        self.check_and_record_with_capacity(peer_id, nonce, now_ms, MAX_REPLAY_ENTRIES)
    }

    pub(super) fn check_and_record_with_capacity(
        &self,
        peer_id: &str,
        nonce: &str,
        now_ms: u64,
        max_entries: usize,
    ) -> bool {
        let mut guard = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        guard.retain(|_, ts| now_ms.saturating_sub(*ts) <= REPLAY_TTL_MS);
        let key = format!("{peer_id}:{nonce}");
        if guard.contains_key(&key) {
            return false;
        }
        // Fail closed once capacity is saturated by unexpired entries: never
        // clear the table, which would open a replay window for all prior nonces.
        if guard.len() >= max_entries {
            return false;
        }
        guard.insert(key, now_ms);
        true
    }

    pub fn prune_expired(&self, now_ms: u64) {
        let mut guard = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        guard.retain(|_, ts| now_ms.saturating_sub(*ts) <= REPLAY_TTL_MS);
        if guard.capacity() > 64 && guard.len() * 4 < guard.capacity() {
            guard.shrink_to_fit();
        }
    }
}
