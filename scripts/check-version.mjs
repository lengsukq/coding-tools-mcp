import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function readJson(relativePath) {
  return JSON.parse(readFileSync(resolve(projectRoot, relativePath), "utf8"));
}

const packageJson = readJson("package.json");
const packageLock = readJson("package-lock.json");
const tauriConfig = readJson("src-tauri/tauri.conf.json");
const cargoToml = readFileSync(resolve(projectRoot, "src-tauri/Cargo.toml"), "utf8");
const cargoLock = readFileSync(resolve(projectRoot, "src-tauri/Cargo.lock"), "utf8");

const cargoTomlVersion = cargoToml.match(/^version = "([^"]+)"$/m)?.[1];
const cargoLockVersion = cargoLock.match(
  /\[\[package\]\]\nname = "coding-tools-mcp-desktop"\nversion = "([^"]+)"/,
)?.[1];

const versions = {
  "package.json": packageJson.version,
  "package-lock.json": packageLock.version,
  "package-lock.json#packages.": packageLock.packages?.[""]?.version,
  "src-tauri/Cargo.toml": cargoTomlVersion,
  "src-tauri/Cargo.lock": cargoLockVersion,
  "src-tauri/tauri.conf.json": tauriConfig.version,
};
const uniqueVersions = new Set(Object.values(versions));
const version = packageJson.version;
const mismatches = Object.entries(versions).filter(([, value]) => value !== version);

if (uniqueVersions.size !== 1 || mismatches.length > 0) {
  console.error("Project version sources are inconsistent:");
  for (const [source, value] of Object.entries(versions)) {
    console.error(`- ${source}: ${value ?? "missing"}`);
  }
  process.exit(1);
}

const releaseTag = process.env.RELEASE_TAG?.trim();
if (releaseTag && releaseTag !== `v${version}`) {
  console.error(`Release tag ${releaseTag} does not match project version v${version}`);
  process.exit(1);
}

console.log(`Project version ${version} is consistent${releaseTag ? ` with ${releaseTag}` : ""}`);
