use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use uuid::Uuid;

const BLOCK_SIZE: usize = 64;
const REPLAY_TTL_MS: u64 = 120_000;
const REQUEST_MAX_SKEW_MS: u64 = 60_000;
const MAX_REPLAY_ENTRIES: usize = 4096;

pub fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

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
    let mut out = String::with_capacity(64);
    for byte in mac {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Constant-time byte comparison to prevent timing side-channels on PIN proofs and HMACs.
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&a, &b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
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
    hasher.update(unix_time_ms().to_le_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generate a uniform 6-digit numeric pairing PIN (`100000`..=`999999`) with rejection sampling.
pub fn random_pin_6() -> String {
    loop {
        let bytes = *Uuid::new_v4().as_bytes();
        // Mask out UUID v4 version/variant bits by hashing with a second v4 UUID.
        let digest = Sha256::digest([bytes.as_slice(), Uuid::new_v4().as_bytes()].concat());
        let raw = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
        // Reject above largest multiple of 900_000 fitting in u32 (4_294_800_000) to avoid bias.
        if raw < 4_294_800_000 {
            let code = 100_000 + (raw % 900_000);
            return format!("{code:06}");
        }
    }
}

pub fn derive_fingerprint(device_id: &str, node_secret: &str) -> String {
    let hex = sha256_hex(format!("ctm-device-fp-v1:{device_id}:{node_secret}").as_bytes());
    let upper = hex[..16].to_ascii_uppercase();
    format!(
        "{}-{}-{}-{}",
        &upper[0..4],
        &upper[4..8],
        &upper[8..12],
        &upper[12..16]
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

pub fn verify_peer_request_signature(
    peer_secret: &str,
    caller_device_id: &str,
    method: &str,
    path_and_query: &str,
    timestamp_ms: u64,
    nonce: &str,
    body: &[u8],
    provided_signature: &str,
    now_ms: u64,
) -> Result<(), &'static str> {
    if now_ms.abs_diff(timestamp_ms) > REQUEST_MAX_SKEW_MS {
        return Err("TIMESTAMP_SKEW_EXCEEDED");
    }
    if nonce.len() < 8 || nonce.len() > 128 || nonce.chars().any(char::is_control) {
        return Err("INVALID_NONCE");
    }
    let expected = sign_peer_request(
        peer_secret,
        caller_device_id,
        method,
        path_and_query,
        timestamp_ms,
        nonce,
        body,
    );
    if !constant_time_eq(
        expected.as_bytes(),
        provided_signature.trim().to_ascii_lowercase().as_bytes(),
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
        let mut guard = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        guard.retain(|_, ts| now_ms.saturating_sub(*ts) <= REPLAY_TTL_MS);
        if guard.len() >= MAX_REPLAY_ENTRIES {
            guard.clear();
        }
        let key = format!("{peer_id}:{nonce}");
        if guard.contains_key(&key) {
            return false;
        }
        guard.insert(key, now_ms);
        true
    }
}
