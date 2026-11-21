<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { Activity, FolderGit2, History, Plus, RefreshCw, Sparkles } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import EmptyState from "$src/components/ui/EmptyState.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import PageHeader from "$src/components/ui/PageHeader.vue";
import PageShell from "$src/components/ui/PageShell.vue";
import DashboardFocusPane from "$src/components/dashboard/DashboardFocusPane.vue";
import DashboardPreferencesPopover from "$src/components/dashboard/DashboardPreferencesPopover.vue";
import DashboardUsagePane from "$src/components/dashboard/DashboardUsagePane.vue";
import DashboardWorkspacesPane from "$src/components/dashboard/DashboardWorkspacesPane.vue";
import { listHistorySessions, type HistorySessionSummary } from "$lib/api/history";
import { getLastWorkspaceId } from "$lib/api/settings";
import { getServiceUsageStats, type ServiceUsageStats } from "$lib/api/usage";
import { openWorkspaceDirectory } from "$lib/api/workspaces";
import { useClipboard } from "$lib/composables/use-clipboard";
import { buildUsageChart, buildUsagePoint, summarizeUsage, type UsagePoint } from "$lib/dashboard";
import {
  dashboardPreferences,
  loadDashboardPreferences,
  moveWorkspace,
  sortWorkspaceIds,
  toggleDashboardModule,
  togglePinnedWorkspace,
  updateDashboardPreferences,
  type DashboardModuleId,
} from "$lib/dashboard-preferences";
import type { DashboardAttentionItem, DashboardWorkspaceRow } from "$lib/dashboard-types";
import { workspaces } from "$lib/stores/app";
import { globalHealthSummary } from "$lib/stores/health";
import { showToast } from "$lib/stores/toast";
import { taskFocus } from "$lib/ui-model";
import {
  getCachedPlanningState,
  getCachedWorkspaceGitSummary,
  peekCachedPlanningState,
  peekCachedWorkspaceGitSummary,
  workspaceResourceState,
} from "$lib/workspace-status-cache";

type DashboardPane = "focus" | "workspaces" | "activity" | "usage";
const USAGE_POLL_INTERVAL_MS = 30_000;
const MAX_USAGE_SAMPLES = 60;
const MAX_RECENT_ACTIVITIES = 12;

const router = useRouter();
const { copyText } = useClipboard(1600);
const lastId = ref("");
const preferencesOpen = ref(false);
const activePane = ref<DashboardPane>("focus");
const workspaceLayout = ref<"cards" | "table">("cards");
const workspaceSearch = ref("");
const workspaceScope = ref<"all" | "pinned" | "changed">("all");
const history = ref<Record<string, HistorySessionSummary[]>>({});
const usage = ref<Record<string, ServiceUsageStats[]>>({});
const analyticsOpen = ref(false);
const auxiliaryError = ref("");
let timer = 0;
let generation = 0;
let inFlight = false;
let loadedHistoryKey = "";
let rerunRequested = false;

const pinnedIds = computed(() => new Set(dashboardPreferences.value.pinnedWorkspaceIds));

const ordered = computed<DashboardWorkspaceRow[]>(() => {
  const byId = new Map(workspaces.value.map((w) => [w.id, w]));
  return sortWorkspaceIds([...byId.keys()], dashboardPreferences.value).map((id) => {
    const workspace = byId.get(id)!;
    const state = peekCachedPlanningState(id);
    const git = peekCachedWorkspaceGitSummary(id);
    const query = workspaceResourceState(id);
    return { workspace, state, git, query, focus: taskFocus(state) };
  });
});

const orderedIds = computed(() => ordered.value.map((row) => row.workspace.id));
const orderedIndexById = computed(() => new Map(ordered.value.map((row, index) => [row.workspace.id, index])));

const filteredWorkspaces = computed(() => {
  const q = workspaceSearch.value.trim().toLowerCase();
  return ordered.value.filter((row) => {
    if (workspaceScope.value === "pinned" && !pinnedIds.value.has(row.workspace.id)) {
      return false;
    }
    if (workspaceScope.value === "changed" && changedCountNumber(row) <= 0) {
      return false;
    }
    if (!q) return true;
    return (
      row.workspace.name.toLowerCase().includes(q) ||
      row.workspace.path.toLowerCase().includes(q) ||
      row.focus.title.toLowerCase().includes(q)
    );
  });
});

const spotlightWorkspaces = computed(() => ordered.value.slice(0, 6));

