import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

const source = (path) => readFile(new URL("../" + path, import.meta.url), "utf8");

test("LAN peer subsystem follows modular facade architecture across Rust and Vue", async () => {
  const facade = await source("src-tauri/src/lan_peer.rs");
  assert.match(facade, /pub mod client;/);
  assert.match(facade, /pub mod crypto;/);
  assert.match(facade, /pub mod discovery;/);
  assert.match(facade, /pub mod identity;/);
  assert.match(facade, /pub mod pairing;/);
  assert.match(facade, /pub mod server;/);

  const view = await source("src/views/settings/LanPeersSettingsView.vue");
  const composable = await source("src/lib/lan/use-lan-peers.ts");
  const localCard = await source("src/components/settings/LanLocalDeviceCard.vue");
  const discoveryCard = await source("src/components/settings/LanPeersDiscoveryCard.vue");

  assert.match(view, /useLanPeers\(\)/);
  assert.match(view, /<LanLocalDeviceCard/);
  assert.match(view, /<LanPeersDiscoveryCard/);
  assert.doesNotMatch(view, /getLanPeersOverview\(|pairLanPeer\(/);
  assert.match(composable, /export function useLanPeers\(\)/);
  assert.match(composable, /export function parseExtraSubnetsText\(/);
  assert.match(composable, /useUnsavedChanges\("局域网多设备配对"/);
  assert.match(composable, /seq !== loadSequence/);
  assert.doesNotMatch(localCard, /invoke\(|getLanPeersOverview\(/);
  assert.doesNotMatch(discoveryCard, /invoke\(|pairLanPeer\(/);
});

test("LAN discovery and network enumeration support macOS, Windows, and EasyTier / overlay subnets", async () => {
  const localNet = await source("src-tauri/src/local_network.rs");
  const discovery = await source("src-tauri/src/lan_peer/discovery.rs");
  const client = await source("src-tauri/src/lan_peer/client.rs");

  assert.match(localNet, /#\[cfg\(unix\)\]/);
  assert.match(localNet, /libc::getifaddrs/);
  assert.match(localNet, /utun/);
  assert.match(localNet, /#\[cfg\(windows\)\]/);
  assert.match(localNet, /to_socket_addrs/);
  assert.match(localNet, /windows_ipconfig_ipv4s/);
  assert.match(localNet, /10\.144\.144\.1:80/);
  assert.match(localNet, /10\.126\.126\.1:80/);
  assert.match(localNet, /normalize_extra_subnets/);
  assert.match(localNet, /collect_overlay_discovery_targets/);
  assert.match(discovery, /StdUdpSocket::bind\(\("0\.0\.0\.0", LAN_DISCOVERY_UDP_PORT\)\)/);
  assert.match(discovery, /join_multicast_v4/);
  assert.match(discovery, /probe_configured_overlay_targets/);
  assert.match(discovery, /probe_peer_hello/);
  assert.doesNotMatch(discovery, /unsafe\s*\{/);
  assert.match(client, /remember_preferred_peer_host/);
});

test("Local loopback Agent zero-OAuth direct access enforces anti-tunnel, anti-Origin, and LAN scope guards", async () => {
  const listener = await source("src-tauri/src/mcp/listener.rs");
  const generalSettings = await source("src/views/settings/GeneralSettingsView.vue");
  const localCard = await source("src/components/settings/LanLocalDeviceCard.vue");

  assert.match(listener, /is_direct_loopback_agent/);
  assert.match(listener, /check_mcp_surface_scope/);
  assert.match(listener, /LAN_MCP_DISABLED/);
  assert.match(listener, /NOAUTH_LOOPBACK_ONLY/);
  assert.match(listener, /allow_local_noauth/);
  assert.match(listener, /"local_loopback"/);
  assert.match(generalSettings, /allowLocalNoauth/);
  assert.match(localCard, /allowLocalNoauth/);
  assert.match(localCard, /extraSubnetsText/);
});

test("parseExtraSubnetsText deduplicates comma and newline EasyTier CIDR / seed entries", async () => {
  const composableSrc = await source("src/lib/lan/use-lan-peers.ts");
  // Extract pure helper for fast isolated verification
  const match = composableSrc.match(/export function parseExtraSubnetsText[\s\S]*?return out;\n\}/);
  assert.ok(match, "parseExtraSubnetsText helper exists");
  const jsCode = ts.transpileModule(match[0], {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const { parseExtraSubnetsText } = await import(`data:text/javascript,${encodeURIComponent(jsCode)}`);
  assert.deepEqual(
    parseExtraSubnetsText(" 10.144.144.0/24, 192.168.2.0/24 \n 10.144.144.0/24 ; 10.144.144.2:28765 "),
    ["10.144.144.0/24", "192.168.2.0/24", "10.144.144.2:28765"],
  );
});

test("LAN server hard-blocks public tunnel headers, non-LAN IPs, and multi-hop loops", async () => {
  const localNet = await source("src-tauri/src/local_network.rs");
  const server = await source("src-tauri/src/lan_peer/server.rs");
  const crypto = await source("src-tauri/src/lan_peer/crypto.rs");

  assert.match(localNet, /PROXY_FORWARDED_HEADERS/);
  assert.match(localNet, /"cf-connecting-ip"/);
  assert.match(localNet, /"x-forwarded-for"/);
  assert.match(localNet, /"via"/);
  assert.match(localNet, /"true-client-ip"/);
  assert.match(server, /has_proxy_forwarded_headers\(/);
  assert.match(server, /PUBLIC_TUNNEL_FORBIDDEN/);
  assert.match(server, /NON_LAN_ORIGIN_FORBIDDEN/);
  assert.match(server, /is_allowed_peer_ip/);
  assert.match(server, /FEDERATION_LOOP_DENIED/);
  assert.match(server, /REPLAY_DETECTED/);
  assert.match(server, /PEER_READONLY_POLICY/);
  assert.match(crypto, /pub fn constant_time_eq\(/);
  assert.match(crypto, /pub struct ReplayGuard/);
});

test("Global MCP routes composite {device_id}:{workspace_id}, device_list, computer_*, and peer reviews", async () => {
  const handlers = await source("src-tauri/src/mcp/gateway_handlers.rs");
  const scope = await source("src-tauri/src/mcp/server/scope.rs");
  const dispatch = await source("src-tauri/src/mcp/server/dispatch.rs");
  const listener = await source("src-tauri/src/mcp/listener.rs");
  const reviewHttp = await source("src-tauri/src/mcp/listener/review_http.rs");
  const computerSchema = await source("src-tauri/src/computer/schema.rs");

  assert.match(handlers, /"device_list"/);
  assert.match(scope, /parse_workspace_scope\(/);
  assert.match(scope, /ResolvedToolWorkspace::Remote/);
  assert.match(dispatch, /forward_workspace_tool\(/);
  assert.match(dispatch, /forward_computer_tool\(/);
  assert.match(dispatch, /COMPUTER_DEVICE_AMBIGUOUS/);
  assert.match(computerSchema, /"device_id"/);
  assert.match(listener, /\/review\/peer\/\{device_id\}\/\{id\}/);
  assert.match(listener, /lan_router\(lan_gateway\)/);
  assert.match(reviewHttp, /pub\(super\) async fn peer_review_page\(/);
});
