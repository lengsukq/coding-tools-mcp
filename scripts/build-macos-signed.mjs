#!/usr/bin/env node
// Screen Recording TCC grants require a stable code-signing identity across
// builds. An ad-hoc signed .app may display as enabled in System Settings
// while CGPreflightScreenCaptureAccess() still returns false.
import { spawnSync } from "node:child_process";

if (process.platform !== "darwin") {
  console.error("Signed macOS DMG builds must run on macOS.");
  process.exit(1);
}

function discoverUniqueCodesigningIdentity() {
  const probe = spawnSync("security", ["find-identity", "-v", "-p", "codesigning"], {
    encoding: "utf8",
    shell: false,
  });
  if (probe.status !== 0) {
    return { identities: [], stderr: probe.stderr?.trim() || probe.error?.message || "" };
  }
  const matches = [...probe.stdout.matchAll(/^\s*\d+\)\s+([0-9A-Fa-f]{40})\s+"([^"]+)"/gm)].map(
    ([, sha1, label]) => ({ sha1, label }),
  );
  return { identities: matches, stderr: "" };
}

let identity = process.env.APPLE_SIGNING_IDENTITY?.trim() || "";
if (!identity) {
  const { identities, stderr } = discoverUniqueCodesigningIdentity();
  if (identities.length === 1) {
    identity = identities[0].sha1;
    console.log(
      `Auto-discovered unique macOS code-signing identity: ${identities[0].label} (${identity})`,
    );
  } else {
    console.error("APPLE_SIGNING_IDENTITY is not set and could not auto-select a unique identity.");
    if (identities.length > 1) {
      console.error("Available code-signing identities in keychain:");
      for (const item of identities) {
        console.error(`  - ${item.sha1}  "${item.label}"`);
      }
    } else if (stderr) {
      console.error(`security find-identity failed: ${stderr}`);
    } else {
      console.error("No valid code-signing certificates found in your macOS keychain.");
    }
    console.error(
      'Re-run with: APPLE_SIGNING_IDENTITY="<SHA-1 or Certificate Name>" pnpm run desktop:build:signed',
    );
    process.exit(1);
  }
}

if (identity === "-") {
  console.error(
    "APPLE_SIGNING_IDENTITY='-' is an ad-hoc signature and cannot reliably " +
      "preserve Screen Recording TCC permissions between builds. Use an Apple " +
      "Development or Developer ID Application certificate instead.",
  );
  process.exit(1);
}

const extraArgs = process.argv.slice(2).filter((arg, index) => !(index === 0 && arg === "--"));
const args = [
  "run",
  "desktop:build",
  "--target",
  "aarch64-apple-darwin",
  ...extraArgs,
];
console.log(`Building ARM64 DMG with Apple code-signing identity: ${identity}`);
const result = spawnSync("pnpm", args, {
  stdio: "inherit",
  env: { ...process.env, APPLE_SIGNING_IDENTITY: identity },
  shell: false,
});
if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}
process.exit(result.status ?? 1);
