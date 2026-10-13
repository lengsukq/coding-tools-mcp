#!/usr/bin/env node
// Screen Recording TCC grants require a stable code-signing identity across
// builds. An ad-hoc signed .app may display as enabled in System Settings
// while CGPreflightScreenCaptureAccess() still returns false.
import { spawnSync } from "node:child_process";

if (process.platform !== "darwin") {
  console.error("Signed macOS DMG builds must run on macOS.");
  process.exit(1);
}

const identity = process.env.APPLE_SIGNING_IDENTITY?.trim();
if (!identity || identity === "-") {
  console.error(
    "Missing a stable APPLE_SIGNING_IDENTITY. Use an Apple Development or " +
    "Developer ID Application certificate in your macOS keychain. " +
    "Ad-hoc signatures ('-') cannot reliably preserve Screen Recording TCC " +
    "permissions between builds. See docs/specs/computer-use/design.md."
  );
  process.exit(1);
}

const args = [
  "run", "desktop:build", "--", "--target", "aarch64-apple-darwin",
  ...process.argv.slice(2),
];
console.log("Building ARM64 DMG with the configured Apple code-signing identity.");
const result = spawnSync("npm", args, {
  stdio: "inherit",
  env: process.env,
  shell: false,
});
if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}
process.exit(result.status ?? 1);
