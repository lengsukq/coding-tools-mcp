import { computed, onUnmounted, ref, watch, type Ref } from "vue";
import { createResource } from "./async-resource";
import { getPlanningState, type PlanningStateDto } from "$lib/api/planning";
import { detectInstalledIdes, getGlobalMcpOverview, getWorkspaceActivityMetrics, getWorkspaceGitSummary, listWorkspaceReviews, type DetectedIdeDto, type GlobalMcpOverviewDto, type WorkspaceActivityMetricsDto, type WorkspaceGitSummaryDto, type WorkspaceReviewSummaryDto, } from "$lib/api/workspaces";
const resources = new Map<string, { invalidate: () => void }>();
export const statusRevision = ref(0);
function resource<T>(key: string, ttl = 6000) {
    if (!resources.has(key))
        resources.set(key, createResource<T>(ttl, () => { statusRevision.value++; }));
    return resources.get(key)! as ReturnType<typeof createResource<T>>;
}
export function workspaceResourceState(workspaceId: string, kind = "planning") {
    statusRevision.value;
    const r = resource(`${kind}:${workspaceId}`);
    return { ...r.state, stale: r.stale() };
}
export function getCachedWorkspaceGitSummary(id: string, force = false) { return resource<WorkspaceGitSummaryDto>(`git:${id}`).get(() => getWorkspaceGitSummary(id), force); }
export function peekCachedWorkspaceGitSummary(id: string) { statusRevision.value; return resource<WorkspaceGitSummaryDto>(`git:${id}`).state.value; }
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
const subscribers = new Map<string, number>();
let timer: ReturnType<typeof setInterval> | null = null;
function refreshShared() {
    if (document.visibilityState === "hidden")
        return;
    statusRevision.value++;
    for (const id of subscribers.keys()) {
        void getCachedPlanningState(id).catch(() => { });
        void getCachedWorkspaceGitSummary(id).catch(() => { });
    }
    void getCachedGlobalMcpOverview().catch(() => { });
}
function subscribe(id: string) {
    subscribers.set(id, (subscribers.get(id) ?? 0) + 1);
    if (!timer) {
        timer = setInterval(refreshShared, 3000);
        window.addEventListener("focus", refreshShared);
    }
    refreshShared();
    return () => {
        const count = (subscribers.get(id) ?? 1) - 1;
        if (count)
            subscribers.set(id, count);
        else
            subscribers.delete(id);
        if (!subscribers.size && timer) {
            clearInterval(timer);
            timer = null;
            window.removeEventListener("focus", refreshShared);
        }
    };
}
export function useWorkspaceStatus(id: Ref<string> | (() => string)) {
    let release: (() => void) | undefined;
    const read = typeof id === "function" ? id : () => id.value;
    watch(read, value => { release?.(); if (value)
        release = subscribe(value); }, { immediate: true });
    onUnmounted(() => release?.());
    return {
        planning: computed(() => peekCachedPlanningState(read())), git: computed(() => peekCachedWorkspaceGitSummary(read())),
        query: computed(() => workspaceResourceState(read())), gitQuery: computed(() => workspaceResourceState(read(), "git")),
        refresh: () => Promise.allSettled([getCachedPlanningState(read(), true), getCachedWorkspaceGitSummary(read(), true)]),
    };
}
