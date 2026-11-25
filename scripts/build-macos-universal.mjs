#!/usr/bin/env node
// Tauri v2's `universal-apple-darwin` build only runs `lipo` on the main
// `default-run` binary (`coding-tools-mcp-desktop`), while `tauri-bundler`
// copies every `[[bin]]` declared in `src-tauri/Cargo.toml` (including
// `coding-tools-mcp`). Build with `--no-bundle`, create the universal
// `coding-tools-mcp` binary with `lipo`, then run `tauri bundle`.
import { chmodSync, existsSync, mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const DEFAULT_REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const UNIVERSAL_TARGET = "universal-apple-darwin";
const SECONDARY_BIN_NAME = "coding-tools-mcp";

export function parseUniversalArgs(argv) {
  let bundles = "dmg";
  const passthrough = [];

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--bundles" || arg === "-b") {
      const value = argv[i + 1];
      if (!value) {
        throw new Error(`Missing value after ${arg}`);
      }
      bundles = value;
      i += 1;
    } else if (arg.startsWith("--bundles=")) {
      bundles = arg.slice("--bundles=".length);
      if (!bundles) {
        throw new Error("Missing value for --bundles=");
      }
    } else {
      passthrough.push(arg);
    }
  }

  return { bundles, passthrough };
}

export function universalBinaryPaths(repoRoot = DEFAULT_REPO_ROOT, binName = SECONDARY_BIN_NAME) {
  const targetDir = resolve(repoRoot, "src-tauri", "target");
  return {
    arm64: resolve(targetDir, "aarch64-apple-darwin", "release", binName),
    x86_64: resolve(targetDir, "x86_64-apple-darwin", "release", binName),
    universalDir: resolve(targetDir, UNIVERSAL_TARGET, "release"),
    universal: resolve(targetDir, UNIVERSAL_TARGET, "release", binName),
  };
}

function runOrExit(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: "inherit",
    env: process.env,
    shell: false,
  });
  if (result.error) {
    throw new Error(`Failed to execute ${command}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

export function buildMacosUniversal(argv = process.argv.slice(2), repoRoot = DEFAULT_REPO_ROOT) {
  if (process.platform !== "darwin") {
    throw new Error("Universal macOS DMG builds must run on macOS.");
  }

  const { bundles, passthrough } = parseUniversalArgs(argv);
  const pnpmCmd = "pnpm";

  runOrExit(
    pnpmCmd,
    ["exec", "tauri", "build", "--target", UNIVERSAL_TARGET, "--no-bundle", ...passthrough],
    repoRoot,
  );

  const bins = universalBinaryPaths(repoRoot);
  for (const [arch, binPath] of [
    ["aarch64-apple-darwin", bins.arm64],
    ["x86_64-apple-darwin", bins.x86_64],
  ]) {
    if (!existsSync(binPath)) {
      throw new Error(`Missing ${arch} binary required for universal bundle: ${binPath}`);
    }
  }

  mkdirSync(bins.universalDir, { recursive: true });
  runOrExit("lipo", ["-create", bins.arm64, bins.x86_64, "-output", bins.universal], repoRoot);
  chmodSync(bins.universal, 0o755);

  runOrExit(
    pnpmCmd,
    ["exec", "tauri", "bundle", "--target", UNIVERSAL_TARGET, "--bundles", bundles, ...passthrough],
    repoRoot,
  );
}

const isEntrypoint =
  process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isEntrypoint) {
  try {
    buildMacosUniversal();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
