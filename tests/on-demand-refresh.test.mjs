import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = async (path) =>
  readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("sidebar workspace items render cached status without subscribing or polling", async () => {
  const sidebar = await source("src/components/WorkspaceNavItem.vue");
  const cache = await source("src/lib/workspace-status-cache.ts");
  assert.match(sidebar, /peekCachedWorkspaceGitSummary/);
  assert.match(sidebar, /peekCachedPlanningState/);
  assert.doesNotMatch(sidebar, /useWorkspaceStatus\s*\(/);
  assert.doesNotMatch(cache, /setInterval\s*\(/);
  assert.match(cache, /watch\(read, value =>/);
});

test("dashboard only samples usage while expanded and history is loaded on demand", async () => {
  const dashboard = await source("src/views/DashboardView.vue");
  assert.match(dashboard, /watch\(analyticsOpen, open =>/);
  assert.match(dashboard, /if \(!open\) return/);
  assert.match(dashboard, /if \(visible\('activity'\) &&/);
  assert.match(dashboard, /loadedHistoryKey/);
});

test("connection health is not checked on every app focus or via a root timer", async () => {
  const shell = await source("src/components/AppShell.vue");
  const health = await source("src/lib/stores/health.ts");
  const gateway = await source("src/views/settings/GatewaySettingsView.vue");
  assert.doesNotMatch(shell, /startGlobalHealthMonitor/);
  assert.doesNotMatch(health, /setInterval\s*\(/);
  assert.match(gateway, /recordConnectionHealth/);
  assert.match(gateway, /@click="runHealth"/);
});

test("Windows internal Git commands suppress console windows", async () => {
  const platform = await source("src-tauri/src/platform/command.rs");
  const workspace = await source("src-tauri/src/commands/workspace.rs");
  assert.match(platform, /#\[cfg\(windows\)\]/);
  assert.match(platform, /CREATE_NO_WINDOW/);
  assert.match(workspace, /background_command\("git"\)/);
  assert.match(workspace, /spawn_blocking\(move \|\| compute_workspace_git_summary\(root\)\)/);
});
