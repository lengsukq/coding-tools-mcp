import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = async (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("global and workspace context UI hide harnesses by default and render checkable skills/agents picker", async () => {
  const globalCard = await source("src/components/settings/GeneralContextCard.vue");
  const workspacePanel = await source("src/components/workspace/RuntimePolicyPanel.vue");
  const picker = await source("src/components/settings/AgentContextPicker.vue");

  assert.match(globalCard, /const showAdvancedSources = ref\(false\)/);
  assert.match(globalCard, /<AgentContextPicker/);
  assert.match(globalCard, /v-if="showAdvancedSources"/);

  assert.match(workspacePanel, /const showAdvancedSources = ref\(false\)/);
  assert.match(workspacePanel, /<AgentContextPicker/);
  assert.match(workspacePanel, /v-if="showAdvancedSources"/);
  assert.match(workspacePanel, /onMounted\(/);

  assert.match(picker, /type="checkbox"/);
  assert.match(picker, /update:disabledSkillIds/);
  assert.match(picker, /update:disabledInstructionIds/);
  assert.match(picker, /已在全局禁用/);
  assert.match(picker, /不注入/);
});

test("startup settings expose launch-at-login and lightweight autostart controls wired to native lifecycle", async () => {
  const settingsView = await source("src/views/settings/GeneralSettingsView.vue");
  const libRs = await source("src-tauri/src/lib.rs");
  const tauriConf = await source("src-tauri/tauri.conf.json");
  const chromeRs = await source("src-tauri/src/commands/window_chrome.rs");
  const autostartRs = await source("src-tauri/src/platform/autostart.rs");

  assert.match(settingsView, /runtime\.launchAtLogin/);
  assert.match(settingsView, /runtime\.launchInLightweightMode/);
  assert.match(settingsView, /开机自动启动/);
  assert.match(settingsView, /开机时轻量启动/);

  assert.match(tauriConf, /"create":\s*false/);
  assert.match(libRs, /should_start_lightweight/);
  assert.match(libRs, /enter_lightweight_startup/);
  assert.match(libRs, /create_main_window_from_config/);
  assert.match(libRs, /restore_runtime_state_inner/);
  assert.match(libRs, /RunEvent::Reopen/);
  assert.match(chromeRs, /pub fn enter_lightweight_startup/);
  assert.match(autostartRs, /LaunchAgents/);
  assert.match(autostartRs, /--autostart/);
});

test("agent_context follows symlinks and parses multiline YAML frontmatter", async () => {
  const discovery = await source("src-tauri/src/agent_context/discovery.rs");
  const presentation = await source("src-tauri/src/agent_context/presentation.rs");

  assert.match(discovery, /\.follow_links\(true\)/);
  assert.match(discovery, /"\.agents\/skills"/);
  assert.match(discovery, /"\.gemini\/config\/skills"/);
  assert.match(presentation, /parse_block_scalar_header/);
  assert.match(presentation, /join_folded_block/);
});
