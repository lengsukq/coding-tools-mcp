import { computed, shallowReactive, shallowRef, watch, type Ref, type ShallowRef } from "vue";
import { createResource, type ResourceState } from "./async-resource";
import { getPlanningState, type PlanningStateDto } from "$lib/api/planning";
import {
  detectInstalledIdes,
  getGlobalMcpOverview,
  getWorkspaceActivityMetrics,
  getWorkspaceGitSummary,
  listWorkspaceReviews,
  type DetectedIdeDto,
  type GlobalMcpOverviewDto,
  type WorkspaceActivityMetricsDto,
  type WorkspaceGitSummaryDto,
  type WorkspaceReviewSummaryDto,
} from "$lib/api/workspaces";

/**
 * Git status starts several OS processes per repository, so it uses a longer TTL
 * than lightweight planning/global reads.
 */
export const WORKSPACE_STATUS_TTL_MS = {
  git: 15_000,
  planning: 6_000,
  activity: 10_000,
  reviews: 6_000,
  global: 6_000,
  ides: 60_000,
} as const;

type WorkspaceResourceKind = "git" | "planning" | "activity" | "reviews";
const WORKSPACE_RESOURCE_KINDS: readonly WorkspaceResourceKind[] = ["git", "planning", "activity", "reviews"];

const EMPTY_RESOURCE_STATE = Object.freeze({
  value: null,
  updatedAt: 0,
  loading: false,
  error: null,
  stale: false,
});

const resources = new Map<string, ReturnType<typeof createResource<unknown>>>();
const slots = shallowReactive(new Map<string, ShallowRef<ResourceState<unknown>>>());

function ttlForKey(key: string): number {
  if (key.startsWith("git:")) return WORKSPACE_STATUS_TTL_MS.git;
  if (key.startsWith("activity:")) return WORKSPACE_STATUS_TTL_MS.activity;
  if (key.startsWith("reviews:")) return WORKSPACE_STATUS_TTL_MS.reviews;
  if (key === "ides") return WORKSPACE_STATUS_TTL_MS.ides;
  return WORKSPACE_STATUS_TTL_MS.planning;
}

function syncSlot(key: string, state: ResourceState<unknown>) {
  const snapshot: ResourceState<unknown> = { ...state };
  const existing = slots.get(key);
  if (existing) {
    existing.value = snapshot;
  } else {
    slots.set(key, shallowRef(snapshot));
  }
}

function ensureResource<T>(key: string, ttl = ttlForKey(key)): ReturnType<typeof createResource<T>> {
  let entry = resources.get(key);
  if (!entry) {
    entry = createResource<unknown>(ttl, () => {
      const current = resources.get(key);
      if (current) syncSlot(key, current.state);
    });
    resources.set(key, entry);
  }
  return entry as ReturnType<typeof createResource<T>>;
}

function peekValue<T>(key: string): T | null {
  const slot = slots.get(key);
  return (slot?.value.value as T | null) ?? null;
}

function readResourceState(key: string) {
  const slot = slots.get(key);
  if (!slot) return EMPTY_RESOURCE_STATE;
  const snapshot = slot.value;
  const entry = resources.get(key);
  return { ...snapshot, stale: entry ? entry.stale() : false };
}

export function workspaceResourceState(workspaceId: string, kind: WorkspaceResourceKind = "planning") {
  return readResourceState(`${kind}:${workspaceId}`);
}

export function getCachedWorkspaceGitSummary(id: string, force = false) {
  return ensureResource<WorkspaceGitSummaryDto>(`git:${id}`, WORKSPACE_STATUS_TTL_MS.git).get(
    () => getWorkspaceGitSummary(id),
    force,
  );
}

export function peekCachedWorkspaceGitSummary(id: string) {
  return peekValue<WorkspaceGitSummaryDto>(`git:${id}`);
}

