import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL("../" + path, import.meta.url), "utf8");

test("audit console delegates data ownership to one composable", async () => {
  const page = await source("src/views/ToolAuditView.vue");
  const store = await source("src/lib/audit/use-tool-audit.ts");
  assert.match(page, /useToolAudit\(\)/);
  assert.match(store, /export function useToolAudit\(\)/);
  assert.match(store, /listToolAuditRecords\(buildFilter\(nextOffset\)\)/);
  assert.match(store, /sequence !== loadSequence/);
  assert.match(store, /await clearToolAuditRecords/);
  assert.doesNotMatch(page, /listToolAuditRecords\(/);
});

test("Global MCP routing isolates scope resolution from tool dispatch", async () => {
  const server = await source("src-tauri/src/mcp/server.rs");
  const dispatch = await source("src-tauri/src/mcp/server/dispatch.rs");
  const scope = await source("src-tauri/src/mcp/server/scope.rs");
  assert.match(server, /pub use dispatch::handle_gateway_request;/);
  assert.match(server, /pub use scope::audit_workspace_hint;/);
  assert.match(dispatch, /fn handle_gateway_tools_call\(/);
  assert.match(dispatch, /fn handle_workspace_tool\(/);
  assert.match(dispatch, /attach_review_session\(&request, &structured/);
  assert.match(scope, /fn remove_workspace_scope\(/);
  assert.match(scope, /INVALID_WORKSPACE_SCOPE/);
  assert.match(scope, /WORKSPACE_NOT_SELECTED/);
});

test("Computer Use native FFI is isolated from authorization and policy state", async () => {
  const computer = await source("src-tauri/src/computer.rs");
  const native = await source("src-tauri/src/computer/native.rs");
  const tests = await source("src-tauri/src/computer/tests.rs");
  assert.match(computer, /mod native;/);
  assert.match(computer, /fn prepare_or_request_sensitive_action/);
  assert.doesNotMatch(computer, /fn computer_native_(?:capture|action|windows|free)/);
  assert.match(native, /unsafe extern "C"/);
  assert.match(native, /computer_native_free\(pointer\)/);
  assert.match(tests, /high_risk_confirmation_is_local_exact_and_single_use/);
});

test("audit rows use typed output events rather than mutating parent filters", async () => {
  const page = await source("src/views/ToolAuditView.vue");
  const row = await source("src/components/audit/AuditRecordRow.vue");
  assert.match(page, /<AuditRecordRow/);
  assert.match(page, /@filter-session="quickFilterBySession"/);
  assert.match(row, /defineEmits</);
  assert.match(row, /"copy-text": \[text: string, key: string, label: string\]/);
  assert.match(row, /emit\('toggle'\)/);
  assert.doesNotMatch(row, /listToolAuditRecords|clearToolAuditRecords/);
});

test("workspace commands keep stable exports but split independent domains", async () => {
  const root = await source("src-tauri/src/commands/workspace.rs");
  assert.match(root, /mod crud;/);
  assert.match(root, /mod git;/);
  assert.match(root, /mod ide;/);
  assert.match(root, /mod reviews;/);
  assert.match(await source("src-tauri/src/commands/workspace/git.rs"), /pub async fn get_workspace_git_summary/);
  assert.match(await source("src-tauri/src/commands/workspace/crud.rs"), /pub fn delete_workspace/);
  assert.match(await source("src-tauri/src/commands/workspace/reviews.rs"), /pub fn list_workspace_reviews/);
});
