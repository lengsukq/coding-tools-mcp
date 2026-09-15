use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::tools::registry::{exposed_tool_names, input_schema, tool_definition};

pub const TOOL_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
struct RuntimeIdentity {
    id: String,
    started_at: String,
}

static RUNTIME_IDENTITY: OnceLock<RuntimeIdentity> = OnceLock::new();

pub fn fingerprint(tool_profile: &str, known_schema_hash: Option<&str>) -> Value {
    let identity = RUNTIME_IDENTITY.get_or_init(|| RuntimeIdentity {
        id: Uuid::new_v4().to_string(),
        started_at: unix_timestamp(),
    });
    let schema_hash = schema_hash(tool_profile);
    let schema_changed = known_schema_hash.is_some_and(|known| known != schema_hash);
    json!({
        "runtime_id": identity.id,
        "started_at": identity.started_at,
        "app_version": env!("CARGO_PKG_VERSION"),
        "build": {
            "git_sha": build_git_sha(),
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "identity_source": if build_git_sha() == "unknown" { "unavailable" } else { "compile_env" }
        },
        "tool_schema_version": TOOL_SCHEMA_VERSION,
        "tool_schema_hash": schema_hash,
        "known_schema_hash": known_schema_hash,
        "schema_changed": schema_changed,
        "reconnect_recommended": schema_changed
    })
}

pub fn schema_hash(tool_profile: &str) -> String {
    let tools = exposed_tool_names(tool_profile)
        .into_iter()
        .filter_map(|name| {
            tool_definition(name).map(|definition| {
                json!({
                    "name": definition.name,
                    "title": definition.title,
                    "description": definition.description,
                    "read_only": definition.read_only,
                    "destructive": definition.destructive,
                    "open_world": definition.open_world,
                    "input_schema": input_schema(definition.name)
                })
            })
        })
        .collect::<Vec<_>>();
    let canonical = serde_json::to_vec(&json!({
        "version": TOOL_SCHEMA_VERSION,
        "profile": tool_profile,
        "tools": tools
    }))
    .expect("tool schema fingerprint serialization");
    format!("sha256:{:x}", Sha256::digest(canonical))
}

fn build_git_sha() -> &'static str {
    option_env!("CODING_TOOLS_GIT_SHA")
        .or(option_env!("GITHUB_SHA"))
        .unwrap_or("unknown")
}

fn unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_hash_is_stable_for_same_profile() {
        assert_eq!(schema_hash("compact"), schema_hash("compact"));
    }

    #[test]
    fn schema_hash_changes_between_different_tool_profiles() {
        assert_ne!(schema_hash("compact"), schema_hash("full"));
    }

    #[test]
    fn known_schema_hash_drives_reconnect_hint() {
        let current = schema_hash("compact");
        let fresh = fingerprint("compact", Some(&current));
        assert_eq!(fresh["schema_changed"], false);
        assert_eq!(fresh["reconnect_recommended"], false);
        let stale = fingerprint("compact", Some("sha256:old"));
        assert_eq!(stale["schema_changed"], true);
        assert_eq!(stale["reconnect_recommended"], true);
    }
}