export function getCachedPlanningState(id: string, force = false) {
  return ensureResource<PlanningStateDto>(`planning:${id}`, WORKSPACE_STATUS_TTL_MS.planning).get(
    () => getPlanningState(id),
    force,
  );
}

export function peekCachedPlanningState(id: string) {
  return peekValue<PlanningStateDto>(`planning:${id}`);
}

export function setCachedPlanningState(id: string, value: PlanningStateDto) {
  ensureResource<PlanningStateDto>(`planning:${id}`, WORKSPACE_STATUS_TTL_MS.planning).set(value);
}

export function getCachedWorkspaceActivityMetrics(id: string, force = false) {
  return ensureResource<WorkspaceActivityMetricsDto>(`activity:${id}`, WORKSPACE_STATUS_TTL_MS.activity).get(
    () => getWorkspaceActivityMetrics(id),
    force,
  );
}

export function peekCachedWorkspaceActivityMetrics(id: string) {
  return peekValue<WorkspaceActivityMetricsDto>(`activity:${id}`);
}

export function getCachedWorkspaceReviews(id: string, force = false) {
  return ensureResource<WorkspaceReviewSummaryDto[]>(`reviews:${id}`, WORKSPACE_STATUS_TTL_MS.reviews).get(
    () => listWorkspaceReviews(id),
    force,
  );
}

export function peekCachedWorkspaceReviews(id: string) {
  return peekValue<WorkspaceReviewSummaryDto[]>(`reviews:${id}`);
}

export function setCachedWorkspaceReviews(id: string, value: WorkspaceReviewSummaryDto[]) {
  ensureResource<WorkspaceReviewSummaryDto[]>(`reviews:${id}`, WORKSPACE_STATUS_TTL_MS.reviews).set(value);
}

export function getCachedGlobalMcpOverview(force = false) {
  return ensureResource<GlobalMcpOverviewDto>("global", WORKSPACE_STATUS_TTL_MS.global).get(
    getGlobalMcpOverview,
    force,
  );
}

export function globalResourceState() {
  return readResourceState("global");
}

export function peekCachedGlobalMcpOverview() {
  return peekValue<GlobalMcpOverviewDto>("global");
}

export function getCachedDetectedIdes(force = false) {
  return ensureResource<DetectedIdeDto[]>("ides", WORKSPACE_STATUS_TTL_MS.ides).get(detectInstalledIdes, force);
}

export function invalidateWorkspaceStatus(id: string) {
  for (const kind of WORKSPACE_RESOURCE_KINDS) {
    resources.get(`${kind}:${id}`)?.invalidate();
  }
}

export function evictWorkspaceStatus(id: string) {
  for (const kind of WORKSPACE_RESOURCE_KINDS) {
    const key = `${kind}:${id}`;
    resources.delete(key);
    slots.delete(key);
  }
}

/** Load on entry to a workspace tab. Never poll Git across inactive workspaces. */
export function useWorkspaceStatus(id: Ref<string> | (() => string), options: { git?: boolean; global?: boolean } = {}) {
  const read = typeof id === "function" ? id : () => id.value;
  watch(read, value => {
    if (!value) return;
    // Errors are captured into each resource's reactive state.error for UI display.
    void getCachedPlanningState(value).catch(() => undefined);
    if (options.git) void getCachedWorkspaceGitSummary(value).catch(() => undefined);
    if (options.global) void getCachedGlobalMcpOverview().catch(() => undefined);
  }, { immediate: true });
  return {
    planning: computed(() => peekCachedPlanningState(read())),
    git: computed(() => peekCachedWorkspaceGitSummary(read())),
    query: computed(() => workspaceResourceState(read())),
    gitQuery: computed(() => workspaceResourceState(read(), "git")),
    refresh: () =>
      Promise.allSettled([
        getCachedPlanningState(read(), true),
        ...(options.git ? [getCachedWorkspaceGitSummary(read(), true)] : []),
        ...(options.global ? [getCachedGlobalMcpOverview(true)] : []),
      ]),
  };
}
