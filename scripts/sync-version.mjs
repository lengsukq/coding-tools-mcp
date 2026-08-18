import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];

if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("Usage: npm run version:set -- <major.minor.patch>");
  process.exit(1);
}

function readJson(relativePath) {
  return JSON.parse(readFileSync(resolve(projectRoot, relativePath), "utf8"));
}

function writeJson(relativePath, value) {
  writeFileSync(resolve(projectRoot, relativePath), `${JSON.stringify(value, null, 2)}\n`);
}

const packageJson = readJson("package.json");
packageJson.version = version;
writeJson("package.json", packageJson);

const packageLock = readJson("package-lock.json");
packageLock.version = version;
packageLock.packages[""].version = version;
writeJson("package-lock.json", packageLock);

const tauriConfig = readJson("src-tauri/tauri.conf.json");
tauriConfig.version = version;
writeJson("src-tauri/tauri.conf.json", tauriConfig);

const cargoTomlPath = resolve(projectRoot, "src-tauri/Cargo.toml");
const cargoToml = readFileSync(cargoTomlPath, "utf8");
const updatedCargoToml = cargoToml.replace(/^(version = ")[^"]+("$)/m, `$1${version}$2`);
if (updatedCargoToml === cargoToml) {
  throw new Error("Could not find the package version in src-tauri/Cargo.toml");
}
writeFileSync(cargoTomlPath, updatedCargoToml);

const cargoLockPath = resolve(projectRoot, "src-tauri/Cargo.lock");
const cargoLock = readFileSync(cargoLockPath, "utf8");
const updatedCargoLock = cargoLock.replace(
  /(\[\[package\]\]\nname = "coding-tools-mcp-desktop"\nversion = ")[^"]+("\n)/,
  `$1${version}$2`,
);
if (updatedCargoLock === cargoLock) {
  throw new Error("Could not find the package version in src-tauri/Cargo.lock");
}
writeFileSync(cargoLockPath, updatedCargoLock);

console.log(`Synchronized project version to ${version}`);
