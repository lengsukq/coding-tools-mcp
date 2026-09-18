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

interface CacheEntry<T> {
  value: T | null;
  updatedAt: number;
  inFlight: Promise<T> | null;
}

const gitCache = new Map<string, CacheEntry<WorkspaceGitSummaryDto>>();
const planningCache = new Map<string, CacheEntry<PlanningStateDto>>();
const activityCache = new Map<string, CacheEntry<WorkspaceActivityMetricsDto>>();
const reviewsCache = new Map<string, CacheEntry<WorkspaceReviewSummaryDto[]>>();
const mcpCache = new Map<string, CacheEntry<GlobalMcpOverviewDto>>();
const idesCache = new Map<string, CacheEntry<DetectedIdeDto[]>>();

const DEFAULT_TTL_MS = 3_000;
const IDE_TTL_MS = 60_000;
const GLOBAL_KEY = "global";

function peekCached<T>(cache: Map<string, CacheEntry<T>>, key: string): T | null {
  return cache.get(key)?.value ?? null;
}

function getCached<T>(
  cache: Map<string, CacheEntry<T>>,
  key: string,
  loader: () => Promise<T>,
  force = false,
  ttlMs = DEFAULT_TTL_MS,
): Promise<T> {
  const now = Date.now();
  const current = cache.get(key);
  if (!force && current?.value && now - current.updatedAt <= ttlMs) {
    return Promise.resolve(current.value);
  }
  if (current?.inFlight) return current.inFlight;

  const entry: CacheEntry<T> = current ?? { value: null, updatedAt: 0, inFlight: null };
  const request = loader()
    .then((value) => {
      entry.value = value;
      entry.updatedAt = Date.now();
      return value;
    })
    .finally(() => {
      entry.inFlight = null;
    });
  entry.inFlight = request;
  cache.set(key, entry);
  return request;
}

export function getCachedWorkspaceGitSummary(workspaceId: string, force = false) {
  return getCached(gitCache, workspaceId, () => getWorkspaceGitSummary(workspaceId), force);
}

export function peekCachedWorkspaceGitSummary(workspaceId: string) {
  return peekCached(gitCache, workspaceId);
}

export function getCachedPlanningState(workspaceId: string, force = false) {
  return getCached(planningCache, workspaceId, () => getPlanningState(workspaceId), force);
}

export function peekCachedPlanningState(workspaceId: string) {
  return peekCached(planningCache, workspaceId);
}

export function getCachedWorkspaceActivityMetrics(workspaceId: string, force = false) {
  return getCached(activityCache, workspaceId, () => getWorkspaceActivityMetrics(workspaceId), force);
}

export function peekCachedWorkspaceActivityMetrics(workspaceId: string) {
  return peekCached(activityCache, workspaceId);
}

export function getCachedWorkspaceReviews(workspaceId: string, force = false) {
  return getCached(reviewsCache, workspaceId, () => listWorkspaceReviews(workspaceId), force);
}

export function peekCachedWorkspaceReviews(workspaceId: string) {
  return peekCached(reviewsCache, workspaceId);
}

export function setCachedWorkspaceReviews(workspaceId: string, value: WorkspaceReviewSummaryDto[]) {
  reviewsCache.set(workspaceId, { value, updatedAt: Date.now(), inFlight: null });
}

export function getCachedGlobalMcpOverview(force = false) {
  return getCached(mcpCache, GLOBAL_KEY, getGlobalMcpOverview, force);
}

export function peekCachedGlobalMcpOverview() {
  return peekCached(mcpCache, GLOBAL_KEY);
}

export function getCachedDetectedIdes(force = false) {
  return getCached(idesCache, GLOBAL_KEY, detectInstalledIdes, force, IDE_TTL_MS);
}

export function peekCachedDetectedIdes() {
  return peekCached(idesCache, GLOBAL_KEY);
}

export function invalidateWorkspaceStatus(workspaceId: string) {
  gitCache.delete(workspaceId);
  planningCache.delete(workspaceId);
  activityCache.delete(workspaceId);
  reviewsCache.delete(workspaceId);
}