const focused = computed(
  () =>
    ordered.value.find((w) => w.workspace.id === lastId.value && (w.focus.plan || w.focus.goal)) ??
    ordered.value.find((w) => w.focus.plan || w.focus.goal),
);

const attention = computed<DashboardAttentionItem[]>(() =>
  ordered.value.flatMap((row) => {
    const result: DashboardAttentionItem[] = [];
    const id = row.workspace.id;
    if (row.query.error || row.query.stale) {
      result.push({
        key: id + "fetch",
        title: row.workspace.name + " · 状态待更新",
        detail: row.query.error || "上次数据已过期，正在重新检查。",
        path: `/workspace/${id}/planning`,
      });
    }
    for (const item of [...(row.state?.plans ?? []), ...(row.state?.goals ?? [])]) {
      if (item.status !== "awaiting_acceptance") continue;
      result.push({
        key: id + item.id,
        title: row.workspace.name + " · 待验收",
        detail: item.title,
        path: `/workspace/${id}/planning`,
        query: { item: item.id },
      });
    }
    if (row.state?.execution.last_error || ["blocked", "failed", "error"].includes(row.state?.execution.state ?? "")) {
      result.push({
        key: id + "error",
        title: row.workspace.name + " · 执行需要处理",
        detail: row.state?.execution.last_error || "执行已阻塞，请查看诊断。",
        path: `/workspace/${id}/diagnostics`,
      });
    }
    return result;
  }),
);

const attentionTotal = computed(
  () => attention.value.length + (["warning", "error"].includes(globalHealthSummary.value.state) ? 1 : 0),
);

const activities = computed(() =>
  workspaces.value
    .flatMap((workspace) =>
      (history.value[workspace.id] ?? []).map((session) => ({
        workspace,
        session,
        sortMs: new Date(session.updated_at ?? session.created_at ?? 0).getTime() || 0,
      })),
    )
    .sort((a, b) => b.sortMs - a.sortMs)
    .slice(0, MAX_RECENT_ACTIVITIES),
);

const samples = ref<UsagePoint[]>([]);
const chart = computed(() => buildUsageChart(samples.value));
const totals = computed(() => summarizeUsage(usage.value));

function addWorkspace() {
  window.dispatchEvent(new CustomEvent("coding-tools:add-workspace"));
}

function visible(id: DashboardModuleId) {
  return !dashboardPreferences.value.hiddenModules.includes(id);
}

function changedCountNumber(row: DashboardWorkspaceRow): number {
  if (!row.git) return 0;
  return (row.git.available ? row.git.changedFiles : 0) + row.git.subRepositories.reduce((n, r) => n + r.changedFiles, 0);
}

function changedCount(row: DashboardWorkspaceRow) {
  if (!row.git) return "—";
  return changedCountNumber(row);
}

function selectPane(pane: DashboardPane) {
  activePane.value = pane;
  analyticsOpen.value = pane === "usage";
  if (pane === "activity") void loadAuxiliary();
}

function handleMoveWorkspace(workspaceId: string, direction: -1 | 1) {
  moveWorkspace(workspaceId, direction, orderedIds.value);
}

async function loadAuxiliary(forceHistory = false) {
  if (inFlight) {
    rerunRequested = true;
    return;
  }
  const needHistory = visible('activity') && activePane.value === 'activity';
  const needUsage = analyticsOpen.value;
  if (!needHistory && !needUsage) return;

  inFlight = true;
  const id = ++generation;
  const items = [...workspaces.value];
  const historyKey = items.map((w) => w.id).join(",");
  const errors: string[] = [];
  try {
    if (visible('activity') && activePane.value === 'activity' && (forceHistory || loadedHistoryKey !== historyKey)) {
      const settled = await Promise.allSettled(
        items.map(async (w) => [w.id, (await listHistorySessions(w.id)).sessions] as const),
      );
      if (id === generation) {
        const nextHistory: Record<string, HistorySessionSummary[]> = {};
        for (const outcome of settled) {
          if (outcome.status === "fulfilled") {
            const [wsId, sessions] = outcome.value;
            nextHistory[wsId] = sessions;
          } else {
            errors.push(String(outcome.reason));
          }
        }
        history.value = nextHistory;
        loadedHistoryKey = historyKey;
      }
    }
    if (analyticsOpen.value) {
      const settled = await Promise.allSettled(
        items.map(async (w) => [w.id, await getServiceUsageStats(w.id)] as const),
      );
      if (id === generation) {
        const nextUsage: Record<string, ServiceUsageStats[]> = {};
        for (const outcome of settled) {
          if (outcome.status === "fulfilled") {
            const [wsId, stats] = outcome.value;
            nextUsage[wsId] = stats;
          } else {
            errors.push(String(outcome.reason));
          }
        }
        usage.value = nextUsage;
        samples.value = [...samples.value, buildUsagePoint(nextUsage, samples.value.at(-1))].slice(-MAX_USAGE_SAMPLES);
      }
    }
    if (id === generation) {
      auxiliaryError.value = errors[0] ?? "";
    }
  } finally {
    inFlight = false;
    if (rerunRequested && id === generation) {
      rerunRequested = false;
      void loadAuxiliary();
    }
  }
}

