import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const MAX_MCP_REGISTRY_DESCRIPTION_LEN = 100;

function readJson(relativePath) {
  return JSON.parse(readFileSync(resolve(projectRoot, relativePath), "utf8"));
}

function readCargoPackageVersion(cargoToml) {
  const packageSection = cargoToml.match(/\[package\]([\s\S]*?)(?:\n\[|$)/)?.[1] ?? "";
  return packageSection.match(/^version\s*=\s*"([^"]+)"$/m)?.[1];
}

const packageJson = readJson("package.json");
const packageLockPath = resolve(projectRoot, "package-lock.json");
const packageLock = existsSync(packageLockPath) ? readJson("package-lock.json") : null;
const tauriConfig = readJson("src-tauri/tauri.conf.json");
const serverJson = readJson("server.json");
const cargoToml = readFileSync(resolve(projectRoot, "src-tauri/Cargo.toml"), "utf8");
const cargoLock = readFileSync(resolve(projectRoot, "src-tauri/Cargo.lock"), "utf8");
const projectContextMd = readFileSync(resolve(projectRoot, "docs/project-context.md"), "utf8");
const techStackMd = readFileSync(
  resolve(projectRoot, "docs/project-context/tech-stack.md"),
  "utf8",
);

const cargoTomlVersion = readCargoPackageVersion(cargoToml);
const cargoLockVersion = cargoLock.match(
  /\[\[package\]\]\nname = "coding-tools-mcp-desktop"\nversion = "([^"]+)"/,
)?.[1];
const projectContextVersion = projectContextMd.match(/^\|\s*版本\s*\|\s*`([^`]+)`\s*\|$/m)?.[1];
const techStackVersion = techStackMd.match(/^\|\s*应用版本\s*\|\s*`([^`]+)`/m)?.[1];

const versions = {
  "package.json": packageJson.version,
  ...(packageLock
    ? {
        "package-lock.json": packageLock.version,
        "package-lock.json#packages.": packageLock.packages?.[""]?.version,
      }
    : {}),
  "src-tauri/Cargo.toml": cargoTomlVersion,
  "src-tauri/Cargo.lock": cargoLockVersion,
  "src-tauri/tauri.conf.json": tauriConfig.version,
  "server.json": serverJson.version,
  "docs/project-context.md": projectContextVersion,
  "docs/project-context/tech-stack.md": techStackVersion,
};

if (Array.isArray(serverJson.packages)) {
  serverJson.packages.forEach((pkg, index) => {
    versions[`server.json#packages[${index}].version`] = pkg?.version;
  });
}

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

if (
  typeof serverJson.description !== "string" ||
  serverJson.description.length > MAX_MCP_REGISTRY_DESCRIPTION_LEN
) {
  console.error(
    `server.json description must be <= ${MAX_MCP_REGISTRY_DESCRIPTION_LEN} characters (got ${serverJson.description?.length ?? 0})`,
  );
  process.exit(1);
}

const releaseTag = process.env.RELEASE_TAG?.trim();
if (releaseTag && releaseTag !== `v${version}`) {
  console.error(`Release tag ${releaseTag} does not match project version v${version}`);
  process.exit(1);
}

console.log(`Project version ${version} is consistent${releaseTag ? ` with ${releaseTag}` : ""}`);
