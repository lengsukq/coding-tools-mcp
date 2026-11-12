import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL("../" + path, import.meta.url), "utf8");

test("history keeps the session catalog visible if review snapshots fail", async () => {
  const panel = await source("src/components/workspace/HistoryContextPanel.vue");
  assert.match(panel, /Promise\.allSettled/);
  assert.match(panel, /listHistorySessions\(workspaceId\)/);
  assert.match(panel, /listWorkspaceReviews\(workspaceId\)/);
  assert.match(panel, /getPlanningState\(workspaceId\)/);
  assert.match(panel, /history\.status === "fulfilled"/);
  assert.match(panel, /reviewError\.value/);
  assert.match(panel, /generation !== loadGeneration/);
  assert.match(panel, /任务与计划历史/);
  assert.match(panel, /openTask\(task\.id\)/);
});

test("planning notes are collapsible without hover overlays and layout stacks in narrow workspaces", async () => {
  const planning = await source("src/components/workspace/WorkspacePlanning.vue");
  assert.match(planning, /<details v-if="step\.notes"/);
  assert.match(planning, /@container workspace-page \(max-width: 800px\)/);
  assert.match(planning, /grid-template-columns: minmax\(0, 1fr\)/);
  assert.doesNotMatch(planning, /hoveredStepNote|hover:scale|<Transition name="ui-tab-fade">/);
});

test("Global MCP asks for history checkpoints and preserves compact conversation identifiers", async () => {
  const server = await source("src-tauri/src/mcp/server.rs");
  const dispatch = await source("src-tauri/src/mcp/server/dispatch.rs");
  assert.match(dispatch, /history_recording=true/);
  assert.match(dispatch, /action=checkpoint at the end of every user-requested development task/);
  assert.match(server, /name == "history_manage"/);
  assert.match(dispatch, /canonical_name == "history_manage"/);
  const tests = await source("src-tauri/src/mcp/server/tests.rs");
  assert.match(tests, /compact_history\["_host_session_key"\]/);
});
