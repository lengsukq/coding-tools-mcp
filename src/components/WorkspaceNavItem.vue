<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import { Folder } from "@lucide/vue";
import type { PlanningStateDto } from "$lib/api/planning";
import type { WorkspaceGitSummaryDto } from "$lib/api/workspaces";
import { getCachedPlanningState, getCachedWorkspaceGitSummary } from "$lib/workspace-status-cache";
import type { WorkspaceProfile } from "$lib/types";

const props = defineProps<{
  workspace: WorkspaceProfile;
  active?: boolean;
}>();

defineEmits<{ click: [] }>();

const git = ref<WorkspaceGitSummaryDto | null>(null);
const planning = ref<PlanningStateDto | null>(null);
const gitLoading = ref(false);
let refreshTimer = 0;
let refreshInFlight = false;
let lastLoadedAt = 0;

const totalChanges = computed(() => {
  if (!git.value) return 0;
  return (git.value.available ? git.value.changedFiles : 0)
    + git.value.subRepositories.reduce((sum, repo) => sum + repo.changedFiles, 0);
});
const gitMeta = computed(() => {
  if (gitLoading.value && !git.value) return "Git 检测中…";
  if (!git.value) return "Workspace";
  if (git.value.available) {
    const branch = git.value.branch || "detached";
    return totalChanges.value ? branch + " · " + totalChanges.value + " changes" : branch + " · clean";
  }
  if (git.value.subRepositories.length) {
    if (git.value.subRepositories.length === 1) {
      const repo = git.value.subRepositories[0]!;
      const branch = repo.branch || "detached";
      return totalChanges.value ? branch + " · " + totalChanges.value + " changes" : branch + " · clean";
    }
    return totalChanges.value
      ? git.value.subRepositories.length + " repos · " + totalChanges.value + " changes"
      : git.value.subRepositories.length + " repos · clean";
  }
  return "非 Git";
});
const focusSummary = computed(() => {
  const state = planning.value;
  if (!state) return null;
  const plan = state.plans.find(item => item.id === state.focus_plan_id);
  if (plan) {
    const completed = plan.steps.filter(step => step.status === "completed" || step.status === "skipped").length;
    const total = plan.steps.length;
    return {
      kind: "Plan",
      title: plan.title,
      completed,
      total,
      percent: total ? Math.round(completed / total * 100) : 0,
    };
  }
  const goal = state.goals.find(item => item.id === state.focus_goal_id);
  if (!goal) return null;
  const completed = goal.success_criteria.filter(item => item.completed).length;
  const total = goal.success_criteria.length;
  return {
    kind: "Goal",
    title: goal.title,
    completed,
    total,
    percent: total ? Math.round(completed / total * 100) : 0,
  };
});
const executionState = computed(() => planning.value?.execution.state?.toLowerCase() ?? "");
const workspaceState = computed(() => {
  if (planning.value?.execution.last_error || ["blocked", "failed", "error"].includes(executionState.value)) return "error";
  if (["running", "in_progress", "executing"].includes(executionState.value)) return "running";
  if (!git.value || (!git.value.available && !git.value.subRepositories.length)) return "neutral";
  return totalChanges.value > 0 ? "dirty" : "clean";
});

async function loadStatus(force = false) {
  if (refreshInFlight) return;
  refreshInFlight = true;
  gitLoading.value = true;
  try {
    const [gitResult, planningResult] = await Promise.allSettled([
      getCachedWorkspaceGitSummary(props.workspace.id, force),
      getCachedPlanningState(props.workspace.id, force),
    ]);
    git.value = gitResult.status === "fulfilled" ? gitResult.value : null;
    planning.value = planningResult.status === "fulfilled" ? planningResult.value : null;
    lastLoadedAt = Date.now();
  } finally {
    gitLoading.value = false;
    refreshInFlight = false;
  }
}

function syncRefreshTimer() {
  window.clearInterval(refreshTimer);
  refreshTimer = 0;
  if (props.active) {
    refreshTimer = window.setInterval(() => {
      if (document.visibilityState === "visible") void loadStatus(true);
    }, 8000);
  }
}

watch(() => props.workspace.id, () => {
  void loadStatus();
  syncRefreshTimer();
}, { immediate: true });
watch(() => props.active, (active, previous) => {
  syncRefreshTimer();
  if (active && !previous && Date.now() - lastLoadedAt > 3000) {
    void loadStatus();
  }
});
onUnmounted(() => window.clearInterval(refreshTimer));
</script>

