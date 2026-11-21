use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::util::hash::sha256_hex_str;
use crate::util::time::unix_now_secs;

/// Host/transport selections expire after 30 minutes without an RPC.
/// In-flight requests are leased and never evicted by the maintenance sweep.
const SESSION_TTL_SECONDS: u64 = 30 * 60;
// Long enough to cover the two-minute local approval window and the five-minute grant.
const COMPUTER_CAPABILITY_TTL_SECONDS: u64 = 10 * 60;
const MAX_SESSIONS: usize = 512;
const MAX_COMPUTER_CAPABILITIES: usize = 256;

fn is_scope_expired(scope: &GatewaySessionScope, now: u64) -> bool {
    scope.in_flight_requests == 0
        && (scope.last_seen_at == 0 || now.saturating_sub(scope.last_seen_at) > SESSION_TTL_SECONDS)
}

fn is_capability_expired(cap: &ComputerCapability, now: u64) -> bool {
    now.saturating_sub(cap.created_at) > COMPUTER_CAPABILITY_TTL_SECONDS
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewaySessionScope {
    pub active_workspace_id: Option<String>,
    pub last_seen_at: u64,
    #[serde(skip)]
    pub in_flight_requests: u32,
    /// Host-provided metadata IDs are intentionally not valid Computer Use
    /// identities. Only sessions minted by initialize can access that surface.
    #[serde(skip)]
    pub issued_by_server: bool,
    /// Only the authenticated principal that initialized the MCP transport
    /// may reuse this session for host-level Computer Use access. Never accept
    /// client-provided request metadata as this identity.
    #[serde(skip)]
    pub computer_owner: Option<String>,
}

#[derive(Debug, Default)]
pub struct GatewaySessionStore {
    sessions: Mutex<HashMap<String, GatewaySessionScope>>,
    // Only hashes are stored. These bearer capabilities are never inferred from
    // client-provided `_meta` IDs and never grant access without local approval.
    computer_capabilities: Mutex<HashMap<String, ComputerCapability>>,
    maintenance: Mutex<SessionMaintenanceStats>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMaintenanceStats {
    pub last_sweep_at: u64,
    pub reclaimed_sessions: u64,
    pub reclaimed_capabilities: u64,
}

/// A transport request keeps its session alive until the response is sent,
/// even when a long-running tool exceeds the idle timeout.
pub struct ActiveSessionLease {
    store: Arc<GatewaySessionStore>,
    session_id: String,
}

impl Drop for ActiveSessionLease {
    fn drop(&mut self) {
        let mut sessions = self
            .store
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(scope) = sessions.get_mut(&self.session_id) {
            scope.in_flight_requests = scope.in_flight_requests.saturating_sub(1);
            scope.last_seen_at = unix_now_secs();
        }
    }
}

#[derive(Debug, Clone)]
struct ComputerCapability {
    session_id: String,
    owner: String,
    created_at: u64,
}

fn computer_capability_hash(token: &str) -> String {
    sha256_hex_str(token)
}

impl GatewaySessionStore {
    fn ensure_session_capacity(sessions: &mut HashMap<String, GatewaySessionScope>, now: u64) {
        if sessions.len() < MAX_SESSIONS {
            return;
        }
        Self::prune_expired(sessions, now);
        while sessions.len() >= MAX_SESSIONS {
            let Some(oldest_idle) = sessions
                .iter()
                .filter(|(_, scope)| scope.in_flight_requests == 0)
                .min_by_key(|(_, scope)| scope.last_seen_at)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            sessions.remove(&oldest_idle);
        }
    }

    fn get_live_mut<'a>(
        sessions: &'a mut HashMap<String, GatewaySessionScope>,
        session_id: &str,
        now: u64,
    ) -> Option<&'a mut GatewaySessionScope> {
        let expired = sessions
            .get(session_id)
            .is_some_and(|scope| is_scope_expired(scope, now));
        if expired {
            sessions.remove(session_id);
            return None;
        }
        sessions.get_mut(session_id)
    }

    pub fn begin_request(self: &Arc<Self>, session_id: &str) -> Option<ActiveSessionLease> {
        let now = unix_now_secs();
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let scope = Self::get_live_mut(&mut sessions, session_id, now)?;
        scope.in_flight_requests = scope.in_flight_requests.saturating_add(1);
        scope.last_seen_at = now;
        Some(ActiveSessionLease {
            store: Arc::clone(self),
            session_id: session_id.to_owned(),
        })
    }

    /// Called periodically; unlike lazy pruning this frees inactive sessions
    /// even if no new requests arrive. Never touches external commands/tunnels.
    pub fn maintenance_sweep(&self) -> (usize, usize) {
        let now = unix_now_secs();
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let before = sessions.len();
        Self::prune_expired(&mut sessions, now);
        let removed_sessions = before - sessions.len();
        if sessions.capacity() > 64 && sessions.len() * 4 < sessions.capacity() {
            sessions.shrink_to_fit();
        }
        let mut capabilities = self
            .computer_capabilities
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let before_capabilities = capabilities.len();
        capabilities.retain(|_, cap| {
            !is_capability_expired(cap, now) && sessions.contains_key(&cap.session_id)
        });
        let removed_capabilities = before_capabilities - capabilities.len();
        if capabilities.capacity() > 64 && capabilities.len() * 4 < capabilities.capacity() {
            capabilities.shrink_to_fit();
        }
        drop(capabilities);
        drop(sessions);
        let mut totals = self.maintenance.lock().unwrap_or_else(|e| e.into_inner());
        totals.last_sweep_at = now;
        totals.reclaimed_sessions += removed_sessions as u64;
        totals.reclaimed_capabilities += removed_capabilities as u64;
        (removed_sessions, removed_capabilities)
    }

    pub fn maintenance_stats(&self) -> SessionMaintenanceStats {
        self.maintenance
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Return a server-minted, principal-bound session for locally enabled
    /// Computer Use. Never accept an identifier supplied by the caller.
    pub fn computer_auto_session(&self, owner: &str) -> String {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_now_secs();
        if let Some((id, scope)) = sessions.iter_mut().find(|(id, scope)| {
            id.starts_with("computer-auto-")
                && !is_scope_expired(scope, now)
                && scope.issued_by_server
                && scope.computer_owner.as_deref() == Some(owner)
        }) {
            scope.last_seen_at = now;
            return id.clone();
        }
        Self::ensure_session_capacity(&mut sessions, now);
        let id = format!("computer-auto-{}", uuid::Uuid::new_v4());
        sessions.insert(
            id.clone(),
            GatewaySessionScope {
                active_workspace_id: None,
                last_seen_at: now,
                in_flight_requests: 0,
                issued_by_server: true,
                computer_owner: Some(owner.to_owned()),
            },
        );
        id
    }

    /// Create a short-lived, high-entropy capability for clients which drop
    /// the MCP transport session header between calls. The caller must first
    /// have a verified OAuth / bearer / local auth principal.
    pub fn issue_computer_capability(&self, session_id: &str, owner: &str) -> Option<String> {
        if !self.computer_owner_matches(session_id, owner) {
            return None;
        }
        let token = format!(
            "cu1_{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let mut capabilities = self
            .computer_capabilities
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_now_secs();
        if capabilities.len() >= MAX_COMPUTER_CAPABILITIES {
            capabilities.retain(|_, entry| !is_capability_expired(entry, now));
            while capabilities.len() >= MAX_COMPUTER_CAPABILITIES {
                let Some(oldest) = capabilities
                    .iter()
                    .min_by_key(|(_, entry)| entry.created_at)
                    .map(|(k, _)| k.clone())
                else {
                    break;
                };
                capabilities.remove(&oldest);
            }
        }
        capabilities.insert(
            computer_capability_hash(&token),
            ComputerCapability {
                session_id: session_id.to_string(),
                owner: owner.to_string(),
                created_at: now,
            },
        );
        Some(token)
    }

    pub fn resolve_computer_capability(&self, token: &str, owner: &str) -> Option<String> {
        if token.len() != 68
            || !token.starts_with("cu1_")
            || !token[4..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        let now = unix_now_secs();
        let hash = computer_capability_hash(token);
        let capability = {
            let mut capabilities = self
                .computer_capabilities
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let expired = capabilities
                .get(&hash)
                .is_some_and(|entry| is_capability_expired(entry, now));
            if expired {
                capabilities.remove(&hash);
                return None;
            }
            capabilities.get(&hash).cloned()
        }?;
        if capability.owner != owner || !self.computer_owner_matches(&capability.session_id, owner)
        {
            return None;
        }
        Some(capability.session_id)
    }

    pub fn revoke_computer_capabilities(&self, session_id: &str) {
        let mut capabilities = self
            .computer_capabilities
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        capabilities.retain(|_, entry| entry.session_id != session_id);
    }

    /// Compare a previously server-issued Computer Use session to the verified
    /// HTTP auth identity; client-supplied metadata never confers host rights.
    pub fn computer_owner_matches(&self, session_id: &str, owner: &str) -> bool {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_now_secs();
        Self::get_live_mut(&mut sessions, session_id, now).is_some_and(|scope| {
            scope.issued_by_server && scope.computer_owner.as_deref() == Some(owner)
        })
    }

    fn prune_expired(sessions: &mut HashMap<String, GatewaySessionScope>, now: u64) {
        sessions.retain(|_, scope| !is_scope_expired(scope, now));
    }

    /// Register an opaque host-provided session key (for example ChatGPT's
    /// request metadata) so state can survive clients that do not preserve
    /// the MCP transport session header between tool calls.
    pub fn ensure_host_session(&self, session_id: &str) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_now_secs();
        if let Some(scope) = Self::get_live_mut(&mut sessions, session_id, now) {
            scope.last_seen_at = now;
            return;
        }
        Self::ensure_session_capacity(&mut sessions, now);
        sessions.insert(
            session_id.to_string(),
            GatewaySessionScope {
                active_workspace_id: None,
                last_seen_at: now,
                in_flight_requests: 0,
                issued_by_server: false,
                computer_owner: None,
            },
        );
    }

    /// Issue an opaque server-owned session identifier.
    #[cfg(test)]
    pub fn issue(&self) -> String {
        self.issue_with_owner(None)
    }

    pub fn issue_with_owner(&self, computer_owner: Option<String>) -> String {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_now_secs();
        Self::ensure_session_capacity(&mut sessions, now);
        loop {
            let session_id = uuid::Uuid::new_v4().to_string();
            if sessions.contains_key(&session_id) {
                continue;
            }
            sessions.insert(
                session_id.clone(),
                GatewaySessionScope {
                    active_workspace_id: None,
                    last_seen_at: now,
                    in_flight_requests: 0,
                    issued_by_server: true,
                    computer_owner: computer_owner.clone(),
                },
            );
            return session_id;
        }
    }

    pub fn contains(&self, session_id: &str) -> bool {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::get_live_mut(&mut sessions, session_id, unix_now_secs()).is_some()
    }

    pub fn is_server_issued(&self, session_id: &str) -> bool {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::get_live_mut(&mut sessions, session_id, unix_now_secs())
            .is_some_and(|scope| scope.issued_by_server)
    }

    pub fn touch_existing(&self, session_id: &str) -> bool {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_now_secs();
        let Some(scope) = Self::get_live_mut(&mut sessions, session_id, now) else {
            return false;
        };
        scope.last_seen_at = now;
        true
    }

    pub fn current(&self, session_id: &str) -> GatewaySessionScope {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::get_live_mut(&mut sessions, session_id, unix_now_secs())
            .cloned()
            .unwrap_or_default()
    }

    pub fn select(&self, session_id: &str, workspace_id: String) -> bool {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = unix_now_secs();
        let Some(scope) = Self::get_live_mut(&mut sessions, session_id, now) else {
            return false;
        };
        scope.active_workspace_id = Some(workspace_id);
        scope.last_seen_at = now;
        true
    }

    pub fn clear_selection(&self, session_id: &str) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(scope) = sessions.get_mut(session_id) {
            scope.active_workspace_id = None;
            scope.last_seen_at = unix_now_secs();
        }
    }

    pub fn clear_workspace(&self, workspace_id: &str) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for scope in sessions.values_mut() {
            if scope.active_workspace_id.as_deref() == Some(workspace_id) {
                scope.active_workspace_id = None;
            }
        }
    }

    pub fn active_session_count(&self) -> usize {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::prune_expired(&mut sessions, unix_now_secs());
        sessions.len()
    }

    pub fn snapshots(&self) -> Vec<(String, GatewaySessionScope)> {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::prune_expired(&mut sessions, unix_now_secs());
        let mut rows = sessions
            .iter()
            .map(|(id, scope)| (id.clone(), scope.clone()))
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| std::cmp::Reverse(row.1.last_seen_at));
        rows
    }

    #[cfg(test)]
    pub(crate) fn register_for_test(&self, session_id: &str) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions
            .entry(session_id.to_string())
            .or_insert_with(|| GatewaySessionScope {
                active_workspace_id: None,
                last_seen_at: unix_now_secs(),
                in_flight_requests: 0,
                issued_by_server: true,
                computer_owner: None,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_session_is_reclaimed_but_active_rpc_is_preserved() {
        let store = Arc::new(GatewaySessionStore::default());
        let idle = store.issue_with_owner(None);
        let active = store.issue_with_owner(None);
        let lease = store.begin_request(&active).expect("valid active session");
        {
            let mut sessions = store.sessions.lock().unwrap();
            for scope in sessions.values_mut() {
                scope.last_seen_at = unix_now_secs() - SESSION_TTL_SECONDS - 10;
            }
        }
        let (removed, _) = store.maintenance_sweep();
        assert_eq!(removed, 1);
        assert!(!store.contains(&idle));
        assert!(store.contains(&active), "request lease prevents expiration");
        drop(lease);
        {
            let mut sessions = store.sessions.lock().unwrap();
            sessions.get_mut(&active).unwrap().last_seen_at =
                unix_now_secs() - SESSION_TTL_SECONDS - 10;
        }
        let (removed, _) = store.maintenance_sweep();
        assert_eq!(removed, 1);
        assert!(!store.contains(&active));
        let stats = store.maintenance_stats();
        assert_eq!(stats.reclaimed_sessions, 2);
        assert!(stats.last_sweep_at > 0);
    }

    #[test]
    fn stale_computer_capabilities_are_reclaimed_without_reviving_sessions() {
        let store = GatewaySessionStore::default();
        let session = store.issue_with_owner(Some("oauth:a".into()));
        let token = store
            .issue_computer_capability(&session, "oauth:a")
            .unwrap();
        {
            let mut caps = store.computer_capabilities.lock().unwrap();
            caps.get_mut(&computer_capability_hash(&token))
                .unwrap()
                .created_at = unix_now_secs() - COMPUTER_CAPABILITY_TTL_SECONDS - 1;
        }
        let (_, removed) = store.maintenance_sweep();
        assert_eq!(removed, 1);
        assert!(store.contains(&session));
        assert_eq!(store.resolve_computer_capability(&token, "oauth:a"), None);
    }

    #[test]
    fn authenticated_computer_sessions_are_server_issued_and_reused_per_owner() {
        let sessions = GatewaySessionStore::default();
        let a = sessions.computer_auto_session("oauth:client-a");
        assert!(a.starts_with("computer-auto-"));
        assert!(sessions.is_server_issued(&a));
        assert!(sessions.computer_owner_matches(&a, "oauth:client-a"));
        assert_eq!(a, sessions.computer_auto_session("oauth:client-a"));

        let b = sessions.computer_auto_session("oauth:client-b");
        assert_ne!(a, b);
        assert!(sessions.computer_owner_matches(&b, "oauth:client-b"));
        assert!(!sessions.computer_owner_matches(&a, "oauth:client-b"));
        assert!(!sessions.computer_owner_matches(&b, "oauth:client-a"));
    }

    #[test]
    fn client_supplied_metadata_cannot_forge_a_computer_auto_session() {
        let sessions = GatewaySessionStore::default();
        sessions.ensure_host_session("computer-auto-forged-client-id");
        assert!(!sessions.is_server_issued("computer-auto-forged-client-id"));
        assert!(
            !sessions.computer_owner_matches("computer-auto-forged-client-id", "oauth:client-a")
        );

        let valid = sessions.computer_auto_session("oauth:client-a");
        assert!(sessions.is_server_issued(&valid));
        sessions.ensure_host_session(&valid);
        assert!(sessions.computer_owner_matches(&valid, "oauth:client-a"));
    }

    #[test]
    fn capability_is_secret_owner_bound_expiring_and_revocable() {
        let sessions = GatewaySessionStore::default();
        let issued = sessions.issue_with_owner(Some("oauth:client-a".into()));
        let token = sessions
            .issue_computer_capability(&issued, "oauth:client-a")
            .unwrap();
        assert_eq!(
            sessions.resolve_computer_capability(&token, "oauth:client-a"),
            Some(issued.clone())
        );
        assert_eq!(
            sessions.resolve_computer_capability(&token, "oauth:client-b"),
            None
        );
        assert_eq!(
            sessions.resolve_computer_capability("cu1_invalid", "oauth:client-a"),
            None
        );
        assert_eq!(
            sessions.resolve_computer_capability(&format!("{token}x"), "oauth:client-a"),
            None
        );
        sessions.revoke_computer_capabilities(&issued);
        assert_eq!(
            sessions.resolve_computer_capability(&token, "oauth:client-a"),
            None
        );

        let expiring = sessions
            .issue_computer_capability(&issued, "oauth:client-a")
            .unwrap();
        let key = computer_capability_hash(&expiring);
        sessions
            .computer_capabilities
            .lock()
            .unwrap()
            .get_mut(&key)
            .unwrap()
            .created_at = unix_now_secs() - COMPUTER_CAPABILITY_TTL_SECONDS - 1;
        assert_eq!(
            sessions.resolve_computer_capability(&expiring, "oauth:client-a"),
            None
        );

        let metadata = "untrusted-client-supplied-id";
        sessions.ensure_host_session(metadata);
        assert_eq!(
            sessions.issue_computer_capability(metadata, "oauth:client-a"),
            None
        );
    }

    #[test]
    fn authenticated_client_owns_only_its_issued_session() {
        let store = GatewaySessionStore::default();
        let first = store.issue_with_owner(Some("oauth:client-a".into()));
        let second = store.issue_with_owner(Some("oauth:client-b".into()));
        assert!(store.computer_owner_matches(&first, "oauth:client-a"));
        assert!(store.computer_owner_matches(&second, "oauth:client-b"));
        assert!(!store.computer_owner_matches(&first, "oauth:client-b"));
        assert!(!store.computer_owner_matches(&second, "oauth:client-a"));
        assert!(!store.computer_owner_matches(&first, "shared_bearer"));
    }

    #[test]
    fn metadata_or_legacy_sessions_are_not_computer_credentials() {
        let store = GatewaySessionStore::default();
        store.ensure_host_session("client-chosen");
        assert!(!store.computer_owner_matches("client-chosen", "oauth:client-a"));
        let legacy = store.issue();
        assert!(store.is_server_issued(&legacy));
        assert!(!store.computer_owner_matches(&legacy, "oauth:client-a"));
        assert!(!store.computer_owner_matches("unknown", "oauth:client-a"));
    }

    #[test]
    fn session_store_enforces_capacity_cap_without_evicting_in_flight_leases() {
        let store = Arc::new(GatewaySessionStore::default());
        let active = store.issue_with_owner(None);
        let _lease = store.begin_request(&active).expect("lease");
        for i in 0..(MAX_SESSIONS + 16) {
            store.ensure_host_session(&format!("host-{i}"));
        }
        assert!(store.active_session_count() <= MAX_SESSIONS);
        assert!(
            store.contains(&active),
            "leased session must survive eviction"
        );
    }
}
