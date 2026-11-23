import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL("../" + path, import.meta.url), "utf8");

test("Cargo manifest separates desktop-ui feature from standalone zero-GTK headless binary", async () => {
  const cargoToml = await source("src-tauri/Cargo.toml");
  const buildRs = await source("src-tauri/build.rs");
  const headlessBin = await source("src-tauri/src/bin/coding-tools-mcp.rs");

  assert.match(cargoToml, /default\s*=\s*\["desktop-ui"\]/);
  assert.match(cargoToml, /desktop-ui\s*=\s*\["dep:tauri",\s*"dep:tauri-plugin-dialog",\s*"dep:tauri-build"\]/);
  assert.match(cargoToml, /name\s*=\s*"coding-tools-mcp"/);
  assert.match(buildRs, /#\[cfg\(feature = "desktop-ui"\)\]/);
  assert.match(headlessBin, /run_cli_entry/);
});

test("Linux platform primitives cover tcp6, WebKitGTK RSS sampling, and 0600 Unix socket IPC", async () => {
  const linuxNet = await source("src-tauri/src/platform/linux/net.rs");
  const linuxProc = await source("src-tauri/src/platform/linux/process.rs");
  const localIpc = await source("src-tauri/src/platform/local_ipc.rs");
  const singleInstance = await source("src-tauri/src/platform/single_instance.rs");
  const uiMemory = await source("src-tauri/src/commands/ui_memory.rs");

  assert.match(linuxNet, /\/proc\/net\/tcp6/);
  assert.match(linuxProc, /parse_statm_resident_bytes/);
  assert.match(linuxProc, /is_webkit_helper_process/);
  assert.match(uiMemory, /#\[cfg\(target_os = "linux"\)\]/);
  assert.match(localIpc, /0o700/);
  assert.match(localIpc, /0o600/);
  assert.match(localIpc, /start_daemon_ipc_server/);
  assert.match(singleInstance, /acquire_unix_desktop_instance_at/);
});

test("Headless CLI exposes daemon serve, workspace auto-roots, LAN PIN pairing, and systemd user service", async () => {
  const cli = await source("src-tauri/src/cli.rs");
  const daemon = await source("src-tauri/src/cli/daemon.rs");
  const wsCmd = await source("src-tauri/src/cli/workspace_cmd.rs");
  const lanCmd = await source("src-tauri/src/cli/lan_config_cmd.rs");
  const systemdCmd = await source("src-tauri/src/cli/systemd_cmd.rs");
  const autoDiscover = await source("src-tauri/src/workspace/auto_discover.rs");

  assert.match(cli, /"serve"/);
  assert.match(cli, /"workspace"/);
  assert.match(cli, /"lan"/);
  assert.match(cli, /"service"/);
  assert.match(daemon, /--pair-pin/);
  assert.match(wsCmd, /auto-root/);
  assert.match(lanCmd, /LocalDaemonRequest::GeneratePin/);
  assert.match(systemdCmd, /coding-tools-mcp\.service/);
  assert.match(autoDiscover, /discover_unregistered_git_workspaces/);
});

test("CI and Release workflows validate and package both Linux Headless and Linux Desktop targets", async () => {
  const ci = await source(".github/workflows/ci.yml");
  const release = await source(".github/workflows/release.yml");
  const discoveryCard = await source("src/components/settings/LanPeersDiscoveryCard.vue");
  const localCard = await source("src/components/settings/LanLocalDeviceCard.vue");

  assert.match(ci, /--no-default-features --lib --bin coding-tools-mcp/);
  assert.match(release, /build-linux-headless:/);
  assert.match(release, /build-linux-desktop:/);
  assert.match(release, /aarch64-unknown-linux-gnu/);
  assert.match(discoveryCard, /macOS \/ Windows \/ Linux/);
  assert.match(localCard, /macOS \/ Windows \/ Linux/);
});
