import { computed, ref, watch, type Ref } from "vue";
import { createResource } from "./async-resource";
import { getPlanningState, type PlanningStateDto } from "$lib/api/planning";
import { detectInstalledIdes, getGlobalMcpOverview, getWorkspaceActivityMetrics, getWorkspaceGitSummary, listWorkspaceReviews, type DetectedIdeDto, type GlobalMcpOverviewDto, type WorkspaceActivityMetricsDto, type WorkspaceGitSummaryDto, type WorkspaceReviewSummaryDto, } from "$lib/api/workspaces";
const resources = new Map<string, { invalidate: () => void }>();
// Git status starts several OS processes per repository, so don't run the
// expensive scan at the same cadence as the lightweight planning read.
const GIT_SUMMARY_TTL_MS = 15_000;
export const statusRevision = ref(0);
function resource<T>(key: string, ttl = key.startsWith("git:") ? GIT_SUMMARY_TTL_MS : 6000) {
    if (!resources.has(key))
        resources.set(key, createResource<T>(ttl, () => { statusRevision.value++; }));
    return resources.get(key)! as ReturnType<typeof createResource<T>>;
}
export function workspaceResourceState(workspaceId: string, kind = "planning") {
    statusRevision.value;
    const r = resource(`${kind}:${workspaceId}`);
    return { ...r.state, stale: r.stale() };
}
export function getCachedWorkspaceGitSummary(id: string, force = false) { return resource<WorkspaceGitSummaryDto>(`git:${id}`, GIT_SUMMARY_TTL_MS).get(() => getWorkspaceGitSummary(id), force); }
export function peekCachedWorkspaceGitSummary(id: string) { statusRevision.value; return resource<WorkspaceGitSummaryDto>(`git:${id}`, GIT_SUMMARY_TTL_MS).state.value; }
export function getCachedPlanningState(id: string, force = false) { return resource<PlanningStateDto>(`planning:${id}`, 6000).get(() => getPlanningState(id), force); }
export function peekCachedPlanningState(id: string) { statusRevision.value; return resource<PlanningStateDto>(`planning:${id}`, 6000).state.value; }
export function setCachedPlanningState(id: string, value: PlanningStateDto) { resource<PlanningStateDto>(`planning:${id}`, 6000).set(value); }
export function getCachedWorkspaceActivityMetrics(id: string, force = false) { return resource<WorkspaceActivityMetricsDto>(`activity:${id}`, 10000).get(() => getWorkspaceActivityMetrics(id), force); }
export function peekCachedWorkspaceActivityMetrics(id: string) { statusRevision.value; return resource<WorkspaceActivityMetricsDto>(`activity:${id}`, 10000).state.value; }
export function getCachedWorkspaceReviews(id: string, force = false) { return resource<WorkspaceReviewSummaryDto[]>(`reviews:${id}`).get(() => listWorkspaceReviews(id), force); }
export function peekCachedWorkspaceReviews(id: string) { statusRevision.value; return resource<WorkspaceReviewSummaryDto[]>(`reviews:${id}`).state.value; }
export function setCachedWorkspaceReviews(id: string, value: WorkspaceReviewSummaryDto[]) { resource<WorkspaceReviewSummaryDto[]>(`reviews:${id}`).set(value); }
export function getCachedGlobalMcpOverview(force = false) { return resource<GlobalMcpOverviewDto>("global", 6000).get(getGlobalMcpOverview, force); }
export function globalResourceState() { statusRevision.value; const r = resource<GlobalMcpOverviewDto>("global", 6000); return { ...r.state, stale: r.stale() }; }
export function peekCachedGlobalMcpOverview() { statusRevision.value; return resource<GlobalMcpOverviewDto>("global", 6000).state.value; }
export function getCachedDetectedIdes(force = false) { return resource<DetectedIdeDto[]>("ides", 60000).get(detectInstalledIdes, force); }
export function peekCachedDetectedIdes() { statusRevision.value; return resource<DetectedIdeDto[]>("ides", 60000).state.value; }
export function invalidateWorkspaceStatus(id: string) { for (const kind of ["git", "planning", "activity", "reviews"])
    resources.get(`${kind}:${id}`)?.invalidate(); }
/** Load on entry to a workspace tab. Never poll Git across inactive workspaces. */
export function useWorkspaceStatus(id: Ref<string> | (() => string), options: { git?: boolean; global?: boolean } = {}) {
    const read = typeof id === "function" ? id : () => id.value;
    watch(read, value => {
        if (!value) return;
        void getCachedPlanningState(value).catch(() => { });
        if (options.git) void getCachedWorkspaceGitSummary(value).catch(() => { });
        if (options.global) void getCachedGlobalMcpOverview().catch(() => { });
    }, { immediate: true });
    return {
        planning: computed(() => peekCachedPlanningState(read())), git: computed(() => peekCachedWorkspaceGitSummary(read())),
        query: computed(() => workspaceResourceState(read())), gitQuery: computed(() => workspaceResourceState(read(), "git")),
        refresh: () => Promise.allSettled([
            getCachedPlanningState(read(), true),
            ...(options.git ? [getCachedWorkspaceGitSummary(read(), true)] : []),
            ...(options.global ? [getCachedGlobalMcpOverview(true)] : []),
        ]),
    };
}
