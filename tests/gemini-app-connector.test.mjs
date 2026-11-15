import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

const source = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

const helperSource = await source("src/lib/gemini-connector.ts");
const compiled = ts.transpileModule(helperSource, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const {
  assessGeminiConnection, assessGeminiLocalConnection,
  validGeminiMcpEndpoint, validGeminiLocalEndpoint, geminiLocalCommand,
  GEMINI_DEVELOPER_MODE_URL, GEMINI_LOCAL_DEFAULT_BIN, GEMINI_LOCAL_AUTH_FILE,
} =
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

test("Gemini Local MCP only generates a loopback stdio bridge command with a safe URL", () => {
  assert.equal(validGeminiLocalEndpoint("http://127.0.0.1:28765/mcp"), true);
  assert.equal(validGeminiLocalEndpoint("http://localhost:28765/mcp"), true);
  assert.equal(geminiLocalCommand("http://localhost:28765/mcp"),
    "~/.local/bin/coding-tools-mcp mcp-stdio http://127.0.0.1:28765/mcp");
  assert.equal(geminiLocalCommand("http://127.0.0.1:28765/mcp", "/Users/leo/Library Application/bin"),
    "\"/Users/leo/Library Application/bin\" mcp-stdio http://127.0.0.1:28765/mcp");
  assert.equal(GEMINI_LOCAL_DEFAULT_BIN, "~/.local/bin/coding-tools-mcp");
  assert.equal(GEMINI_LOCAL_AUTH_FILE, "~/.local/share/coding-tools-mcp/gemini-stdio-auth.json");
  for (const endpoint of [
    "http://0.0.0.0:28765/mcp",
    "http://10.0.0.5:28765/mcp",
    "http://127.0.0.2:28765/mcp",
    "https://127.0.0.1:28765/mcp",
    "http://127.0.0.1/mcp",
    "http://127.0.0.1:28765/other",
    "http://user:secret@127.0.0.1:28765/mcp",
    "http://127.0.0.1:28765/mcp?token=x",
    "http://127.0.0.1:28765/mcp#x",
    "http://127.0.0.1:28765/mcp --debug",
    "not a url",
  ]) {
    assert.equal(validGeminiLocalEndpoint(endpoint), false, endpoint);
    assert.equal(geminiLocalCommand(endpoint), null, endpoint);
  }
});

test("Local MCP is independent of public tunnel, tracks native bridge preparation, and blocks bearer secrets", () => {
  const local = { localEndpoint: "http://127.0.0.1:28765/mcp", mcpRunning: true, authType: "oauth" };
  assert.equal(assessGeminiLocalConnection(local).ready, true);
  assert.equal(assessGeminiLocalConnection(local).canCopy, true);
  assert.equal(assessGeminiLocalConnection({ ...local, binaryReady: false }).ready, false);
  assert.equal(assessGeminiLocalConnection({ ...local, binaryReady: false }).needsPrepare, true);
  assert.equal(assessGeminiLocalConnection({ ...local, credentialsReady: false }).ready, false);
  assert.equal(assessGeminiLocalConnection({ ...local, mcpRunning: false }).ready, false);
  assert.equal(assessGeminiLocalConnection({ ...local, authType: "noauth" }).ready, true);
  assert.equal(assessGeminiLocalConnection({ ...local, authType: "bearer" }).canCopy, false);
  assert.match(assessGeminiLocalConnection({ ...local, authType: "bearer" }).issues.join(" "), /Token|Bearer/);
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
  assert.match(view, /:local-endpoint="localEndpoint"/);
  assert.match(view, /getGlobalRuntimeSettings/);
  assert.match(view, /:public-health="geminiPublicHealth"/);
  assert.match(view, /:config-dirty="dirty"/);
  assert.match(panel, /assessGeminiConnection/);
  assert.match(panel, /assessGeminiLocalConnection/);
  assert.match(panel, /prepareGeminiLocalBridge/);
  assert.match(panel, /Local MCP/);
  assert.match(panel, /copyLocalCommand/);
  assert.match(panel, /mcp-stdio/);
  assert.match(panel, /navigator\.clipboard\.writeText\(props\.endpoint\)/);
  assert.match(panel, /账号已完成授权/);
  assert.match(panel, /Connected Apps/);
  assert.doesNotMatch(panel, /getSharedSecret|oauth_client_secret|bearer_token/);
});

test("Gemini docs describe native stdio bridge, OAuth DCR, eligibility and web-to-mobile connection", async () => {
  const docs = await source("docs/web-chat-connectors.md");
  assert.match(docs, /Custom apps/);
  assert.match(docs, /DCR/);
  assert.match(docs, /Keep Activity/);
  assert.match(docs, /Gemini 移动 App/);
  assert.match(docs, /mcp-stdio/);
  assert.match(docs, /\.local\/bin\/coding-tools-mcp/);
  assert.match(docs, /gemini-stdio-auth\.json/);
  assert.match(docs, /Local MCP/);
  assert.match(docs, /https:\/\/support\.google\.com\/gemini\/answer\/17209137/);
});
