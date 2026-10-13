import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const rust = readFileSync(new URL("../src-tauri/src/computer.rs", import.meta.url), "utf8");
const native = readFileSync(new URL("../src-tauri/src/platform/macos/computer_native.m", import.meta.url), "utf8");
const ui = readFileSync(new URL("../src/views/settings/GatewaySettingsView.vue", import.meta.url), "utf8");

test("false CG preflight is only diagnostic, never a hard blocker for local consent", () => {
  const approval = rust.split("pub fn local_approve(")[1]?.split("pub fn local_revoke(")[0] ?? "";
  const resume = rust.split("pub fn local_resume(")[1]?.split("struct Snapshot")[0] ?? "";
  assert.ok(approval.includes("locked().approve(request_id, app_bundle)"));
  assert.doesNotMatch(approval, /screen_recording_permission\s*\(/);
  assert.doesNotMatch(resume, /screen_recording_permission\s*\(/);
  assert.match(approval, /accessibility_permission\s*\(/);
});

test("ScreenCaptureKit is reached after local MCP consent, with macOS TCC as authority", () => {
  assert.doesNotMatch(native, /if\s*\(!hasPermission\s*\(\)\s*\)/);
  assert.match(native, /SCScreenshotManager\s+captureImageWithFilter/);
  assert.match(native, /SCShareableContent\s+getShareableContentExcludingDesktopWindows/);
  assert.match(native, /if\s*\(isScreenLocked\s*\(\)\)/);
  const observe = rust.split('("computer_observe", "windows" | "screenshot" | "ui_tree")')[1] ?? "";
  assert.match(observe, /observation_context\s*\(session\)/);
  assert.match(observe, /COMPUTER_NOT_APPROVED/);
  assert.match(ui, /approveComputerScreenRequest\(/);
  assert.doesNotMatch(ui, /!computer\.screenRecordingGranted\s*\|\|/);
});
