//! Symmetric LAN peer discovery, 6-digit PIN mutual pairing, and single-hop
//! federated MCP gateway transport across macOS, Windows, and Linux.

pub mod client;
pub mod crypto;
pub mod discovery;
pub mod identity;
pub mod pairing;
pub mod server;
#[cfg(test)]
mod tests;

pub const LAN_PROTOCOL_VERSION: &str = "ctm-lan-v1";
pub const PEER_ID_HEADER: &str = "x-ctm-peer-id";
pub const TIMESTAMP_HEADER: &str = "x-ctm-timestamp";
pub const NONCE_HEADER: &str = "x-ctm-nonce";
pub const HOP_HEADER: &str = "x-ctm-hop";
pub const SIGNATURE_HEADER: &str = "x-ctm-signature";

pub use client::{
    federated_device_list, federated_workspace_descriptors, fetch_peer_review,
    forward_computer_tool, forward_workspace_select, forward_workspace_tool,
    invalidate_peer_cache, list_paired_peer_statuses_async, notify_peer_unpair,
    PairedPeerStatusDto,
};
pub use discovery::{
    list_discovered_peers, probe_configured_overlay_targets, probe_peer_hello,
    sync_discovery_daemon, trigger_discovery_scan, DiscoveredPeerDto,
};
pub use identity::{
    ensure_local_identity, format_remote_workspace_scope, get_peer_secret, identity_from_settings,
    local_device_summary, parse_workspace_scope, LocalDeviceDto, PeerDeviceMetaDto,
    WorkspaceTarget, LAN_PEER_SECRETS_SCOPE,
};
pub use pairing::{
    cancel_pairing_pin, current_pairing_pin, generate_pairing_pin, pair_with_peer,
    remove_local_paired_peer, rename_paired_peer, update_paired_peer_enabled, PairingPinDto,
};
pub use server::lan_router;

/// Reclaim expired LAN discovery, catalog, and replay-guard caches.
pub fn prune_caches() {
    client::prune_peer_cache();
    discovery::prune_discovered_peers();
    server::prune_replay_cache();
}
