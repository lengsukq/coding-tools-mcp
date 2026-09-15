import { ref } from "vue";

export type DashboardDensity = "comfortable" | "compact";
export type DashboardModuleId =
  | "focus"
  | "attention"
  | "workspaces"
  | "activity"
  | "usage"
  | "health";

export interface DashboardPreferences {
  density: DashboardDensity;
  hiddenModules: DashboardModuleId[];
  pinnedWorkspaceIds: string[];
  workspaceOrder: string[];
}

const STORAGE_KEY = "coding-tools.dashboard.preferences.v2";

const defaults: DashboardPreferences = {
  density: "comfortable",
  hiddenModules: [],
  pinnedWorkspaceIds: [],
  workspaceOrder: [],
};

function readPreferences(): DashboardPreferences {
  if (typeof window === "undefined") return defaults;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaults;
    const parsed = JSON.parse(raw) as Partial<DashboardPreferences>;
    return {
      density: parsed.density === "compact" ? "compact" : "comfortable",
      hiddenModules: Array.isArray(parsed.hiddenModules) ? parsed.hiddenModules : [],
      pinnedWorkspaceIds: Array.isArray(parsed.pinnedWorkspaceIds) ? parsed.pinnedWorkspaceIds : [],
      workspaceOrder: Array.isArray(parsed.workspaceOrder) ? parsed.workspaceOrder : [],
    };
  } catch {
    return defaults;
  }
}

export const dashboardPreferences = ref<DashboardPreferences>(defaults);

export function loadDashboardPreferences() {
  dashboardPreferences.value = readPreferences();
}

export function updateDashboardPreferences(
  updater: (current: DashboardPreferences) => DashboardPreferences,
) {
  const next = updater(dashboardPreferences.value);
  if (typeof window !== "undefined") {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  }
  dashboardPreferences.value = next;
}

export function toggleDashboardModule(moduleId: DashboardModuleId) {
  updateDashboardPreferences((current) => ({
    ...current,
    hiddenModules: current.hiddenModules.includes(moduleId)
      ? current.hiddenModules.filter((item) => item !== moduleId)
      : [...current.hiddenModules, moduleId],
  }));
}

export function togglePinnedWorkspace(workspaceId: string) {
  updateDashboardPreferences((current) => ({
    ...current,
    pinnedWorkspaceIds: current.pinnedWorkspaceIds.includes(workspaceId)
      ? current.pinnedWorkspaceIds.filter((item) => item !== workspaceId)
      : [...current.pinnedWorkspaceIds, workspaceId],
  }));
}

export function moveWorkspace(workspaceId: string, direction: -1 | 1, visibleIds: string[] = []) {
  updateDashboardPreferences((current) => {
    const base = visibleIds.length > 0
      ? visibleIds
      : [...current.workspaceOrder, workspaceId].filter((id, index, ids) => ids.indexOf(id) === index);
    const ids = base.filter((id) => id !== workspaceId);
    const sourceIndex = Math.max(0, base.indexOf(workspaceId));
    const targetIndex = Math.max(0, Math.min(ids.length, sourceIndex + direction));
    ids.splice(targetIndex, 0, workspaceId);
    const hidden = current.workspaceOrder.filter((id) => !ids.includes(id));
    return { ...current, workspaceOrder: [...ids, ...hidden] };
  });
}

export function sortWorkspaceIds(ids: string[], preferences: DashboardPreferences): string[] {
  const order = new Map(preferences.workspaceOrder.map((id, index) => [id, index]));
  const pinned = new Set(preferences.pinnedWorkspaceIds);
  return [...ids].sort((a, b) => {
    const pinnedDelta = Number(pinned.has(b)) - Number(pinned.has(a));
    if (pinnedDelta !== 0) return pinnedDelta;
    const aOrder = order.get(a) ?? Number.MAX_SAFE_INTEGER;
    const bOrder = order.get(b) ?? Number.MAX_SAFE_INTEGER;
    return aOrder - bOrder;
  });
}
