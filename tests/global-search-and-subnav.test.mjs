import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

const source = (path) => readFile(new URL("../" + path, import.meta.url), "utf8");

async function loadTsModule(relPath) {
  const raw = await source(relPath);
  const code = ts.transpileModule(raw, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  return import("data:text/javascript;base64," + Buffer.from(code).toString("base64"));
}

test("settings and workspace views split mega-pages and use left side navigation rails instead of top PageTabs", async () => {
  const settingsView = await source("src/views/settings/SettingsView.vue");
  const workspaceView = await source("src/views/WorkspaceView.vue");
  const dashboardView = await source("src/views/DashboardView.vue");
  const appRoot = await source("src/App.vue");
  const appShell = await source("src/components/AppShell.vue");

  assert.match(settingsView, /ui-subnav-rail/);
  assert.doesNotMatch(settingsView, /<PageTabs/);

  assert.match(workspaceView, /ui-subnav-rail/);
  assert.doesNotMatch(workspaceView, /<PageTabs/);

  assert.match(dashboardView, /ui-subnav-rail/);
  assert.match(appRoot, /<GlobalSearchDialog/);
  assert.match(appShell, /全局搜索设置与项目/);
});

test("global search indexes all split settings subpages, workspaces, workspace tabs, and actions", async () => {
  const {
    SETTINGS_SIDEBAR_GROUPS,
    WORKSPACE_SIDEBAR_GROUPS,
    resolveActiveSettingsItemId,
    buildGlobalSearchEntries,
    filterGlobalSearchEntries,
  } = await loadTsModule("src/lib/global-search.ts");

  const allSettingsItems = SETTINGS_SIDEBAR_GROUPS.flatMap((g) => g.items);
  assert.equal(allSettingsItems.length, 15);
  assert.equal(new Set(allSettingsItems.map((i) => i.id)).size, 15);
  assert.equal(WORKSPACE_SIDEBAR_GROUPS.flatMap((g) => g.items).length, 8);

  assert.equal(resolveActiveSettingsItemId("connection", "health"), "gateway");
  assert.equal(resolveActiveSettingsItemId("connection", "gemini-connector"), "gemini");
  assert.equal(resolveActiveSettingsItemId("connection", "auth-scope"), "auth");
  assert.equal(resolveActiveSettingsItemId("connection", "lan-peers"), "lan-peers");
  assert.equal(resolveActiveSettingsItemId("execution", "context"), "context");
  assert.equal(resolveActiveSettingsItemId("computer", "audit"), "computer-audit");
  assert.equal(resolveActiveSettingsItemId("tools", "software"), "software");

  const entries = buildGlobalSearchEntries(
    [
      {
        id: "ws-alpha",
        name: "alpha-service",
        path: "/Users/leo/code/alpha-service",
        runtime: {},
      },
    ],
    { canCompact: true },
  );

  const geminiHits = filterGlobalSearchEntries(entries, "gemini stdio");
  assert.equal(geminiHits[0]?.id, "settings-gemini");

  const lanHits = filterGlobalSearchEntries(entries, "easytier 配对码");
  assert.equal(lanHits[0]?.id, "settings-lan");

  const nameHits = filterGlobalSearchEntries(entries, "允许局域网访问");
  assert.equal(nameHits[0]?.id, "settings-auth");

  const wsDiffHits = filterGlobalSearchEntries(entries, "alpha diff");
  assert.equal(wsDiffHits[0]?.id, "ws-ws-alpha-changes");
});

test("secondary navigation rail stays side-by-side at default 1280px and minimum 960px desktop widths", async () => {
  const css = await source("src/styles/design-system.css");
  // Default 1280px window minus primary sidebar leaves ~980-1010px app-main, and minWidth 960px leaves ~696px.
  // Ensure subnav never collapses to a single full-width column above 639px app-main.
  assert.doesNotMatch(
    css,
    /@container\s+app-main\s*\(max-width:\s*(?:[7-9]\d\d|1\d\d\d)px\)\s*\{[^}]*\.ui-subnav-layout\s*\{[^}]*grid-template-columns:\s*1fr/s,
  );
  assert.match(css, /@container\s+app-main\s*\(max-width:\s*639px\)[\s\S]*?\.ui-subnav-layout\s*\{[\s\S]*?grid-template-columns:\s*1fr/);
});
