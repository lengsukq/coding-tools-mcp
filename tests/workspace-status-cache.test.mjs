import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";
import { computed } from "vue";

async function loadCacheWithMocks() {
  const asyncResourceSrc = await readFile(new URL("../src/lib/async-resource.ts", import.meta.url), "utf8");
  const cacheSrc = await readFile(new URL("../src/lib/workspace-status-cache.ts", import.meta.url), "utf8");

  const asyncResourceJs = ts.transpileModule(asyncResourceSrc, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const asyncResourceUrl = `data:text/javascript;base64,${Buffer.from(asyncResourceJs).toString("base64")}`;

  const planningStub = `export async function getPlanningState(id) { return { mode: "direct", focus_goal_id: null, focus_plan_id: null, goals: [], plans: [], execution: { state: "idle", plan_id: null, goal_id: null, step_id: null, last_error: null, verification: [] }, workspaceId: id }; }`;
  const workspacesStub = `
    export async function detectInstalledIdes() { return []; }
    export async function getGlobalMcpOverview() { return { state: "running" }; }
    export async function getWorkspaceActivityMetrics(id) { return { id }; }
    export async function getWorkspaceGitSummary(id) { return { available: true, branch: "main-" + id, changedFiles: 0, subRepositories: [] }; }
    export async function listWorkspaceReviews() { return []; }
  `;

  const planningUrl = `data:text/javascript;base64,${Buffer.from(planningStub).toString("base64")}`;
  const workspacesUrl = `data:text/javascript;base64,${Buffer.from(workspacesStub).toString("base64")}`;

  const cacheJs = ts.transpileModule(cacheSrc, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  }).outputText
    .replace(/from\s+"vue"/g, `from ${JSON.stringify(import.meta.resolve("vue"))}`)
    .replace(/from\s+"\.\/async-resource"/g, `from ${JSON.stringify(asyncResourceUrl)}`)
    .replace(/from\s+"\$lib\/api\/planning"/g, `from ${JSON.stringify(planningUrl)}`)
    .replace(/from\s+"\$lib\/api\/workspaces"/g, `from ${JSON.stringify(workspacesUrl)}`);

  return import(`data:text/javascript;base64,${Buffer.from(cacheJs).toString("base64")}`);
}

test("workspace-status-cache isolates reactivity per workspace key and keeps peek read-only", async () => {
  const cache = await loadCacheWithMocks();
  assert.equal(cache.WORKSPACE_STATUS_TTL_MS.git, 15_000);
  assert.equal(cache.WORKSPACE_STATUS_TTL_MS.planning, 6_000);

  let alphaRuns = 0;
  let betaRuns = 0;
  const alphaPlanning = computed(() => {
    alphaRuns += 1;
    return cache.peekCachedPlanningState("ws-alpha");
  });
  const betaPlanning = computed(() => {
    betaRuns += 1;
    return cache.peekCachedPlanningState("ws-beta");
  });

  // Initial read-only peeks return null without interfering across keys
  assert.equal(alphaPlanning.value, null);
  assert.equal(betaPlanning.value, null);
  assert.equal(alphaRuns, 1);
  assert.equal(betaRuns, 1);

  // Updating ws-alpha only recomputes ws-alpha's computed, never ws-beta's
  cache.setCachedPlanningState("ws-alpha", { mode: "plan", id: "alpha-v1" });
  assert.equal(alphaPlanning.value?.mode, "plan");
  assert.equal(betaPlanning.value, null);
  assert.equal(alphaRuns, 2);
  assert.equal(betaRuns, 1);

  // Subsequent updates to ws-alpha still do not touch ws-beta
  cache.setCachedPlanningState("ws-alpha", { mode: "goal", id: "alpha-v2" });
  assert.equal(alphaPlanning.value?.mode, "goal");
  assert.equal(betaPlanning.value, null);
  assert.equal(alphaRuns, 3);
  assert.equal(betaRuns, 1);

  // Fetching git summary for ws-alpha does not recompute planning for ws-alpha or ws-beta
  await cache.getCachedWorkspaceGitSummary("ws-alpha");
  assert.equal(alphaPlanning.value?.mode, "goal");
  assert.equal(betaPlanning.value, null);
  assert.equal(alphaRuns, 3);
  assert.equal(betaRuns, 1);

  // Evicting ws-alpha resets only ws-alpha
  cache.evictWorkspaceStatus("ws-alpha");
  assert.equal(alphaPlanning.value, null);
  assert.equal(betaPlanning.value, null);
  assert.equal(alphaRuns, 4);
  assert.equal(betaRuns, 1);
});
