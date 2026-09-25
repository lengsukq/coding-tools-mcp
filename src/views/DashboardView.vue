<script setup lang="ts">
import {
  Activity,
  AlertTriangle,
  ArrowUpRight,
  Boxes,
  Check,
  ChevronDown,
  ChevronUp,
  Command,
  Copy,
  Cpu,
  FolderOpen,
  Gauge,
  GitBranch,
  LayoutDashboard,
  ListChecks,
  Pin,
  PinOff,
  Play,
  RotateCw,
  ShieldCheck,
  Sparkles,
  Square,
  X,
  Zap,
} from "@lucide/vue";
import { computed, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { useRouter } from "vue-router";
import DashboardCommandPalette from "$src/components/dashboard/DashboardCommandPalette.vue";
import DashboardPreferencesPopover from "$src/components/dashboard/DashboardPreferencesPopover.vue";
import DashboardUsagePanel from "$src/components/dashboard/DashboardUsagePanel.vue";
import { runGlobalHealthChecks, type HealthItem } from "$lib/api/health";
import { listHistorySessions, type HistorySessionSummary } from "$lib/api/history";
import type { PlanningStateDto } from "$lib/api/planning";
import { getLastWorkspaceId } from "$lib/api/settings";
import type { ServiceUsageStats } from "$lib/api/usage";
import {
  buildUsageChart,
  buildUsagePoint,
  formatCount,
  loadPlanningByWorkspace,
  loadUsageByWorkspace,
  summarizePlanning,
  summarizeUsage,
  type UsagePoint,
} from "$lib/dashboard";
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
import {
  getGlobalMcpOverview,
  getRuntimeStatus,
  openWorkspaceDirectory,
  startRuntime,
  stopRuntime,
  type GlobalMcpOverviewDto,
} from "$lib/api/workspaces";
import { DEFAULT_GLOBAL_GATEWAY, getGlobalGatewayConfig, type GlobalGatewayConfigDto } from "$lib/api/global-gateway";
import { runServiceToggle } from "$lib/runtime/service";
import { showToast } from "$lib/stores/toast";
import { globalMcpRuntimeState, workspaces } from "$lib/stores/app";
import type { WorkspaceProfile } from "$lib/types";

interface FocusItem {
  workspace: WorkspaceProfile;
  title: string;
  detail: string;
  progress: number;
  progressLabel: string;
  mode: string;
}

async function loadGlobalOverview() {
  try {
    const overview = await getGlobalMcpOverview();
    globalOverview.value = overview;
    globalMcpRuntimeState.value = overview.state;
  } catch {
    // Keep the last known state; App-level refresh still handles hard failures.
  }
}

function requestAddWorkspace() {
  window.dispatchEvent(new CustomEvent("coding-tools:add-workspace"));
}

interface AttentionItem {
  workspace: WorkspaceProfile;
  title: string;
  detail: string;
  level: "warning" | "error" | "info";
}

interface ActivityItem {
  workspace: WorkspaceProfile;
  title: string;
  detail: string;
  timestamp: number;
}

const router = useRouter();
const lastWorkspaceId = ref("");
const planningByWorkspace = ref<Record<string, PlanningStateDto | null>>({});
const usageByWorkspace = ref<Record<string, ServiceUsageStats[]>>({});
const historyByWorkspace = ref<Record<string, HistorySessionSummary[]>>({});
const usageHistory = ref<UsagePoint[]>([]);
const usageWorkspaceKey = ref("");
const globalRuntimeBusy = ref(false);
const globalHealth = ref<HealthItem[]>([]);
const globalHealthBusy = ref(false);
const gatewayConfig = reactive<GlobalGatewayConfigDto>({ ...DEFAULT_GLOBAL_GATEWAY });
const globalOverview = ref<GlobalMcpOverviewDto>({
  state: "stopped",
  localEndpoint: "",
  publicEndpoint: "",
  workspaceCount: 0,
  sessionCount: 0,
  registryRevision: 0,
  sessions: [],
});
const copiedPathId = ref<string | null>(null);
const commandOpen = ref(false);
const commandQuery = ref("");
const preferencesOpen = ref(false);
let planningGeneration = 0;
let usageGeneration = 0;
let historyGeneration = 0;
let planningTimer = 0;
let usageTimer = 0;
let historyTimer = 0;
let healthTimer = 0;

async function loadGlobalHealth() {
  if (globalHealthBusy.value) return;
  globalHealthBusy.value = true;
  try {
    globalHealth.value = await runGlobalHealthChecks();
  } catch {
    // Preserve the last successful snapshot so the dashboard does not flicker offline.
  } finally {
    globalHealthBusy.value = false;
  }
}

const workspaceCount = computed(() => workspaces.value.length);
const orderedWorkspaces = computed(() => {
  const ids = sortWorkspaceIds(workspaces.value.map((item) => item.id), dashboardPreferences.value);
  const byId = new Map(workspaces.value.map((item) => [item.id, item]));
  return ids.map((id) => byId.get(id)).filter((item): item is WorkspaceProfile => Boolean(item));
});
const mcpRunning = computed(() => globalMcpRuntimeState.value === "running" ? 1 : 0);
const errorServices = computed(() => globalMcpRuntimeState.value === "error" ? 1 : 0);
const serviceHealth = computed(() => globalMcpRuntimeState.value === "running" ? 100 : 0);
const planningStats = computed(() => summarizePlanning(planningByWorkspace.value));
const usageTotals = computed(() => summarizeUsage(usageByWorkspace.value));
const executionStats = computed(() => {
  let running = 0;
  let blocked = 0;
  let verified = 0;
  let changedFiles = 0;
  for (const planning of Object.values(planningByWorkspace.value)) {
    if (!planning) continue;
    const state = planning.execution.state.toLowerCase();
    if (["running", "in_progress", "executing"].includes(state)) running += 1;
    if (["blocked", "failed", "error"].includes(state) || planning.execution.last_error) blocked += 1;
    if (planning.execution.verification.length > 0) verified += 1;
    changedFiles += planning.execution.changed_files.length;
  }
  return { running, blocked, verified, changedFiles };
});
const averageTokens = computed(() => usageTotals.value.toolCallCount === 0 ? 0 : usageTotals.value.estimatedToolCallTokens / usageTotals.value.toolCallCount);
const usageChart = computed(() => buildUsageChart(usageHistory.value));
const runtimeMix = computed(() => [
  { key: "running", label: "Running", value: mcpRunning.value },
  { key: "offline", label: "Offline", value: globalMcpRuntimeState.value === "stopped" ? 1 : 0 },
  { key: "error", label: "Error", value: errorServices.value },
]);
const connectionMix = computed(() => [
  { key: "gateway", label: "FRP", value: gatewayConfig.tunnelType === "frp" ? 1 : 0 },
  { key: "cloudflare", label: "Cloudflare", value: gatewayConfig.tunnelType === "cloudflare" ? 1 : 0 },
  { key: "local", label: "Local", value: !gatewayConfig.enabled || gatewayConfig.tunnelType === "none" ? 1 : 0 },
]);
const planningModeMix = computed(() => [
  { key: "direct", label: "Direct", value: planningStats.value.modes.direct },
  { key: "plan", label: "Plan", value: planningStats.value.modes.plan },
  { key: "goal", label: "Goal", value: planningStats.value.modes.goal },
]);

const focusItems = computed<FocusItem[]>(() => orderedWorkspaces.value.flatMap((workspace) => {
  const planning = planningByWorkspace.value[workspace.id];
  if (!planning) return [];
  const goal = planning.goals.find((item) => item.id === planning.focus_goal_id);
  const plan = planning.plans.find((item) => item.id === planning.focus_plan_id);
  if (!goal && !plan) return [];
  if (plan) {
    const completed = plan.steps.filter((step) => ["completed", "skipped"].includes(step.status)).length;
    const total = plan.steps.length;
    return [{
      workspace,
      title: plan.title,
      detail: goal?.title ?? plan.objective,
      progress: total === 0 ? 0 : Math.round((completed / total) * 100),
      progressLabel: `${completed} / ${total} Steps`,
      mode: planning.mode.toUpperCase(),
    }];
  }
  const completed = goal!.success_criteria.filter((criterion) => criterion.completed).length;
  const total = goal!.success_criteria.length;
  return [{
    workspace,
    title: goal!.title,
    detail: goal!.objective,
    progress: total === 0 ? 0 : Math.round((completed / total) * 100),
    progressLabel: `${completed} / ${total} Criteria`,
    mode: planning.mode.toUpperCase(),
  }];
}));

const primaryFocus = computed(() => focusItems.value.find((item) => item.workspace.id === lastWorkspaceId.value) ?? focusItems.value[0] ?? null);

const attentionItems = computed<AttentionItem[]>(() => {
  const items: AttentionItem[] = [];
  for (const workspace of orderedWorkspaces.value) {
    const planning = planningByWorkspace.value[workspace.id];
    const usage = usageByWorkspace.value[workspace.id] ?? [];
    const reviews = planning
      ? planning.goals.filter((goal) => goal.status === "awaiting_acceptance").length
        + planning.plans.filter((plan) => plan.status === "awaiting_acceptance").length
      : 0;
    if (reviews > 0) items.push({ workspace, title: `${reviews} 项 Planning 等待验收`, detail: "完成后需要人工确认才能归档。", level: "warning" });
    if (planning?.execution.last_error) items.push({ workspace, title: "最近执行存在错误", detail: planning.execution.last_error, level: "error" });
    if (planning && ["blocked", "failed", "error"].includes(planning.execution.state.toLowerCase()) && !planning.execution.last_error) {
      items.push({ workspace, title: `Execution ${planning.execution.state}`, detail: "执行链路需要恢复或重新验证。", level: "warning" });
    }
    const errors = usage.reduce((sum, item) => sum + item.errorCount, 0);
    if (errors > 0) items.push({ workspace, title: `${formatCount(errors)} 次工具请求错误`, detail: "MCP 使用统计检测到失败请求。", level: "warning" });
  }
  return items.slice(0, 6);
});

const usageRanking = computed(() => {
  const rows = orderedWorkspaces.value
    .map((workspace) => ({
      workspace,
      tokens: (usageByWorkspace.value[workspace.id] ?? []).reduce((sum, item) => sum + item.estimatedTokens, 0),
    }))
    .sort((a, b) => b.tokens - a.tokens);
  const max = Math.max(rows[0]?.tokens ?? 0, 1);
  return rows.map((row) => ({ ...row, percentage: Math.round((row.tokens / max) * 100) }));
});

const requestSuccessRate = computed(() => {
  if (usageTotals.value.requestCount === 0) return 100;
  const successful = Math.max(0, usageTotals.value.requestCount - usageTotals.value.errorCount);
  return Math.max(0, Math.min(100, (successful / usageTotals.value.requestCount) * 100));
});

const sessionRouting = computed(() => {
  const total = globalOverview.value.sessionCount;
  const bound = globalOverview.value.sessions.filter((session) => Boolean(session.workspaceId)).length;
  const counts = new Map<string, { id: string; label: string; value: number }>();
  for (const session of globalOverview.value.sessions) {
    const id = session.workspaceId || "unbound";
    const label = session.workspaceId ? session.workspaceName : "未选择 Workspace";
    const current = counts.get(id) ?? { id, label, value: 0 };
    current.value += 1;
    counts.set(id, current);
  }
  const rows = [...counts.values()]
    .sort((a, b) => b.value - a.value)
    .slice(0, 4)
    .map((row) => ({
      ...row,
      percentage: total > 0 ? Math.round((row.value / total) * 100) : 0,
    }));
  return {
    total,
    bound,
    unbound: Math.max(0, total - bound),
    boundRate: total > 0 ? Math.round((bound / total) * 100) : 0,
    rows,
  };
});

const planningCompletion = computed(() => {
  let completed = 0;
  let total = 0;
  for (const workspace of orderedWorkspaces.value) {
    const planning = planningByWorkspace.value[workspace.id];
    if (!planning) continue;
    const plan = planning.plans.find((item) => item.id === planning.focus_plan_id);
    if (plan) {
      total += plan.steps.length;
      completed += plan.steps.filter((step) => ["completed", "skipped"].includes(step.status)).length;
      continue;
    }
    const goal = planning.goals.find((item) => item.id === planning.focus_goal_id);
    if (!goal) continue;
    total += goal.success_criteria.length;
    completed += goal.success_criteria.filter((criterion) => criterion.completed).length;
  }
  return {
    completed,
    total,
    percentage: total > 0 ? Math.round((completed / total) * 100) : 0,
  };
});

const planningModeRows = computed(() => {
  const rows = [
    { key: "direct", label: "Direct", value: planningStats.value.modes.direct },
    { key: "plan", label: "Plan", value: planningStats.value.modes.plan },
    { key: "goal", label: "Goal", value: planningStats.value.modes.goal },
  ];
  const total = Math.max(rows.reduce((sum, row) => sum + row.value, 0), 1);
  return rows.map((row) => ({ ...row, percentage: Math.round((row.value / total) * 100) }));
});

const recentActivities = computed<ActivityItem[]>(() => {
  const items: ActivityItem[] = [];
  for (const workspace of orderedWorkspaces.value) {
    for (const session of historyByWorkspace.value[workspace.id] ?? []) {
      items.push({
        workspace,
        title: session.title || workspace.name,
        detail: session.latest_focus || session.snippets.at(-1)?.text || "History session updated",
        timestamp: parseTimestamp(session.updated_at ?? session.created_at),
      });
    }
  }
  return items.sort((a, b) => b.timestamp - a.timestamp).slice(0, 8);
});

const commandEntries = computed(() => {
  const query = commandQuery.value.trim().toLowerCase();
  const entries = [
    { label: "添加工作区", hint: "Workspace", run: () => window.dispatchEvent(new CustomEvent("coding-tools:add-workspace")) },
    { label: "打开通用设置", hint: "Settings", run: () => router.push("/settings/general") },
    { label: "启动 Global MCP", hint: "Runtime", run: () => void setGlobalRuntime(true) },
    { label: "停止 Global MCP", hint: "Runtime", run: () => void setGlobalRuntime(false) },
    ...orderedWorkspaces.value.map((workspace) => ({ label: `打开 ${workspace.name}`, hint: "Workspace", run: () => openWorkspace(workspace.id) })),
  ];
  return query ? entries.filter((entry) => `${entry.label} ${entry.hint}`.toLowerCase().includes(query)) : entries;
});

function moduleVisible(moduleId: DashboardModuleId) {
  return !dashboardPreferences.value.hiddenModules.includes(moduleId);
}

function parseTimestamp(value?: string | null): number {
  if (!value) return 0;
  const numeric = Number(value);
  if (Number.isFinite(numeric)) return numeric < 1_000_000_000_000 ? numeric * 1000 : numeric;
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? 0 : parsed;
}

function formatRelativeTime(timestamp: number): string {
  if (!timestamp) return "—";
  const minutes = Math.floor(Math.max(0, Date.now() - timestamp) / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours}h` : `${Math.floor(hours / 24)}d`;
}

function workspaceUsageTokens(workspaceId: string) {
  return (usageByWorkspace.value[workspaceId] ?? []).reduce((sum, item) => sum + item.estimatedTokens, 0);
}

function planningSummary(workspaceId: string): string {
  const planning = planningByWorkspace.value[workspaceId];
  if (!planning) return "—";
  const goal = planning.goals.find((item) => item.id === planning.focus_goal_id);
  const plan = planning.plans.find((item) => item.id === planning.focus_plan_id);
  return goal?.title ?? plan?.title ?? planning.mode.toUpperCase();
}

async function loadPlanning(items: WorkspaceProfile[]) {
  const generation = ++planningGeneration;
  if (items.length === 0) {
    planningByWorkspace.value = {};
    return;
  }
  const next = await loadPlanningByWorkspace(items);
  if (generation !== planningGeneration) return;
  const current = planningByWorkspace.value;
  const currentIds = Object.keys(current).sort();
  const nextIds = Object.keys(next).sort();
  const unchanged = currentIds.length === nextIds.length
    && currentIds.every((id, index) => id === nextIds[index]
      && current[id]?.revision === next[id]?.revision);
  if (!unchanged) planningByWorkspace.value = next;
}

async function loadUsage(items: WorkspaceProfile[]) {
  const generation = ++usageGeneration;
  if (items.length === 0) {
    usageByWorkspace.value = {};
    usageHistory.value = [];
    usageWorkspaceKey.value = "";
    return;
  }
  const workspaceKey = items.map((item) => item.id).sort().join("|");
  if (workspaceKey !== usageWorkspaceKey.value) {
    usageWorkspaceKey.value = workspaceKey;
    usageHistory.value = [];
  }
  const next = await loadUsageByWorkspace(items);
  if (generation !== usageGeneration) return;
  usageByWorkspace.value = next;
  usageHistory.value = [...usageHistory.value, buildUsagePoint(next, usageHistory.value.at(-1))].slice(-24);
}

async function loadHistory(items: WorkspaceProfile[]) {
  const generation = ++historyGeneration;
  const entries = await Promise.all(items.map(async (workspace) => {
    try {
      const catalog = await listHistorySessions(workspace.id);
      return [workspace.id, catalog.sessions.slice(0, 5)] as const;
    } catch {
      return [workspace.id, []] as const;
    }
  }));
  if (generation === historyGeneration) historyByWorkspace.value = Object.fromEntries(entries);
}

function openWorkspace(id: string) {
  commandOpen.value = false;
  void router.push(`/workspace/${id}`);
}

async function toggleGlobalMcp() {
  if (globalRuntimeBusy.value) return;
  const wasRunning = globalMcpRuntimeState.value === "running";
  globalRuntimeBusy.value = true;
  try {
    const status = await runServiceToggle(wasRunning, () => startRuntime(), () => stopRuntime(), "Global MCP");
    if (status) globalMcpRuntimeState.value = status.state;
  } finally {
    globalRuntimeBusy.value = false;
  }
}

async function setGlobalRuntime(start: boolean) {
  commandOpen.value = false;
  const running = globalMcpRuntimeState.value === "running";
  if (start !== running) await toggleGlobalMcp();
}

async function copyWorkspacePath(id: string, path: string) {
  try {
    await navigator.clipboard.writeText(path);
    copiedPathId.value = id;
    window.setTimeout(() => {
      if (copiedPathId.value === id) copiedPathId.value = null;
    }, 1500);
    showToast("工作区路径已复制", { kind: "success", duration: 1800 });
  } catch {
    showToast("复制路径失败", { kind: "error" });
  }
}

async function revealDirectory(path: string) {
  try {
    await openWorkspaceDirectory(path);
  } catch (error) {
    showToast(`打开目录失败: ${error instanceof Error ? error.message : String(error)}`, { kind: "error" });
  }
}

function runCommand(run: () => void | Promise<void>) {
  commandOpen.value = false;
  commandQuery.value = "";
  void run();
}

function handleKeydown(event: KeyboardEvent) {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    commandOpen.value = !commandOpen.value;
    commandQuery.value = "";
  } else if (event.key === "Escape") {
    commandOpen.value = false;
    preferencesOpen.value = false;
  }
}

watch(
  workspaces,
  (items) => {
    void loadPlanning(items);
    void loadUsage(items);
    void loadHistory(items);
  },
  { immediate: true, deep: true },
);

onMounted(() => {
  loadDashboardPreferences();
  planningTimer = window.setInterval(() => {
    if (document.visibilityState === "visible") void loadPlanning(workspaces.value);
  }, 3000);
  usageTimer = window.setInterval(() => {
    void loadUsage(workspaces.value);
    void loadGlobalOverview();
  }, 5000);
  historyTimer = window.setInterval(() => void loadHistory(workspaces.value), 30_000);
  healthTimer = window.setInterval(() => {
    if (document.visibilityState === "visible") void loadGlobalHealth();
  }, 10_000);
  void getLastWorkspaceId().then((id) => {
    lastWorkspaceId.value = id ?? "";
  }).catch(() => {
    lastWorkspaceId.value = "";
  });
  void getRuntimeStatus().then((status) => {
    globalMcpRuntimeState.value = status.state;
  }).catch(() => {
    globalMcpRuntimeState.value = "stopped";
  });
  void getGlobalGatewayConfig().then((config) => Object.assign(gatewayConfig, config)).catch(() => undefined);
  void loadGlobalOverview();
  void loadGlobalHealth();
  window.addEventListener("keydown", handleKeydown);
});

onUnmounted(() => {
  window.clearInterval(planningTimer);
  window.clearInterval(usageTimer);
  window.clearInterval(historyTimer);
  window.clearInterval(healthTimer);
  window.removeEventListener("keydown", handleKeydown);
});
</script>

<template>
  <section class="page-scroll wb-dashboard" :data-density="dashboardPreferences.density">
    <header class="page-header wb-dashboard-header">
      <div>
        <div class="wb-dashboard-title-row">
          <div class="wb-dashboard-icon">
            <LayoutDashboard :size="18" />
          </div>
          <div>
            <h1 class="wb-dashboard-title font-display text-2xl tracking-tight">工作台</h1>
            <p class="wb-dashboard-subtitle">查看正在执行的 AI 工作、进度、验证结果与需要你处理的风险。</p>
          </div>
        </div>
      </div>
      <div class="wb-header-actions">
        <button class="wb-command-button ios-glass btn-hover" type="button" @click="commandOpen = true; commandQuery = ''">
          <Command :size="14" />
          <span>Quick Actions</span>
          <span class="wb-kbd">⌘K</span>
        </button>
        <DashboardPreferencesPopover
          :open="preferencesOpen"
          :density="dashboardPreferences.density"
          :hidden-modules="dashboardPreferences.hiddenModules"
          @toggle-open="preferencesOpen = !preferencesOpen"
          @toggle-density="updateDashboardPreferences((current) => ({ ...current, density: current.density === 'compact' ? 'comfortable' : 'compact' }))"
          @toggle-module="toggleDashboardModule"
        />
      </div>
    </header>

    <div class="page-body wb-dashboard-main pb-14">
      <div v-if="workspaceCount === 0" class="wb-empty-state wb-surface card-hover mx-auto mt-16 max-w-xl px-8 py-12 text-center animate-fade-in-up">
        <div class="mx-auto grid h-16 w-16 place-items-center rounded-[24px] bg-[var(--accent-gradient)] text-white shadow-lg">
          <GitBranch :size="28" />
        </div>
        <h2 class="mt-5 text-lg font-semibold font-display">还没有工作区</h2>
        <p class="mt-2 text-xs leading-5 text-[var(--text-secondary)]">添加本地项目后，ChatGPT 可通过同一个 Global MCP 连接选择并操作不同 Workspace。</p>
        <button class="wb-primary-button btn-hover mt-5" type="button" @click="requestAddWorkspace">添加工作区</button>
      </div>

      <template v-else>
        <section v-if="moduleVisible('focus')" class="wb-hero wb-surface card-hover animate-fade-in-up delay-150">
          <div class="wb-focus">
            <div class="wb-eyebrow text-[var(--coral-primary,#e2574c)] font-display"><Sparkles :size="13" /> 当前执行</div>
            <template v-if="primaryFocus">
              <h2 class="font-display">{{ primaryFocus.title }}</h2>
              <p>{{ primaryFocus.detail }}</p>
              <div class="wb-progress"><span :style="{ width: `${primaryFocus.progress}%` }" /></div>
              <div class="wb-focus-meta">
                <span>{{ primaryFocus.workspace.name }}</span><span>{{ primaryFocus.mode }}</span>
                <span>{{ primaryFocus.progressLabel }}</span><span>{{ planningByWorkspace[primaryFocus.workspace.id]?.execution.state ?? 'idle' }}</span>
              </div>
              <div class="wb-focus-actions">
                <button class="wb-primary-button btn-hover" type="button" @click="openWorkspace(primaryFocus.workspace.id)">继续工作 <ArrowUpRight :size="13" /></button>
                <button class="wb-soft-button btn-hover" type="button" @click="toggleGlobalMcp">
                  <RotateCw v-if="globalRuntimeBusy" :size="12" class="animate-spin" />
                  <template v-else-if="globalMcpRuntimeState === 'running'"><Square :size="11" /> 停止 Global MCP</template>
                  <template v-else><Play :size="11" /> 启动 Global MCP</template>
                </button>
              </div>
            </template>
            <template v-else>
              <h2 class="font-display">选择一个工作区开始</h2>
              <p>当前没有聚焦的 Goal 或 Plan。你仍然可以从下面的工作区继续工作。</p>
            </template>
          </div>
          <div class="wb-health-panel">
            <div class="wb-health-ring" :style="{ '--health-angle': `${serviceHealth * 3.6}deg` }">
              <div class="wb-health-ring-content"><strong class="font-display">{{ serviceHealth }}%</strong><span>Global MCP</span><small>{{ globalMcpRuntimeState }}</small></div>
            </div>
            <div class="wb-health-summary">
              <div><span>Executing</span><strong class="font-display">{{ executionStats.running }}</strong></div>
              <div><span>Verified</span><strong class="font-display">{{ executionStats.verified }}</strong></div>
              <div :class="{ alert: executionStats.blocked > 0 }"><span>Blocked</span><strong class="font-display">{{ executionStats.blocked }}</strong></div>
            </div>
            <div class="wb-health-connections">
              <span :class="{ 'animate-pulse-glow': globalMcpRuntimeState === 'running' }">{{ globalMcpRuntimeState === 'running' ? 'MCP Online' : 'MCP Offline' }}</span>
              <span>{{ globalOverview.sessionCount }} Sessions</span>
              <span>{{ executionStats.changedFiles }} Changed Files</span>
            </div>
          </div>
        </section>

        <div class="wb-stat-strip mt-5 animate-fade-in-up delay-300">
          <div class="wb-stat wb-surface wb-stat--blue card-hover">
            <div class="wb-stat-head"><span class="font-display">AI Executions</span><i><Boxes :size="16" /></i></div>
            <strong class="font-display">{{ executionStats.running }}</strong><small><b>{{ executionStats.changedFiles }}</b> 个执行变更文件</small>
          </div>
          <div class="wb-stat wb-surface wb-stat--indigo card-hover">
            <div class="wb-stat-head"><span class="font-display">MCP Tokens</span><i><Cpu :size="16" /></i></div>
            <strong class="font-display">{{ formatCount(usageTotals.estimatedTokens) }}</strong><small><b>{{ formatCount(usageTotals.toolCallCount) }}</b> 次工具调用</small>
          </div>
          <div class="wb-stat wb-surface wb-stat--purple card-hover">
            <div class="wb-stat-head"><span class="font-display">Active Goals</span><i><ListChecks :size="16" /></i></div>
            <strong class="font-display">{{ planningStats.activeGoals }}</strong><small><b>{{ planningStats.activePlans }}</b> 个 Plan 进行中</small>
          </div>
          <div class="wb-stat wb-surface card-hover" :class="planningStats.pendingReview > 0 || errorServices > 0 ? 'wb-stat--orange' : 'wb-stat--green'">
            <div class="wb-stat-head"><span class="font-display">Verification</span><i><ShieldCheck :size="16" /></i></div>
            <strong class="font-display">{{ executionStats.verified }}</strong><small>{{ executionStats.blocked > 0 ? `${executionStats.blocked} 个执行需要处理` : `${planningStats.pendingReview} 项等待人工验收` }}</small>
          </div>
        </div>

        <section class="wb-visual-overview mt-5 animate-fade-in-up delay-500">
          <article class="wb-overview-card wb-overview-card--flow wb-surface card-hover">
            <div class="wb-overview-head">
              <div>
                <span class="wb-overview-kicker"><Activity :size="12" /> 实时流量</span>
                <h3>Token Flow</h3>
              </div>
              <span class="wb-overview-live" :class="{ active: usageChart.latest > 0 }">
                <i />{{ usageChart.latest > 0 ? 'LIVE' : 'IDLE' }}
              </span>
            </div>
            <div class="wb-overview-flow-meta">
              <div><strong>+{{ formatCount(usageChart.latest) }}</strong><span>最近 5 秒采样</span></div>
              <div><strong>{{ usageTotals.requestCount > 0 ? `${requestSuccessRate.toFixed(requestSuccessRate >= 99 ? 1 : 0)}%` : '—' }}</strong><span>请求成功率</span></div>
            </div>
            <div class="wb-overview-spark-shell">
              <svg v-if="usageChart.points.length > 1" viewBox="0 0 100 40" preserveAspectRatio="none" class="wb-overview-spark">
                <defs>
                  <linearGradient id="dashboardOverviewArea" x1="0" x2="0" y1="0" y2="1">
                    <stop offset="0" stop-color="var(--coral-accent, #E5665B)" stop-opacity=".35" />
                    <stop offset="1" stop-color="var(--coral-primary, #e2574c)" stop-opacity="0" />
                  </linearGradient>
                  <linearGradient id="dashboardOverviewLine" x1="0" x2="1" y1="0" y2="0">
                    <stop offset="0" stop-color="var(--coral-400)" />
                    <stop offset=".55" stop-color="var(--coral-700)" />
                    <stop offset="1" stop-color="var(--coral-600)" />
                  </linearGradient>
                </defs>
                <line x1="0" x2="100" y1="27" y2="27" class="wb-overview-gridline" />
                <line x1="0" x2="100" y1="16" y2="16" class="wb-overview-gridline" />
                <path :d="usageChart.areaPath" fill="url(#dashboardOverviewArea)" />
                <path :d="usageChart.path" fill="none" stroke="url(#dashboardOverviewLine)" class="wb-overview-spark-line" />
              </svg>
              <div v-else class="wb-overview-empty">等待更多实时采样…</div>
            </div>
            <div class="wb-overview-foot">
              <span><b>{{ formatCount(usageTotals.estimatedTokens) }}</b> 总 Tokens</span>
              <span><b>{{ formatCount(usageTotals.toolCallCount) }}</b> Tool Calls</span>
              <span><b>{{ formatCount(usageTotals.errorCount) }}</b> Errors</span>
            </div>
          </article>

          <article class="wb-overview-card wb-surface card-hover">
            <div class="wb-overview-head">
              <div>
                <span class="wb-overview-kicker font-display"><GitBranch :size="12" /> Session 路由</span>
                <h3 class="font-display">Chat → Workspace</h3>
              </div>
              <strong class="wb-overview-head-value font-display">{{ sessionRouting.total }}</strong>
            </div>
            <div class="wb-ring-layout">
              <div class="wb-mini-ring" :style="{ '--ring-angle': `${sessionRouting.boundRate * 3.6}deg` }">
                <div><strong class="font-display">{{ sessionRouting.boundRate }}%</strong><span>已绑定</span></div>
              </div>
              <div class="wb-overview-bars">
                <div v-if="sessionRouting.rows.length === 0" class="wb-overview-empty">尚无活跃 Session</div>
                <div v-for="row in sessionRouting.rows" v-else :key="row.id" class="wb-overview-bar-row">
                  <div><span :title="row.label">{{ row.label }}</span><strong class="font-display">{{ row.value }}</strong></div>
                  <div class="wb-overview-bar-track"><i :style="{ width: `${row.percentage}%` }" /></div>
                </div>
              </div>
            </div>
            <div class="wb-overview-foot">
              <span><b>{{ sessionRouting.bound }}</b> 已选 Workspace</span>
              <span :class="{ warning: sessionRouting.unbound > 0 }"><b>{{ sessionRouting.unbound }}</b> 未选择</span>
            </div>
          </article>

          <article class="wb-overview-card wb-surface card-hover">
            <div class="wb-overview-head">
              <div>
                <span class="wb-overview-kicker font-display"><ListChecks :size="12" /> Planning</span>
                <h3 class="font-display">Focus 完成度</h3>
              </div>
              <strong class="wb-overview-head-value font-display">{{ planningCompletion.completed }}/{{ planningCompletion.total }}</strong>
            </div>
            <div class="wb-ring-layout">
              <div class="wb-mini-ring wb-mini-ring--purple" :style="{ '--ring-angle': `${planningCompletion.percentage * 3.6}deg` }">
                <div><strong class="font-display">{{ planningCompletion.percentage }}%</strong><span>Completed</span></div>
              </div>
              <div class="wb-overview-bars">
                <div v-for="row in planningModeRows" :key="row.key" class="wb-overview-bar-row" :class="`is-${row.key}`">
                  <div><span>{{ row.label }}</span><strong class="font-display">{{ row.value }}</strong></div>
                  <div class="wb-overview-bar-track"><i :style="{ width: `${row.percentage}%` }" /></div>
                </div>
              </div>
            </div>
            <div class="wb-overview-foot">
              <span><b>{{ planningStats.activeGoals }}</b> Active Goals</span>
              <span><b>{{ planningStats.activePlans }}</b> Active Plans</span>
              <span :class="{ warning: planningStats.pendingReview > 0 }"><b>{{ planningStats.pendingReview }}</b> Review</span>
            </div>
          </article>

          <article class="wb-overview-card wb-surface card-hover">
            <div class="wb-overview-head">
              <div>
                <span class="wb-overview-kicker font-display"><Boxes :size="12" /> Workspace 负载</span>
                <h3 class="font-display">Token Distribution</h3>
              </div>
              <strong class="wb-overview-head-value font-display">{{ workspaceCount }}</strong>
            </div>
            <div class="wb-overview-workspace-bars">
              <div v-if="usageRanking.length === 0" class="wb-overview-empty">暂无 Workspace 用量</div>
              <div v-for="row in usageRanking.slice(0, 4)" v-else :key="row.workspace.id" class="wb-overview-workspace-row">
                <div><span :title="row.workspace.name">{{ row.workspace.name }}</span><strong class="font-display">{{ formatCount(row.tokens) }}</strong></div>
                <div class="wb-overview-workspace-track"><i :style="{ width: `${row.percentage}%` }" /></div>
              </div>
            </div>
            <div class="wb-overview-foot">
              <span><b>{{ usageRanking[0]?.workspace.name ?? '—' }}</b> Top Workspace</span>
              <span><b>{{ formatCount(usageRanking[0]?.tokens ?? 0) }}</b> Tokens</span>
            </div>
          </article>
        </section>

        <section v-if="moduleVisible('attention') && attentionItems.length" class="wb-section wb-surface card-hover animate-fade-in-up delay-700">
          <div class="wb-section-heading"><div><h3 class="font-display">需要关注</h3><p>只显示真正需要你处理的异常、错误和人工验收。</p></div><AlertTriangle :size="15" class="text-[var(--warning)]" /></div>
          <div class="wb-attention-list">
            <button v-for="item in attentionItems" :key="`${item.workspace.id}-${item.title}`" class="wb-attention-row" type="button" @click="openWorkspace(item.workspace.id)">
              <X v-if="item.level === 'error'" :size="14" class="text-[var(--danger)]" /><AlertTriangle v-else :size="14" class="text-[var(--warning)]" />
              <div class="min-w-0"><strong>{{ item.workspace.name }} · {{ item.title }}</strong><span class="truncate">{{ item.detail }}</span></div><ArrowUpRight :size="12" />
            </button>
          </div>
        </section>

        <section v-if="moduleVisible('workspaces')" class="wb-section wb-surface card-hover animate-fade-in-up delay-700">
          <div class="wb-section-heading"><div><h3 class="font-display">工作区</h3><p>所有项目共享同一个 MCP 连接；这里展示各自独立的项目策略、Planning 与用量。</p></div><span class="text-[10px] text-[var(--text-muted)] font-mono">{{ workspaceCount }} Workspaces</span></div>
          <div class="wb-workspace-list">
            <div class="wb-workspace-header"><span>Workspace</span><span>Context</span><span>Planning</span><span>Tokens</span><span /></div>
            <div v-for="workspace in orderedWorkspaces" :key="workspace.id" class="wb-workspace-row card-hover" :class="{ 'is-pinned': dashboardPreferences.pinnedWorkspaceIds.includes(workspace.id) }">
              <div class="wb-workspace-name"><button type="button" class="truncate font-display font-medium" @click="openWorkspace(workspace.id)">{{ workspace.name }}</button><small :title="workspace.path" class="font-mono">{{ workspace.path }}</small></div>
              <div class="wb-runtime-pill"><span class="wb-runtime-dot is-running" /><span>{{ workspace.runtime.tool_profile }}</span><span class="font-mono text-[9px]">isolated</span></div>
              <div class="wb-mode-pill min-w-0"><GitBranch :size="11" /><span class="truncate" :title="planningSummary(workspace.id)">{{ planningSummary(workspace.id) }}</span></div>
              <div class="wb-workspace-token font-mono">{{ formatCount(workspaceUsageTokens(workspace.id)) }}</div>
              <div class="wb-workspace-actions">
                <button class="wb-icon-button btn-hover !h-7 !w-7 !min-h-7" type="button" @click="togglePinnedWorkspace(workspace.id)"><PinOff v-if="dashboardPreferences.pinnedWorkspaceIds.includes(workspace.id)" :size="11" /><Pin v-else :size="11" /></button>
                <button class="wb-icon-button btn-hover !h-7 !w-7 !min-h-7" type="button" @click="moveWorkspace(workspace.id, -1, orderedWorkspaces.map((item) => item.id))"><ChevronUp :size="11" /></button>
                <button class="wb-icon-button btn-hover !h-7 !w-7 !min-h-7" type="button" @click="moveWorkspace(workspace.id, 1, orderedWorkspaces.map((item) => item.id))"><ChevronDown :size="11" /></button>
                <button class="wb-icon-button btn-hover !h-7 !w-7 !min-h-7" type="button" @click="revealDirectory(workspace.path)"><FolderOpen :size="11" /></button>
                <button class="wb-icon-button btn-hover !h-7 !w-7 !min-h-7" type="button" @click="copyWorkspacePath(workspace.id, workspace.path)"><Check v-if="copiedPathId === workspace.id" :size="11" class="text-[var(--success)]" /><Copy v-else :size="11" /></button>
                <button class="wb-icon-button btn-hover !h-7 !w-7 !min-h-7" type="button" @click="openWorkspace(workspace.id)"><ArrowUpRight :size="11" /></button>
              </div>
            </div>
          </div>
        </section>

        <div class="wb-grid-two animate-fade-in-up delay-900">
          <section v-if="moduleVisible('activity')" class="wb-section wb-surface card-hover">
            <div class="wb-section-heading"><div><h3 class="font-display">最近活动</h3><p>汇总各工作区最近的 History Session。</p></div><Activity :size="14" /></div>
            <div v-if="recentActivities.length" class="wb-activity-list">
              <button
                v-for="item in recentActivities"
                :key="`${item.workspace.id}-${item.timestamp}-${item.title}`"
                class="wb-activity-row"
                type="button"
                @click="openWorkspace(item.workspace.id)"
              >
                <div class="wb-activity-icon">
                  <Activity :size="13" />
                </div>
                <div class="min-w-0 flex-1">
                  <div class="wb-activity-head">
                    <span class="wb-activity-workspace-badge">{{ item.workspace.name }}</span>
                    <strong class="wb-activity-title">{{ item.title }}</strong>
                  </div>
                  <p class="wb-activity-detail">{{ item.detail }}</p>
                </div>
                <div class="wb-activity-meta">
                  <span v-if="formatRelativeTime(item.timestamp) !== '—'" class="wb-activity-time">
                    {{ formatRelativeTime(item.timestamp) }}
                  </span>
                  <ArrowUpRight :size="12" class="wb-activity-arrow" />
                </div>
              </button>
            </div>
            <div v-else class="wb-empty-inline">还没有可汇总的 History Session。</div>
          </section>

          <section v-if="moduleVisible('health')" class="wb-section wb-surface card-hover">
            <div class="wb-section-heading"><div><h3 class="font-display">系统健康</h3><p>Global MCP 与公网入口属于全局运行时，不绑定任何单个 Workspace。</p></div><Gauge :size="14" /></div>
            <div v-if="globalHealth.length" class="wb-global-health-grid mb-3">
              <div
                v-for="item in globalHealth"
                :key="item.label"
                class="wb-health-tile"
              >
                <div class="flex items-center justify-between gap-3">
                  <strong class="text-xs font-semibold text-[var(--text-main)] font-display">{{ item.label }}</strong>
                  <span
                    class="inline-flex items-center gap-1.5 text-[10px] font-semibold px-2 py-0.5 rounded-full"
                    :class="item.ok
                      ? 'bg-[#30d158]/15 text-[#30d158] border border-[#30d158]/30 shadow-[0_0_8px_rgba(48,209,88,0.25)]'
                      : 'bg-[#ff453a]/15 text-[#ff453a] border border-[#ff453a]/30 shadow-[0_0_8px_rgba(255,69,58,0.25)]'"
                  >
                    <span class="h-1.5 w-1.5 rounded-full" :class="item.ok ? 'bg-[#30d158]' : 'bg-[#ff453a] animate-pulse'" />
                    {{ item.ok ? '正常' : '异常' }}
                  </span>
                </div>
                <p class="mt-1.5 text-[11px] leading-relaxed text-[var(--text-secondary)]">{{ item.detail }}</p>
                <p v-if="item.hint" class="mt-1 text-[10px] leading-4 text-[var(--text-muted)] font-mono">{{ item.hint }}</p>
              </div>
            </div>
            <div class="wb-health-overview">
              <div class="wb-mix-chart">
                <div class="wb-mix-head"><span class="font-display">Global MCP</span><strong class="font-display">{{ globalMcpRuntimeState }}</strong></div>
                <div class="wb-mix-track">
                  <i v-for="item in runtimeMix" :key="item.key" :class="`mix-${item.key}`" :style="{ flexGrow: item.value }" />
                </div>
                <div class="wb-mix-legend"><span v-for="item in runtimeMix" :key="item.key"><i :class="`mix-${item.key}`" />{{ item.label }} <strong>{{ item.value }}</strong></span></div>
              </div>
              <div class="wb-mix-chart">
                <div class="wb-mix-head"><span class="font-display">唯一连接方式</span><strong class="font-display">1 Endpoint</strong></div>
                <div class="wb-mix-track">
                  <i v-for="item in connectionMix" :key="item.key" :class="`mix-${item.key}`" :style="{ flexGrow: item.value }" />
                </div>
                <div class="wb-mix-legend"><span v-for="item in connectionMix" :key="item.key"><i :class="`mix-${item.key}`" />{{ item.label }} <strong>{{ item.value }}</strong></span></div>
              </div>
              <div class="wb-mix-chart">
                <div class="wb-mix-head"><span class="font-display">Planning 模式</span><strong class="font-display">{{ planningStats.activeGoals }} Goals</strong></div>
                <div class="wb-mix-track">
                  <i v-for="item in planningModeMix" :key="item.key" :class="`mix-${item.key}`" :style="{ flexGrow: item.value }" />
                </div>
                <div class="wb-mix-legend"><span v-for="item in planningModeMix" :key="item.key"><i :class="`mix-${item.key}`" />{{ item.label }} <strong>{{ item.value }}</strong></span></div>
              </div>
              <div class="wb-health-signal" :class="errorServices === 0 && usageTotals.errorCount === 0 ? 'healthy' : 'warning'">
                <div><span>Quality Signal</span><strong class="font-display">{{ errorServices === 0 && usageTotals.errorCount === 0 ? "Passed" : "Needs Attention" }}</strong></div>
                <small>{{ formatCount(usageTotals.errorCount) }} Tool Errors · {{ planningStats.pendingReview }} Waiting Review</small>
              </div>
              <div class="wb-mix-chart">
                <div class="wb-mix-head"><span class="font-display">Chat Sessions</span><strong class="font-display">{{ globalOverview.sessionCount }}</strong></div>
                <div v-if="globalOverview.sessions.length" class="mt-2 space-y-1.5">
                  <div v-for="session in globalOverview.sessions.slice(0, 4)" :key="session.sessionId" class="flex items-center justify-between gap-3 text-[10px]">
                    <span class="min-w-0 truncate text-[var(--text-secondary)]">{{ session.workspaceName }}</span>
                    <code class="shrink-0 text-[9px] text-[var(--text-muted)] font-mono">{{ session.sessionId.slice(0, 8) }}</code>
                  </div>
                </div>
                <div v-else class="mt-2 text-[10px] text-[var(--text-muted)]">尚无活跃 Chat Session</div>
              </div>
            </div>
          </section>
        </div>

        <section v-if="moduleVisible('usage')" class="wb-section wb-surface wb-analytics-section card-hover animate-fade-in-up delay-1100">
          <div class="wb-section-heading"><div><h3 class="font-display">Token Analytics</h3><p>MCP JSON 传输量估算，用于观察工具调用趋势。</p></div><Zap :size="14" /></div>
          <div class="wb-grid-two">
            <DashboardUsagePanel :totals="usageTotals" :average-tokens="averageTokens" :chart="usageChart" />
            <div class="wb-ranking">
              <div class="wb-ranking-header">
                <div>
                  <span class="wb-ranking-title font-display">工作区消耗排行</span>
                  <p class="wb-ranking-subtitle">各项目的 Token 占用量与相对比例</p>
                </div>
                <span class="wb-ranking-badge font-mono">{{ orderedWorkspaces.length }} Workspaces</span>
              </div>

              <div class="wb-ranking-list">
                <div v-if="usageRanking.length === 0" class="wb-ranking-empty">
                  暂无工作区用量数据
                </div>
                <div
                  v-for="(row, index) in usageRanking"
                  :key="row.workspace.id"
                  class="wb-rank-row"
                >
                  <div class="wb-rank-meta">
                    <span class="wb-rank-index" :class="{ 'is-top': index === 0 }">{{ index + 1 }}</span>
                    <span class="wb-rank-name" :title="row.workspace.name">{{ row.workspace.name }}</span>
                  </div>
                  <div class="wb-rank-track">
                    <i :style="{ width: `${row.percentage}%` }" />
                  </div>
                  <strong class="wb-rank-val">{{ formatCount(row.tokens) }}</strong>
                </div>
              </div>

              <div class="wb-ranking-stats">
                <div class="wb-stat-pill">
                  <span class="wb-stat-pill-label">Input / Output</span>
                  <strong class="wb-stat-pill-value">{{ formatCount(usageTotals.estimatedInputTokens) }} / {{ formatCount(usageTotals.estimatedOutputTokens) }}</strong>
                </div>
                <div class="wb-stat-pill">
                  <span class="wb-stat-pill-label">Avg / Tool Call</span>
                  <strong class="wb-stat-pill-value">{{ formatCount(averageTokens) }}</strong>
                </div>
                <div class="wb-stat-pill">
                  <span class="wb-stat-pill-label">Requests / Calls</span>
                  <strong class="wb-stat-pill-value">{{ formatCount(usageTotals.requestCount) }} / {{ formatCount(usageTotals.toolCallCount) }}</strong>
                </div>
              </div>
            </div>
          </div>
        </section>
      </template>
    </div>
  </section>

  <DashboardCommandPalette
    v-model:query="commandQuery"
    :open="commandOpen"
    :entries="commandEntries"
    @close="commandOpen = false"
    @select="(index) => commandEntries[index] && runCommand(commandEntries[index].run)"
  />
</template>
