import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = async path => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("compact mode destroys the macOS WebView without stopping MCP services", async () => {
  const chrome = await source("src-tauri/src/commands/window_chrome.rs");
  const memory = await source("src-tauri/src/commands/ui_memory.rs");
  assert.match(chrome, /pub fn enter_compact_mode\(/);
  assert.match(chrome, /window\.destroy\(\)/);
  assert.match(chrome, /WebviewWindowBuilder::from_config/);
  assert.match(chrome, /SAVED_GEOMETRY/);
  assert.match(chrome, /set_activation_policy\(ActivationPolicy::Accessory\)/);
  assert.match(chrome, /set_activation_policy\(ActivationPolicy::Regular\)/);
  assert.match(memory, /window_chrome::is_compact_mode\(\)/);
  assert.match(memory, /window_chrome::is_exit_armed\(\)/);
  assert.match(chrome, /if is_compact_mode\(\)/);
  assert.doesNotMatch(chrome, /stop_runtime|stop_global_gateway|stop_tunnel/);
});

test("macOS menu reports status, memory and offers restore, refresh, quit", async () => {
  const desktop = await source("src-tauri/src/lib.rs");
  assert.match(desktop, /"恢复主窗口"/);
  assert.match(desktop, /"刷新内存与服务状态"/);
  assert.match(desktop, /"完全退出"/);
  assert.match(desktop, /"MCP 服务：/);
  assert.match(desktop, /主界面：已卸载（无活动主 WebView）/);
  assert.match(desktop, /WebKit 辅助进程（可能滞留）/);
  assert.match(desktop, /"内存估算：/);
  assert.match(desktop, /"工作区：/);
  assert.match(desktop, /show_menu_on_left_click\(cfg!\(target_os = "macos"\)\)/);
  assert.match(desktop, /Duration::from_secs\(15\)/);
});

test("tray compact request routes through unsaved changes guard", async () => {
  const desktop = await source("src-tauri/src/lib.rs");
  const app = await source("src/App.vue");
  assert.match(desktop, /app\.emit\("compact-requested"/);
  assert.match(app, /listen\("compact-requested"/);
  assert.match(app, /confirmUnsavedChanges\(\)/);
  assert.match(app, /enterCompactMode\(\)/);
  const settings = await source("src/views/settings/GeneralSettingsView.vue");
  assert.match(settings, /轻量驻留（关闭界面）/);
});