async function refresh() {
  await Promise.allSettled(
    ordered.value.flatMap((row) => [getCachedPlanningState(row.workspace.id, true), getCachedWorkspaceGitSummary(row.workspace.id, true)]),
  );
  await loadAuxiliary(true);
}

async function reveal(path: string) {
  try {
    await openWorkspaceDirectory(path);
  } catch (e) {
    showToast(String(e), { kind: "error" });
  }
}

async function copy(path: string) {
  await copyText(path, { key: path, label: "路径" });
}

watch(
  () => dashboardPreferences.value.hiddenModules,
  () => {
    if (activePane.value !== "focus" && !visible(activePane.value)) {
      selectPane("focus");
    }
  },
);

watch(
  () => workspaces.value.map((w) => w.id).join(","),
  () => void loadAuxiliary(),
);
watch(
  () => visible("activity"),
  (open) => {
    if (open && activePane.value === "activity") void loadAuxiliary();
  },
);
watch(analyticsOpen, open => {
  if (timer) window.clearInterval(timer);
  timer = 0;
  if (!open) return;
  void loadAuxiliary();
  timer = window.setInterval(() => {
    if (document.visibilityState === "visible") void loadAuxiliary();
  }, USAGE_POLL_INTERVAL_MS);
});

onMounted(() => {
  loadDashboardPreferences();
  void getLastWorkspaceId()
    .then((id) => {
      lastId.value = id;
      if (id) void getCachedPlanningState(id).catch(() => undefined);
    })
    .catch((err) => {
      console.warn("[DashboardView] failed to read last workspace id:", err);
    });
});

onUnmounted(() => {
  generation++;
  window.clearInterval(timer);
});
</script>

