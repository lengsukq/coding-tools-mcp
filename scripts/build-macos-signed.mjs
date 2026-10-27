#!/usr/bin/env node
// Screen Recording TCC grants require a stable code-signing identity across
// builds. An ad-hoc signed .app may display as enabled in System Settings
// while CGPreflightScreenCaptureAccess() still returns false.
import { spawnSync } from "node:child_process";

if (process.platform !== "darwin") {
  console.error("Signed macOS DMG builds must run on macOS.");
  process.exit(1);
}

// Stable local development certificate (SHA-1 identity from the macOS keychain).
// Override with APPLE_SIGNING_IDENTITY when building on another Mac or for distribution.
const DEFAULT_APPLE_SIGNING_IDENTITY =
  "7A35324EE5EBA56A3047EA2728B949E20AEB38A6";
const identity =
  process.env.APPLE_SIGNING_IDENTITY?.trim() || DEFAULT_APPLE_SIGNING_IDENTITY;
if (identity === "-") {
  console.error(
    "APPLE_SIGNING_IDENTITY='-' is an ad-hoc signature and cannot reliably " +
    "preserve Screen Recording TCC permissions between builds. Use an Apple " +
    "Development or Developer ID Application certificate instead."
  );
  process.exit(1);
}

const args = [
  "run", "desktop:build", "--", "--target", "aarch64-apple-darwin",
  ...process.argv.slice(2),
];
console.log(
  `Building ARM64 DMG with Apple code-signing identity: ${identity}`
);
const result = spawnSync("npm", args, {
  stdio: "inherit",
  env: { ...process.env, APPLE_SIGNING_IDENTITY: identity },
  shell: false,
});
if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}
process.exit(result.status ?? 1);
