import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const rust = readFileSync(new URL("../src-tauri/src/computer.rs", import.meta.url), "utf8");
const native = readFileSync(new URL("../src-tauri/src/platform/macos/computer_native.m", import.meta.url), "utf8");
const ui = readFileSync(new URL("../src/views/settings/ComputerSettingsView.vue", import.meta.url), "utf8");
const gatewayUi = readFileSync(new URL("../src/views/settings/GatewaySettingsView.vue", import.meta.url), "utf8");
const computerApi = readFileSync(new URL("../src/lib/api/computer.ts", import.meta.url), "utf8");
const listener = readFileSync(new URL("../src-tauri/src/mcp/listener.rs", import.meta.url), "utf8");
const macPlist = readFileSync(new URL("../src-tauri/Info.plist", import.meta.url), "utf8");

test("signed macOS app declares ScreenCaptureKit usage and diagnoses that exact running bundle", () => {
  assert.match(macPlist, /<key>NSScreenCaptureUsageDescription<\/key>\s*<string>[^<]+<\/string>/);
  assert.match(native, /objectForInfoDictionaryKey:@"NSScreenCaptureUsageDescription"/);
  assert.match(ui, /capture_usage_description_present/);
  assert.match(ui, /tccutil reset ScreenCapture com\.codingtools\.mcp\.desktop/);
  assert.doesNotMatch(ui, /tccutil reset All(?=\s|["'])/);
});

test("fast Computer Use navigation and capture keep local browser-control boundary", () => {
  assert.match(rust, /"open_url" => &\["url"\]/);
  assert.match(rust, /Some\("focus_app" \| "open_url"\)/);
  assert.match(rust, /"quality": \{ "type": "string", "enum": \["fast", "detailed"\]/);
  assert.match(native, /\[browsers containsObject:target\]/);
  assert.match(native, /withApplicationAtURL:browser\.bundleURL/);
  assert.match(native, /NSStringEnumerationByComposedCharacterSequences/);
  assert.match(native, /double maxWidth = detailed \? 1920\.0 : 1280\.0/);
});

test("Computer Use has a dedicated settings component and does not poll on gateway tab", () => {
  assert.match(ui, /id="settings-computer"/);
  assert.match(ui, /computerAuditPoll = setInterval/);
  assert.doesNotMatch(gatewayUi, /computerAuditPoll|enableComputerAccess|getComputerScreenStatus/);
  assert.match(gatewayUi, /id="settings-gateway"/);
});

test("manually enabled Computer Use does not require request and audits client identity", () => {
  assert.match(computerApi, /invoke<ComputerScreenStatus>\("enable_computer_access"/);
  assert.match(ui, /永久（直到手动关闭）/);
  assert.match(ui, /confirmPermanent/);
  assert.match(ui, /listToolAuditRecords\(\{ search: "computer_"/);
  assert.match(ui, /record\.clientId/);
  assert.match(listener, /sessions\.computer_auto_session\(owner\)/);
  assert.match(listener, /audit_auth_identity\(&state, &headers\)\.0 != "none"/);
});

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