<template>
  <section class="page-scroll" :data-density="dashboardPreferences.density">
    <PageShell>
      <PageHeader kicker="AI 开发工作台" title="今天的开发工作" description="按左侧专题快速切换视图，优先处理待验收与阻塞事项。">
        <template #actions>
          <BaseButton variant="secondary" size="sm" @click="refresh">
            <RefreshCw :size="14" />
            <span>刷新</span>
          </BaseButton>
          <DashboardPreferencesPopover
            :open="preferencesOpen"
            :density="dashboardPreferences.density"
            :hidden-modules="dashboardPreferences.hiddenModules"
            @toggle-open="preferencesOpen = !preferencesOpen"
            @toggle-density="updateDashboardPreferences((p) => ({ ...p, density: p.density === 'compact' ? 'comfortable' : 'compact' }))"
            @toggle-module="toggleDashboardModule"
          />
        </template>
      </PageHeader>

      <div class="ui-subnav-layout">
        <!-- Left Secondary Dashboard View Rail -->
        <aside class="ui-subnav-rail" aria-label="工作台总览分区">
          <div class="ui-subnav-group">
            <div class="ui-subnav-group__title">工作台分区</div>
            <button
              type="button"
              class="ui-subnav-item"
              :class="{ active: activePane === 'focus' }"
              @click="selectPane('focus')"
            >
              <Sparkles :size="15" class="shrink-0" />
              <span class="flex-1 truncate">焦点与待处理</span>
              <span
                v-if="attentionTotal > 0"
                class="rounded-full bg-[var(--ui-warning-soft)] px-1.5 py-0.5 text-[10.5px] font-semibold tabular-nums text-[var(--ui-warning)]"
              >
                {{ attentionTotal }}
              </span>
            </button>

            <button
              v-if="visible('workspaces')"
              type="button"
              class="ui-subnav-item"
              :class="{ active: activePane === 'workspaces' }"
              @click="selectPane('workspaces')"
            >
              <FolderGit2 :size="15" class="shrink-0" />
              <span class="flex-1 truncate">全部工作区</span>
              <span class="rounded-full bg-[var(--ui-surface-subtle)] px-1.5 py-0.5 text-[10.5px] tabular-nums text-[var(--ui-text-muted)]">
                {{ ordered.length }}
              </span>
            </button>

            <button
              v-if="visible('activity')"
              type="button"
              class="ui-subnav-item"
              :class="{ active: activePane === 'activity' }"
              @click="selectPane('activity')"
            >
              <History :size="15" class="shrink-0" />
              <span class="flex-1 truncate">最近开发会话</span>
            </button>

            <button
              v-if="visible('usage')"
              type="button"
              class="ui-subnav-item"
              :class="{ active: activePane === 'usage' }"
              @click="selectPane('usage')"
            >
              <Activity :size="15" class="shrink-0" />
              <span class="flex-1 truncate">传输用量分析</span>
            </button>
          </div>

          <div class="mt-3 border-t border-[var(--ui-line)] pt-3">
            <button type="button" class="ui-subnav-item text-[var(--ui-accent)]" @click="addWorkspace">
              <Plus :size="15" class="shrink-0" />
              <span class="truncate">添加新工作区…</span>
            </button>
          </div>
        </aside>

        <!-- Focused Single-Screen Stage -->
        <div :key="activePane" class="ui-subnav-stage ui-stack min-w-0 animate-fade-in-up">
          <DashboardFocusPane
            v-if="activePane === 'focus'"
            :show-focus="visible('focus')"
            :show-workspaces="visible('workspaces')"
            :focused="focused"
            :attention="attention"
            :attention-total="attentionTotal"
            :health-state="globalHealthSummary.state"
            :health-label="globalHealthSummary.label"
            :health-detail="globalHealthSummary.detail"
            :spotlight-workspaces="spotlightWorkspaces"
            :total-workspaces="ordered.length"
            :pinned-ids="pinnedIds"
            :changed-count="changedCount"
            @open-path="router.push($event)"
            @open-all-workspaces="selectPane('workspaces')"
            @add-workspace="addWorkspace"
          />

          <DashboardWorkspacesPane
            v-else-if="activePane === 'workspaces'"
            v-model:layout="workspaceLayout"
            v-model:search="workspaceSearch"
            v-model:scope="workspaceScope"
            :total-count="ordered.length"
            :filtered-workspaces="filteredWorkspaces"
            :ordered-index-by-id="orderedIndexById"
            :pinned-ids="pinnedIds"
            :changed-count="changedCount"
            @open-path="router.push($event)"
            @add-workspace="addWorkspace"
            @toggle-pin="togglePinnedWorkspace"
            @move="handleMoveWorkspace"
            @reveal="reveal"
            @copy="copy"
          />

          <GlassCard v-else-if="activePane === 'activity'">
            <div class="ui-section-heading">
              <div>
                <h2>最近开发会话</h2>
                <p>汇总各工作区最近归档的上下文检查点，点击直达对应项目的会话历史。</p>
              </div>
              <span class="ui-muted">最新 {{ activities.length }} 条</span>
            </div>
            <p v-if="auxiliaryError" role="alert" class="ui-danger text-xs">历史加载失败：{{ auxiliaryError }}</p>
            <div class="space-y-1">
              <button
                v-for="item in activities"
                :key="item.workspace.id + item.session.number"
                class="ui-list-action"
                @click="router.push(`/workspace/${item.workspace.id}/history`)"
              >
                <span>
                  <strong>{{ item.workspace.name }} · {{ item.session.title || "开发会话" }}</strong>
                  <small class="line-clamp-2">{{ item.session.latest_focus || "查看会话摘要与关联变更" }}</small>
                </span>
                <span>查看会话 →</span>
              </button>
            </div>
            <EmptyState
              v-if="!activities.length && !auxiliaryError"
              title="暂无归档会话活动"
              description="在工作区开启历史记录后，AI 提交的会话检查点会汇总在此处。"
            />
          </GlassCard>

          <DashboardUsagePane
            v-else
            :auxiliary-error="auxiliaryError"
            :totals="totals"
            :samples="samples"
            :chart="chart"
            :ordered="ordered"
            :usage="usage"
          />
        </div>
      </div>
    </PageShell>
  </section>
</template>
