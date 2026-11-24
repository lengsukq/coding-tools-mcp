#!/usr/bin/env node
// Regenerates or verifies docs/ui/component-catalog.json directly from src/components/ui/*.vue.
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, posix, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const uiDir = resolve(projectRoot, "src/components/ui");
const srcDir = resolve(projectRoot, "src");
const catalogPath = resolve(projectRoot, "docs/ui/component-catalog.json");
const checkOnly = process.argv.includes("--check");

function walkFiles(dir) {
  const results = [];
  for (const entry of readdirSync(dir)) {
    const full = resolve(dir, entry);
    const info = statSync(full);
    if (info.isDirectory()) {
      results.push(...walkFiles(full));
    } else if (entry.endsWith(".vue") || entry.endsWith(".ts")) {
      results.push(full);
    }
  }
  return results;
}

function toPosixRelative(absPath) {
  return relative(projectRoot, absPath).split("\\").join(posix.sep);
}

const uiComponents = readdirSync(uiDir)
  .filter((name) => name.endsWith(".vue"))
  .map((file) => ({
    name: file.slice(0, -".vue".length),
    path: `src/components/ui/${file}`,
  }))
  .sort((a, b) => a.name.localeCompare(b.name));

const sourceFiles = walkFiles(srcDir)
  .map((absPath) => ({
    relPath: toPosixRelative(absPath),
    content: readFileSync(absPath, "utf8"),
  }))
  .sort((a, b) => a.relPath.localeCompare(b.relPath));

const components = uiComponents.map(({ name, path }) => {
  const importPattern = new RegExp(`["'][^"']*components/ui/${name}\\.vue["']|["']\\./${name}\\.vue["']`);
  const consumers = sourceFiles
    .filter(({ relPath, content }) => relPath !== path && importPattern.test(content))
    .map(({ relPath }) => relPath);
  return {
    name,
    path,
    consumers,
    count: consumers.length,
  };
});

const rendered = `${JSON.stringify({ components, developmentGallery: "/dev/ui" }, null, 2)}\n`;

if (checkOnly) {
  const existing = readFileSync(catalogPath, "utf8");
  if (existing !== rendered) {
    console.error(
      "docs/ui/component-catalog.json is out of sync with src/components/ui/. Run `npm run catalog:sync`.",
    );
    process.exit(1);
  }
  console.log(`UI component catalog is in sync (${components.length} components)`);
} else {
  writeFileSync(catalogPath, rendered);
  console.log(`Synchronized docs/ui/component-catalog.json (${components.length} components)`);
}
