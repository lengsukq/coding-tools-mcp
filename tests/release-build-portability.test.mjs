import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import ts from "typescript";

import { parseUniversalArgs, universalBinaryPaths } from "../scripts/build-macos-universal.mjs";

const repoRoot = fileURLToPath(new URL("../", import.meta.url));
const source = (path) => readFile(join(repoRoot, path), "utf8");

test("Windows CRLF checkout does not break source-extraction tests or version checks", async () => {
  const lanComposableCrlf = (await source("src/lib/lan/use-lan-peers.ts")).replace(/\r?\n/g, "\r\n");
  const lanMatch = lanComposableCrlf.match(
    /export function parseExtraSubnetsText[\s\S]*?return out;\r?\n\}/,
  );
  assert.ok(lanMatch, "parseExtraSubnetsText regex must match CRLF sources");

  const draftCrlf = (await source("src/lib/upstream-mcp/upstream-draft.ts")).replace(
    /\r?\n/g,
    "\r\n",
  );
  const pureTs = draftCrlf.replace(/import type[\s\S]*?;\r?\n/, "");
  assert.doesNotMatch(pureTs, /^import type\b/m);
  const compiled = ts.transpileModule(pureTs, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  assert.match(compiled, /createEmptyUpstreamDraft/);

  const cargoLockCrlf = (await source("src-tauri/Cargo.lock")).replace(/\r?\n/g, "\r\n");
  const cargoLockVersion = cargoLockCrlf.match(
    /\[\[package\]\]\r?\nname = "coding-tools-mcp-desktop"\r?\nversion = "([^"]+)"/,
  )?.[1];
  assert.ok(cargoLockVersion, "Cargo.lock version regex must match CRLF checkouts");
});

test("parseUniversalArgs and universalBinaryPaths resolve macOS dual-binary build options", () => {
  assert.deepEqual(parseUniversalArgs([]), { bundles: "dmg", passthrough: [] });
  assert.deepEqual(parseUniversalArgs(["--bundles", "app,dmg", "--ci"]), {
    bundles: "app,dmg",
    passthrough: ["--ci"],
  });
  assert.deepEqual(parseUniversalArgs(["--bundles=dmg", "--no-sign"]), {
    bundles: "dmg",
    passthrough: ["--no-sign"],
  });
  assert.throws(() => parseUniversalArgs(["--bundles"]));
  assert.throws(() => parseUniversalArgs(["--bundles="]));

  const paths = universalBinaryPaths("/tmp/demo-repo");
  assert.equal(
    paths.arm64,
    resolve("/tmp/demo-repo/src-tauri/target/aarch64-apple-darwin/release/coding-tools-mcp"),
  );
  assert.equal(
    paths.x86_64,
    resolve("/tmp/demo-repo/src-tauri/target/x86_64-apple-darwin/release/coding-tools-mcp"),
  );
  assert.equal(
    paths.universal,
    resolve("/tmp/demo-repo/src-tauri/target/universal-apple-darwin/release/coding-tools-mcp"),
  );
});

test("macOS release workflows and bundle verifier handle universal coding-tools-mcp binary", async () => {
  const releaseYml = await source(".github/workflows/release.yml");
  const macosReleaseYml = await source(".github/workflows/macos-release.yml");
  const buildScript = await source("scripts/build-macos-universal.mjs");
  const verifyScript = await source("scripts/verify-macos-bundle.sh");

  assert.match(releaseYml, /node scripts\/build-macos-universal\.mjs --bundles dmg/);
  assert.match(macosReleaseYml, /node scripts\/build-macos-universal\.mjs --bundles app,dmg/);
  assert.match(buildScript, /"--no-bundle"/);
  assert.match(buildScript, /"lipo"/);
  assert.match(buildScript, /"bundle"/);
  assert.match(verifyScript, /Contents\/MacOS\/coding-tools-mcp/);
  assert.match(verifyScript, /lipo -archs "\$\{CLI_BIN\}"/);

  if (process.platform !== "win32") {
    const verifyPath = join(dirname(fileURLToPath(import.meta.url)), "../scripts/verify-macos-bundle.sh");
    execFileSync("bash", ["-n", verifyPath], { stdio: "pipe" });
  }
});
