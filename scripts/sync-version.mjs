import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];

if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("Usage: pnpm run version:set -- <major.minor.patch>");
  process.exit(1);
}

function readJson(relativePath) {
  return JSON.parse(readFileSync(resolve(projectRoot, relativePath), "utf8"));
}

function writeJson(relativePath, value) {
  writeFileSync(resolve(projectRoot, relativePath), `${JSON.stringify(value, null, 2)}\n`);
}

function replaceOrThrow(content, pattern, replacement, label) {
  const updated = content.replace(pattern, replacement);
  if (updated === content) {
    throw new Error(`Could not find ${label}`);
  }
  return updated;
}

const packageJson = readJson("package.json");
packageJson.version = version;
writeJson("package.json", packageJson);

if (existsSync(resolve(projectRoot, "package-lock.json"))) {
  const packageLock = readJson("package-lock.json");
  packageLock.version = version;
  packageLock.packages[""].version = version;
  writeJson("package-lock.json", packageLock);
}

const tauriConfig = readJson("src-tauri/tauri.conf.json");
tauriConfig.version = version;
writeJson("src-tauri/tauri.conf.json", tauriConfig);

const serverJson = readJson("server.json");
serverJson.version = version;
if (Array.isArray(serverJson.packages)) {
  for (const pkg of serverJson.packages) {
    pkg.version = version;
  }
}
writeJson("server.json", serverJson);

const cargoTomlPath = resolve(projectRoot, "src-tauri/Cargo.toml");
const cargoToml = readFileSync(cargoTomlPath, "utf8");
const updatedCargoToml = replaceOrThrow(
  cargoToml,
  /(\[package\][\s\S]*?^version\s*=\s*")[^"]+("$)/m,
  `$1${version}$2`,
  "the [package] version in src-tauri/Cargo.toml",
);
writeFileSync(cargoTomlPath, updatedCargoToml);

const cargoLockPath = resolve(projectRoot, "src-tauri/Cargo.lock");
const cargoLock = readFileSync(cargoLockPath, "utf8");
const updatedCargoLock = replaceOrThrow(
  cargoLock,
  /(\[\[package\]\]\nname = "coding-tools-mcp-desktop"\nversion = ")[^"]+("\n)/,
  `$1${version}$2`,
  "the package version in src-tauri/Cargo.lock",
);
writeFileSync(cargoLockPath, updatedCargoLock);

const projectContextPath = resolve(projectRoot, "docs/project-context.md");
writeFileSync(
  projectContextPath,
  replaceOrThrow(
    readFileSync(projectContextPath, "utf8"),
    /^(\|\s*版本\s*\|\s*`)[^`]+(`\s*\|)$/m,
    `$1${version}$2`,
    "the version row in docs/project-context.md",
  ),
);

const techStackPath = resolve(projectRoot, "docs/project-context/tech-stack.md");
writeFileSync(
  techStackPath,
  replaceOrThrow(
    readFileSync(techStackPath, "utf8"),
    /^(\|\s*应用版本\s*\|\s*`)[^`]+(`)/m,
    `$1${version}$2`,
    "the version row in docs/project-context/tech-stack.md",
  ),
);

console.log(`Synchronized project version to ${version}`);
