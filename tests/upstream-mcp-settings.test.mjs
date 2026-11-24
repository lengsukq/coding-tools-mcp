import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

const source = (path) => readFile(new URL("../" + path, import.meta.url), "utf8");

test("Upstream MCP hub follows modular facade architecture across Rust, CLI, and Vue", async () => {
  const facade = await source("src-tauri/src/upstream_mcp.rs");
  assert.match(facade, /mod client;/);
  assert.match(facade, /pub mod import;/);
  assert.match(facade, /mod manager;/);
  assert.match(facade, /mod model;/);
  assert.match(facade, /pub mod naming;/);
  assert.match(facade, /pub mod proxy_tools;/);
  assert.match(facade, /mod transport_http;/);
  assert.match(facade, /mod transport_sse;/);
  assert.match(facade, /mod transport_stdio;/);

  const view = await source("src/views/settings/UpstreamMcpSettingsView.vue");
  const composable = await source("src/lib/upstream-mcp/use-upstream-mcp.ts");
  const listCard = await source("src/components/settings/UpstreamServerListCard.vue");
  const editorModal = await source("src/components/settings/UpstreamServerEditorModal.vue");
  const importModal = await source("src/components/settings/UpstreamImportModal.vue");
  const toolsModal = await source("src/components/settings/UpstreamToolsModal.vue");

  assert.match(view, /useUpstreamMcp\(\)/);
  assert.match(view, /<UpstreamServerListCard/);
  assert.match(view, /<UpstreamServerEditorModal/);
  assert.match(view, /<UpstreamImportModal/);
  assert.match(view, /<UpstreamToolsModal/);
  assert.doesNotMatch(view, /invoke\(|saveUpstreamMcpServer\(/);
  assert.match(composable, /export function useUpstreamMcp\(\)/);

  // Pure presentation subcomponents must never call Tauri IPC directly.
  for (const child of [listCard, editorModal, importModal, toolsModal]) {
    assert.doesNotMatch(child, /@tauri-apps\/api|invoke\(/);
  }
});

test("Upstream MCP security guards: anti-loop header, audit unwrapping, and headless IPC reload", async () => {
  const listener = await source("src-tauri/src/mcp/listener.rs");
  const toolAudit = await source("src-tauri/src/tool_audit.rs");
  const auditRow = await source("src/components/audit/AuditRecordRow.vue");
  const localIpc = await source("src-tauri/src/platform/local_ipc.rs");
  const cli = await source("src-tauri/src/cli.rs");

  assert.match(listener, /UPSTREAM_ORIGIN_HEADER/);
  assert.match(listener, /UPSTREAM_SELF_LOOP_BLOCKED/);
  assert.match(listener, /handle_gateway_upstream_tool_call_async/);
  assert.match(toolAudit, /TOOL_UPSTREAM_CALL/);
  assert.match(auditRow, /record\.executionHost\.startsWith\('upstream:'\)/);
  assert.match(auditRow, /上游 MCP →/);
  assert.match(localIpc, /ReloadUpstream/);
  assert.match(cli, /"upstream"/);
});

test("buildSavePayloadFromDraft isolates secret env/headers and respects bearer token clearing", async () => {
  const composableSrc = await source("src/lib/upstream-mcp/use-upstream-mcp.ts");
  const sliceStart = composableSrc.indexOf("export interface KeyValueDraftRow");
  const sliceEnd = composableSrc.indexOf("export function useUpstreamMcp()");
  assert.ok(sliceStart >= 0 && sliceEnd > sliceStart, "pure helper block exists");

  const pureTs = composableSrc
    .slice(sliceStart, sliceEnd)
    .replace(/UpstreamExposeMode/g, "string")
    .replace(/UpstreamServerConfig/g, "any")
    .replace(/UpstreamSecretPatch/g, "any");
  const jsCode = ts.transpileModule(pureTs, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const mod = await import(`data:text/javascript,${encodeURIComponent(jsCode)}`);

  const stdioDraft = mod.createEmptyUpstreamDraft();
  stdioDraft.alias = "github";
  stdioDraft.name = "GitHub";
  stdioDraft.transportType = "stdio";
  stdioDraft.command = "npx";
  stdioDraft.argsText = "-y\n@modelcontextprotocol/server-github\n";
  stdioDraft.originalSecretEnvKeys = ["OLD_KEY", "KEEP_KEY"];
  stdioDraft.envRows = [
    { key: "LOG_LEVEL", value: "info", secret: false },
    { key: "KEEP_KEY", value: "", secret: true, hasStoredSecret: true },
    { key: "NEW_TOKEN", value: "ghp_123", secret: true },
  ];

  const stdioOut = mod.buildSavePayloadFromDraft(stdioDraft);
  assert.deepEqual(stdioOut.server.transport.args, ["-y", "@modelcontextprotocol/server-github"]);
  assert.deepEqual(stdioOut.server.transport.env, { LOG_LEVEL: "info" });
  assert.deepEqual(stdioOut.server.transport.secretEnvKeys.sort(), ["KEEP_KEY", "NEW_TOKEN"]);
  assert.deepEqual(stdioOut.secretPatch.setEnvSecrets, { NEW_TOKEN: "ghp_123" });
  assert.deepEqual(stdioOut.secretPatch.removeEnvSecrets, ["OLD_KEY"]);

  const httpDraft = mod.createEmptyUpstreamDraft();
  httpDraft.alias = "docs";
  httpDraft.transportType = "streamable_http";
  httpDraft.url = "https://docs.example.com/mcp";
  httpDraft.hasStoredBearer = true;
  httpDraft.clearBearerToken = true;
  const httpOut = mod.buildSavePayloadFromDraft(httpDraft);
  assert.equal(httpOut.secretPatch.bearerToken, null);
  assert.equal(httpOut.server.transport.hasBearerToken, false);
});