<template>
  <button
    type="button"
    class="ios-workspace-nav"
    :class="{ active }"
    :title="focusSummary ? workspace.path + '\n' + focusSummary.kind + ' · ' + focusSummary.title : workspace.path"
    @click="$emit('click')"
  >
    <span class="ios-workspace-nav__icon"><Folder :size="15" :stroke-width="2" /></span>
    <span class="ios-workspace-nav__content">
      <span class="ios-workspace-nav__name">{{ workspace.name }}</span>
      <span class="ios-workspace-nav__meta">{{ gitMeta }}</span>
      <span v-if="focusSummary" class="ios-workspace-nav__focus">
        <span class="ios-workspace-nav__focus-kind">{{ focusSummary.kind }}</span>
        <span class="ios-workspace-nav__focus-title">{{ focusSummary.title }}</span>
        <span class="ios-workspace-nav__focus-progress">{{ focusSummary.completed }}/{{ focusSummary.total }}</span>
      </span>
      <span v-if="active && focusSummary" class="ios-workspace-nav__progress" aria-hidden="true">
        <span :style="{ width: focusSummary.percent + '%' }" />
      </span>
    </span>
    <span class="ios-workspace-nav__status" :class="'is-' + workspaceState" />
  </button>
</template>

<style scoped>
.ios-workspace-nav {
  position: relative;
  display: flex;
  width: 100%;
  min-width: 0;
  min-height: 52px;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
  border: 1px solid transparent;
  border-radius: var(--ui-radius-row);
  background: transparent;
  color: var(--ui-text);
  text-align: left;
  cursor: pointer;
  transition: background 150ms ease-out, border-color 150ms ease-out;
}
.ios-workspace-nav.active { min-height: 58px; }

.ios-workspace-nav:hover {
  background: var(--ui-surface-hover);
  border-color: var(--ui-line);
}

.ios-workspace-nav.active {
  background: var(--ui-accent-soft);
  border-color: color-mix(in srgb, var(--ui-accent) 24%, transparent);
}

.ios-workspace-nav__icon {
  display: grid;
  width: 30px;
  height: 30px;
  flex: 0 0 30px;
  place-items: center;
  border-radius: var(--ui-radius-control);
  background: var(--ui-surface-subtle);
  color: var(--ui-text-secondary);
}

.active .ios-workspace-nav__icon {
  background: var(--ui-accent);
  color: #fff;
}

.ios-workspace-nav__content {
  display: block;
  min-width: 0;
  flex: 1;
}

.ios-workspace-nav__name,
.ios-workspace-nav__meta,
.ios-workspace-nav__focus-title {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ios-workspace-nav__name {
  font-size: 13px;
  font-weight: 600;
  line-height: 1.25;
  letter-spacing: -.01em;
}

.ios-workspace-nav__meta {
  margin-top: 3px;
  color: var(--ui-text-muted);
  font-size: 11px;
  line-height: 1.2;
}
.ios-workspace-nav__focus {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 5px;
  margin-top: 4px;
  color: var(--ui-text-secondary);
  font-size: 11px;
  line-height: 1.2;
}
.ios-workspace-nav__focus-title {
  min-width: 0;
  flex: 1;
}
.ios-workspace-nav__focus-kind {
  flex: 0 0 auto;
  color: var(--ui-accent);
  font-size: 10px;
  font-weight: 600;
}
.ios-workspace-nav__focus-progress {
  flex: 0 0 auto;
  color: var(--ui-text-muted);
  font-variant-numeric: tabular-nums;
}
.ios-workspace-nav__progress {
  display: block;
  height: 3px;
  margin-top: 5px;
  overflow: hidden;
  border-radius: 999px;
  background: color-mix(in srgb,var(--text-main) 7%,transparent);
}
.ios-workspace-nav__progress > span {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: var(--ui-accent);
  transition: width 180ms ease;
}

.ios-workspace-nav__status {
  width: 7px;
  height: 7px;
  flex: 0 0 7px;
  border-radius: 999px;
  background: rgba(100, 116, 139, .38);
}

.ios-workspace-nav__status.is-clean { background: var(--ui-success); }
.ios-workspace-nav__status.is-dirty { background: var(--ui-warning); }
.ios-workspace-nav__status.is-running { background: var(--ui-accent); }
.ios-workspace-nav__status.is-error { background: var(--ui-danger); }
.ios-workspace-nav__status.is-neutral { background: rgba(100, 116, 139, .38); }

:global(.dark) .ios-workspace-nav:hover {
  background: var(--ui-surface-hover);
  border-color: var(--ui-line);
}

:global(.dark) .ios-workspace-nav.active {
  background: var(--ui-accent-soft);
  border-color: color-mix(in srgb, var(--ui-accent) 28%, transparent);
  box-shadow: none;
}
</style>
