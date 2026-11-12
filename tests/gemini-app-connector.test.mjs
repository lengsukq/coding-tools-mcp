import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

const source = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

const helperSource = await source("src/lib/gemini-connector.ts");
const compiled = ts.transpileModule(helperSource, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const { assessGeminiConnection, validGeminiMcpEndpoint, GEMINI_DEVELOPER_MODE_URL } =
  await import(`data:text/javascript,${encodeURIComponent(compiled)}`);

function input(overrides = {}) {
  return {
    endpoint: "https://mcp.example.com/mcp",
    publicEnabled: true,
    mcpRunning: true,
    authType: "oauth",
    publicHealth: "passed",
    configDirty: false,
    ...overrides,
  };
}

test("Gemini endpoint only allows public HTTPS /mcp without secrets or fragments", () => {
  assert.equal(validGeminiMcpEndpoint("https://mcp.example.com/mcp"), true);
  for (const endpoint of [
    "http://mcp.example.com/mcp",
    "https://localhost/mcp",
    "https://127.0.0.1/mcp",
    "https://10.0.0.1/mcp",
    "https://172.16.2.2/mcp",
    "https://192.168.1.1/mcp",
    "https://nas.local/mcp",
    "https://user:password@mcp.example.com/mcp",
    "https://mcp.example.com/mcp?access_token=secret",
    "https://mcp.example.com/mcp#secret",
    "https://mcp.example.com/other",
    "尚未配置",
  ]) {
    assert.equal(validGeminiMcpEndpoint(endpoint), false, endpoint);
  }
});

test("Gemini ready is a preflight result requiring OAuth, running MCP and verified public endpoint", () => {
  assert.equal(assessGeminiConnection(input()).ready, true);
  assert.equal(assessGeminiConnection(input({ authType: "bearer" })).ready, false);
  assert.equal(assessGeminiConnection(input({ publicEnabled: false })).ready, false);
  assert.equal(assessGeminiConnection(input({ mcpRunning: false })).ready, false);
  assert.equal(assessGeminiConnection(input({ publicHealth: "unchecked" })).ready, false);
  assert.equal(assessGeminiConnection(input({ publicHealth: "failed" })).ready, false);
  assert.equal(assessGeminiConnection(input({ configDirty: true })).canCopy, false);
  assert.equal(assessGeminiConnection(input({ endpoint: "http://127.0.0.1/mcp" })).canCopy, false);
  assert.match(assessGeminiConnection(input({ authType: "bearer" })).issues.join(" "), /OAuth/);
  assert.equal(GEMINI_DEVELOPER_MODE_URL, "https://gemini.google.com/devmode");
});

test("Gemini UI derives connection state from global gateway and auth, without copying secrets", async () => {
  const view = await source("src/views/settings/GatewaySettingsView.vue");
  const panel = await source("src/components/settings/GeminiAppConnector.vue");
  assert.match(view, /<GeminiAppConnector/);
  assert.match(view, /getGlobalRuntimeSettings/);
  assert.match(view, /:public-health="geminiPublicHealth"/);
  assert.match(view, /:config-dirty="dirty"/);
  assert.match(panel, /assessGeminiConnection/);
  assert.match(panel, /navigator\.clipboard\.writeText\(props\.endpoint\)/);
  assert.match(panel, /账号已完成授权/);
  assert.match(panel, /Connected Apps/);
  assert.doesNotMatch(panel, /getSharedSecret|oauth_client_secret|bearer_token/);
});

test("Gemini docs describe OAuth DCR, eligibility and web-to-mobile connection", async () => {
  const docs = await source("docs/web-chat-connectors.md");
  assert.match(docs, /Custom apps/);
  assert.match(docs, /DCR/);
  assert.match(docs, /Keep Activity/);
  assert.match(docs, /Gemini 移动 App/);
  assert.match(docs, /https:\/\/support\.google\.com\/gemini\/answer\/17209137/);
});
