import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../", import.meta.url));

async function collectFiles(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await collectFiles(full)));
    } else if (entry.name.endsWith(".ts") || entry.name.endsWith(".vue")) {
      files.push(full);
    }
  }
  return files;
}

test("frontend Tauri invoke calls and Rust generate_handler! commands stay in strict 1:1 sync", async () => {
  const libRs = await readFile(join(repoRoot, "src-tauri/src/lib.rs"), "utf8");
  const handlerMatch = libRs.match(/tauri::generate_handler!\[([\s\S]*?)\]/);
  assert.ok(handlerMatch, "tauri::generate_handler![...] block must exist in src-tauri/src/lib.rs");

  const registeredCommands = new Set(
    handlerMatch[1]
      .replace(/\/\/[^\n]*/g, "")
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean),
  );
  assert.ok(registeredCommands.size > 0, "at least one Tauri command should be registered");

  const frontendFiles = await collectFiles(join(repoRoot, "src"));
  const invokedCommands = new Set();
  const invokePattern = /\binvoke(?:<[^()]*>)?\(\s*["']([a-z0-9_]+)["']/g;

  for (const file of frontendFiles) {
    const content = await readFile(file, "utf8");
    for (const match of content.matchAll(invokePattern)) {
      invokedCommands.add(match[1]);
    }
  }

  const missingInBackend = [...invokedCommands].filter((cmd) => !registeredCommands.has(cmd)).sort();
  const unusedInFrontend = [...registeredCommands].filter((cmd) => !invokedCommands.has(cmd)).sort();

  assert.deepEqual(
    missingInBackend,
    [],
    `Frontend invokes commands missing from tauri::generate_handler!: ${missingInBackend.join(", ")}`,
  );
  assert.deepEqual(
    unusedInFrontend,
    [],
    `Registered Tauri commands have no frontend invoke() caller: ${unusedInFrontend.join(", ")}`,
  );
});
