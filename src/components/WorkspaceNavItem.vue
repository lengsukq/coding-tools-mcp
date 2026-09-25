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
  min-height: 56px;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
  border: 1px solid transparent;
  border-radius: 14px;
  background: transparent;
  color: var(--text-main);
  text-align: left;
  cursor: pointer;
  transition: background 160ms ease, border-color 160ms ease, box-shadow 160ms ease, transform 160ms ease;
}
.ios-workspace-nav.active { min-height: 64px; }

.ios-workspace-nav:hover {
  background: rgba(255, 255, 255, 0.58);
  border-color: rgba(255, 255, 255, 0.72);
}

.ios-workspace-nav:active { transform: scale(.985); }

.ios-workspace-nav.active {
  background: linear-gradient(135deg, color-mix(in srgb,var(--ios-blue) 15%,transparent), color-mix(in srgb,var(--accent-indigo) 9%,transparent));
  border-color: color-mix(in srgb,var(--ios-blue) 16%,transparent);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, .72), 0 8px 22px color-mix(in srgb,var(--ios-blue) 8%,transparent);
}

.ios-workspace-nav__icon {
  display: grid;
  width: 30px;
  height: 30px;
  flex: 0 0 30px;
  place-items: center;
  border-radius: 10px;
  background: var(--primary-soft, rgba(var(--pal-rgb, 99, 102, 241), 0.1));
  color: var(--primary, #5e5ce6);
}

.active .ios-workspace-nav__icon {
  background: var(--primary-gradient, linear-gradient(145deg, var(--ios-blue), var(--accent-indigo)));
  color: #fff;
  box-shadow: 0 5px 14px color-mix(in srgb,var(--ios-blue) 24%,transparent);
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
  font-size: 12px;
  font-weight: 650;
  line-height: 1.25;
  letter-spacing: -.01em;
}

.ios-workspace-nav__meta {
  margin-top: 3px;
  color: var(--text-muted);
  font-size: 9.5px;
  line-height: 1;
}
.ios-workspace-nav__focus {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 5px;
  margin-top: 4px;
  color: var(--text-secondary);
  font-size: 9px;
  line-height: 1.1;
}
.ios-workspace-nav__focus-title {
  min-width: 0;
  flex: 1;
}
.ios-workspace-nav__focus-kind {
  flex: 0 0 auto;
  color: var(--ios-blue);
  font-size: 8px;
  font-weight: 700;
  letter-spacing: .04em;
  text-transform: uppercase;
}
.ios-workspace-nav__focus-progress {
  flex: 0 0 auto;
  color: var(--text-muted);
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
  background: linear-gradient(90deg,var(--ios-blue),var(--accent-indigo));
  transition: width 180ms ease;
}

.ios-workspace-nav__status {
  width: 7px;
  height: 7px;
  flex: 0 0 7px;
  border-radius: 999px;
  background: rgba(100, 116, 139, .38);
}

.ios-workspace-nav__status.is-clean { background: var(--accent-success); box-shadow: 0 0 0 3px color-mix(in srgb,var(--accent-success) 12%,transparent); }
.ios-workspace-nav__status.is-dirty { background: var(--accent-warning); box-shadow: 0 0 0 3px color-mix(in srgb,var(--accent-warning) 12%,transparent); }
.ios-workspace-nav__status.is-running { background: var(--ios-blue); box-shadow: 0 0 0 3px color-mix(in srgb,var(--ios-blue) 13%,transparent); }
.ios-workspace-nav__status.is-error { background: var(--accent-danger); box-shadow: 0 0 0 3px color-mix(in srgb,var(--accent-danger) 13%,transparent); }
.ios-workspace-nav__status.is-neutral { background: rgba(100, 116, 139, .38); }

:global(.dark) .ios-workspace-nav:hover {
  background: rgba(255,255,255,.055);
  border-color: rgba(255,255,255,.07);
}

:global(.dark) .ios-workspace-nav.active {
  background: linear-gradient(135deg, color-mix(in srgb,var(--ios-blue) 18%,transparent), color-mix(in srgb,var(--accent-indigo) 11%,transparent));
  border-color: var(--card-border-active, color-mix(in srgb,var(--ios-blue) 28%,transparent));
  box-shadow: inset 0 1px 0 rgba(255,255,255,.055), 0 8px 22px rgba(0,0,0,.13);
}
</style>
