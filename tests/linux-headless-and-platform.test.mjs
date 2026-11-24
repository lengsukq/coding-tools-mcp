import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../", import.meta.url));
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

test("Linux one-click installer script passes syntax checks and matches release asset naming", async () => {
  const installerPath = join(repoRoot, "scripts/install-linux.sh");
  const installer = await source("scripts/install-linux.sh");
  const linuxDoc = await source("docs/linux-headless.md");
  const readme = await source("README.md");
  const readmeEn = await source("README.en.md");

  assert.match(installer, /-linux-\$\{ARCH\}-headless\.tar\.gz/);
  assert.match(installer, /--from-source/);
  assert.match(installer, /--mirror/);
  assert.match(installer, /--service/);
  assert.match(installer, /--auto-root/);
  assert.match(installer, /--uninstall/);
  assert.match(linuxDoc, /scripts\/install-linux\.sh/);
  assert.match(readme, /scripts\/install-linux\.sh/);
  assert.match(readme, /docs\/linux-headless\.md/);
  assert.match(readmeEn, /scripts\/install-linux\.sh/);
  assert.match(readmeEn, /docs\/linux-headless\.md/);

  if (process.platform !== "win32") {
    execFileSync("bash", ["-n", installerPath], { stdio: "pipe" });
    const helpOutput = execFileSync("bash", [installerPath, "--help"], {
      encoding: "utf8",
      stdio: "pipe",
    });
    assert.match(helpOutput, /--service/);
    assert.match(helpOutput, /--pair-pin/);
  }
});

test("Linux one-click installer supports local archive install, config flags, and uninstall", async () => {
  if (process.platform === "win32") {
    return;
  }
  const installerPath = join(repoRoot, "scripts/install-linux.sh");
  const sandbox = await mkdtemp(join(tmpdir(), "ct-mcp-install-test-"));
  try {
    const stageDir = join(sandbox, "stage");
    const installDir = join(sandbox, "bin");
    const logFile = join(sandbox, "invocations.log");
    const fakeBin = join(stageDir, "coding-tools-mcp");
    const archivePath = join(sandbox, "coding-tools-mcp-v0.3.8-linux-x86_64-headless.tar.gz");

    await mkdir(stageDir, { recursive: true });
    await writeFile(
      fakeBin,
      `#!/usr/bin/env bash
if [[ "$1" == "--version" ]]; then
  echo "coding-tools-mcp 0.3.8-test"
  exit 0
fi
printf '%s\\n' "$*" >> "${logFile}"
`,
      "utf8",
    );
    await chmod(fakeBin, 0o755);
    execFileSync("tar", ["-C", stageDir, "-czf", archivePath, "coding-tools-mcp"], {
      stdio: "pipe",
    });

    const env = {
      ...process.env,
      CODING_TOOLS_MCP_OS: "Linux",
      CODING_TOOLS_MCP_ARCH: "x86_64",
    };

    execFileSync(
      "bash",
      [
        installerPath,
        "--archive",
        archivePath,
        "--install-dir",
        installDir,
        "--device-name",
        "lab-linux",
        "--port",
        "28766",
        "--lan",
        "--auto-root",
        "/srv/code",
      ],
      { env, stdio: "pipe" },
    );

    const installedStat = await stat(join(installDir, "coding-tools-mcp"));
    assert.equal(installedStat.isFile(), true);
    const invocations = (await readFile(logFile, "utf8")).trim().split("\n");
    assert.deepEqual(invocations, [
      "config set device-name lab-linux",
      "config set port 28766",
      "lan enable",
      "workspace auto-root add /srv/code",
    ]);

    execFileSync(
      "bash",
      [installerPath, "--uninstall", "--install-dir", installDir],
      { env, stdio: "pipe" },
    );
    await assert.rejects(stat(join(installDir, "coding-tools-mcp")));
    const afterUninstall = (await readFile(logFile, "utf8")).trim().split("\n");
    assert.equal(afterUninstall.at(-1), "service uninstall");
  } finally {
    await rm(sandbox, { recursive: true, force: true });
  }
});
