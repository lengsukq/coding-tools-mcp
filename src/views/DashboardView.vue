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
  Search,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Square,
  X,
  Zap,
} from "@lucide/vue";
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { useRouter } from "vue-router";
import DashboardUsagePanel from "$src/components/dashboard/DashboardUsagePanel.vue";
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
  stateClass,
  stateLabel,
  summarizeConnections,
  summarizePlanning,
  summarizeUsage,
  tunnelLabel,
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
import { openWorkspaceDirectory, startRuntime, stopRuntime } from "$lib/api/workspaces";
import { runServiceToggle } from "$lib/runtime/service";
import { showToast } from "$lib/stores/toast";
import { mcpRuntimeStates, workspaces } from "$lib/stores/app";
import type { WorkspaceProfile } from "$lib/types";

interface FocusItem {
  workspace: WorkspaceProfile;
  title: string;
  detail: string;
  progress: number;
  progressLabel: string;
  mode: string;
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
const busyMap = reactive<Record<string, boolean>>({});
const copiedPathId = ref<string | null>(null);
const commandOpen = ref(false);
const commandQuery = ref("");
const preferencesOpen = ref(false);
const commandInput = ref<HTMLInputElement | null>(null);
let planningGeneration = 0;
let usageGeneration = 0;
let historyGeneration = 0;
let usageTimer = 0;
let historyTimer = 0;

const workspaceCount = computed(() => workspaces.value.length);
const orderedWorkspaces = computed(() => {
  const ids = sortWorkspaceIds(workspaces.value.map((item) => item.id), dashboardPreferences.value);
  const byId = new Map(workspaces.value.map((item) => [item.id, item]));
  return ids.map((id) => byId.get(id)).filter((item): item is WorkspaceProfile => Boolean(item));
});
const mcpRunning = computed(() => workspaces.value.filter((item) => mcpRuntimeStates.value[item.id] === "running").length);
const errorServices = computed(() => workspaces.value.filter((item) => mcpRuntimeStates.value[item.id] === "error").length);
const serviceHealth = computed(() => workspaceCount.value === 0 ? 0 : Math.round((mcpRunning.value / workspaceCount.value) * 100));
const planningStats = computed(() => summarizePlanning(planningByWorkspace.value));
const connectionStats = computed(() => summarizeConnections(workspaces.value));
const usageTotals = computed(() => summarizeUsage(usageByWorkspace.value));
const averageTokens = computed(() => usageTotals.value.toolCallCount === 0 ? 0 : usageTotals.value.estimatedToolCallTokens / usageTotals.value.toolCallCount);
const usageChart = computed(() => buildUsageChart(usageHistory.value));
const runtimeMix = computed(() => [
  { key: "running", label: "Running", value: mcpRunning.value },
  { key: "offline", label: "Offline", value: Math.max(0, workspaceCount.value - mcpRunning.value - errorServices.value) },
  { key: "error", label: "Error", value: errorServices.value },
]);
const connectionMix = computed(() => [
  { key: "gateway", label: "Gateway", value: connectionStats.value.gateway },
  { key: "frp", label: "FRP", value: connectionStats.value.frp },
  { key: "cloudflare", label: "Cloudflare", value: connectionStats.value.cloudflare },
  { key: "local", label: "Local", value: connectionStats.value.local },
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
    const runtimeState = mcpRuntimeStates.value[workspace.id];
    const planning = planningByWorkspace.value[workspace.id];
    const usage = usageByWorkspace.value[workspace.id] ?? [];
    if (runtimeState === "error") items.push({ workspace, title: "MCP Runtime 异常", detail: "进入工作区查看诊断和日志。", level: "error" });
    const reviews = planning
      ? planning.goals.filter((goal) => goal.status === "awaiting_acceptance").length
        + planning.plans.filter((plan) => plan.status === "awaiting_acceptance").length
      : 0;
    if (reviews > 0) items.push({ workspace, title: `${reviews} 项 Planning 等待验收`, detail: "完成后需要人工确认才能归档。", level: "warning" });
    if (planning?.execution.last_error) items.push({ workspace, title: "最近执行存在错误", detail: planning.execution.last_error, level: "error" });
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
    { label: "启动全部 MCP", hint: "Runtime", run: () => void setAllRuntime(true) },
    { label: "停止全部 MCP", hint: "Runtime", run: () => void setAllRuntime(false) },
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
  if (generation === planningGeneration) planningByWorkspace.value = next;
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

async function toggleWorkspaceMcp(id: string) {
  if (busyMap[id]) return;
  const wasRunning = mcpRuntimeStates.value[id] === "running";
  busyMap[id] = true;
  try {
    const status = await runServiceToggle(wasRunning, () => startRuntime(id), () => stopRuntime(id), "MCP");
    if (status) mcpRuntimeStates.value = { ...mcpRuntimeStates.value, [id]: status.state };
  } finally {
    busyMap[id] = false;
  }
}

async function setAllRuntime(start: boolean) {
  commandOpen.value = false;
  for (const workspace of orderedWorkspaces.value) {
    const running = mcpRuntimeStates.value[workspace.id] === "running";
    if (start !== running) await toggleWorkspaceMcp(workspace.id);
  }
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

watch(commandOpen, async (open) => {
  if (!open) return;
  await nextTick();
  commandInput.value?.focus();
});

onMounted(() => {
  loadDashboardPreferences();
  usageTimer = window.setInterval(() => void loadUsage(workspaces.value), 5000);
  historyTimer = window.setInterval(() => void loadHistory(workspaces.value), 30_000);
  void getLastWorkspaceId().then((id) => {
    lastWorkspaceId.value = id ?? "";
  }).catch(() => {
    lastWorkspaceId.value = "";
  });
  window.addEventListener("keydown", handleKeydown);
});

onUnmounted(() => {
  window.clearInterval(usageTimer);
  window.clearInterval(historyTimer);
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
            <h1 class="wb-dashboard-title">工作台</h1>
            <p class="wb-dashboard-subtitle">继续上一次 Coding 工作、处理需要关注的状态，并快速控制所有 Workspace Runtime。</p>
          </div>
        </div>
      </div>
      <div class="wb-header-actions">
        <button class="wb-command-button ios-glass" type="button" @click="commandOpen = true; commandQuery = ''">
          <Command :size="14" />
          <span>Quick Actions</span>
          <span class="wb-kbd">⌘K</span>
        </button>
        <div class="wb-preferences-wrap">
          <button class="wb-icon-button ios-glass" type="button" title="工作台偏好" @click="preferencesOpen = !preferencesOpen">
            <SlidersHorizontal :size="14" />
          </button>
          <div v-if="preferencesOpen" class="wb-preferences-popover ios-glass-strong rounded-[20px]">
            <p class="wb-pref-title">工作台偏好</p>
            <div class="wb-pref-row">
              <span>信息密度</span>
              <button type="button" @click="updateDashboardPreferences((current) => ({ ...current, density: current.density === 'compact' ? 'comfortable' : 'compact' }))">
                {{ dashboardPreferences.density === "compact" ? "紧凑" : "舒适" }}
              </button>
            </div>
            <div v-for="item in [
              ['focus', '当前 Focus'], ['attention', '需要关注'], ['workspaces', '工作区'],
              ['activity', '最近活动'], ['usage', 'Token Analytics'], ['health', '系统健康'],
            ]" :key="item[0]" class="wb-pref-row">
              <span>{{ item[1] }}</span>
              <button type="button" @click="toggleDashboardModule(item[0] as DashboardModuleId)">
                {{ moduleVisible(item[0] as DashboardModuleId) ? "隐藏" : "显示" }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </header>

    <div class="page-body wb-dashboard-main pb-14">
      <div v-if="workspaceCount === 0" class="wb-empty-state wb-surface mx-auto mt-16 max-w-xl px-8 py-12 text-center">
        <div class="mx-auto grid h-16 w-16 place-items-center rounded-[24px] bg-[var(--accent-gradient)] text-white shadow-lg">
          <GitBranch :size="28" />
        </div>
        <h2 class="mt-5 text-lg font-semibold">还没有工作区</h2>
        <p class="mt-2 text-xs leading-5 text-[var(--text-secondary)]">添加一个本地项目后，可以在这里管理 MCP Runtime、Planning、History 和使用情况。</p>
        <button class="wb-primary-button mt-5" type="button" @click="requestAddWorkspace">添加工作区</button>
      </div>

      <template v-else>
        <section v-if="moduleVisible('focus')" class="wb-hero wb-surface">
          <div class="wb-focus">
            <div class="wb-eyebrow text-[var(--ios-purple)]"><Sparkles :size="13" /> Continue Working</div>
            <template v-if="primaryFocus">
              <h2>{{ primaryFocus.title }}</h2>
              <p>{{ primaryFocus.detail }}</p>
              <div class="wb-progress"><span :style="{ width: `${primaryFocus.progress}%` }" /></div>
              <div class="wb-focus-meta">
                <span>{{ primaryFocus.workspace.name }}</span><span>{{ primaryFocus.mode }}</span>
                <span>{{ primaryFocus.progressLabel }}</span><span>{{ tunnelLabel(primaryFocus.workspace) }}</span>
              </div>
              <div class="wb-focus-actions">
                <button class="wb-primary-button" type="button" @click="openWorkspace(primaryFocus.workspace.id)">继续工作 <ArrowUpRight :size="13" /></button>
                <button class="wb-soft-button" type="button" @click="toggleWorkspaceMcp(primaryFocus.workspace.id)">
                  <RotateCw v-if="busyMap[primaryFocus.workspace.id]" :size="12" class="animate-spin" />
                  <template v-else-if="mcpRuntimeStates[primaryFocus.workspace.id] === 'running'"><Square :size="11" /> 停止 MCP</template>
                  <template v-else><Play :size="11" /> 启动 MCP</template>
                </button>
              </div>
            </template>
            <template v-else>
              <h2>选择一个工作区开始</h2>
              <p>当前没有聚焦的 Goal 或 Plan。你仍然可以从下面的工作区继续工作。</p>
            </template>
          </div>
          <div class="wb-health-panel">
            <div class="wb-health-ring" :style="{ '--health-angle': `${serviceHealth * 3.6}deg` }">
              <div class="wb-health-ring-content"><strong>{{ serviceHealth }}%</strong><span>Runtime</span><small>Online</small></div>
            </div>
            <div class="wb-health-summary">
              <div><span>Running</span><strong>{{ mcpRunning }}</strong></div>
              <div><span>Offline</span><strong>{{ Math.max(0, workspaceCount - mcpRunning) }}</strong></div>
              <div :class="{ alert: errorServices > 0 }"><span>Errors</span><strong>{{ errorServices }}</strong></div>
            </div>
            <div class="wb-health-connections">
              <span>Gateway {{ connectionStats.gateway }}</span>
              <span>FRP {{ connectionStats.frp }}</span>
              <span>CF {{ connectionStats.cloudflare }}</span>
              <span>Local {{ connectionStats.local }}</span>
            </div>
          </div>
        </section>

        <div class="wb-stat-strip mt-4">
          <div class="wb-stat wb-surface wb-stat--blue">
            <div class="wb-stat-head"><span>Workspaces</span><i><Boxes :size="15" /></i></div>
            <strong>{{ workspaceCount }}</strong><small><b>{{ mcpRunning }}</b> 个 Runtime 在线</small>
          </div>
          <div class="wb-stat wb-surface wb-stat--indigo">
            <div class="wb-stat-head"><span>MCP Tokens</span><i><Cpu :size="15" /></i></div>
            <strong>{{ formatCount(usageTotals.estimatedTokens) }}</strong><small><b>{{ formatCount(usageTotals.toolCallCount) }}</b> 次工具调用</small>
          </div>
          <div class="wb-stat wb-surface wb-stat--purple">
            <div class="wb-stat-head"><span>Active Goals</span><i><ListChecks :size="15" /></i></div>
            <strong>{{ planningStats.activeGoals }}</strong><small><b>{{ planningStats.activePlans }}</b> 个 Plan 进行中</small>
          </div>
          <div class="wb-stat wb-surface" :class="planningStats.pendingReview > 0 || errorServices > 0 ? 'wb-stat--orange' : 'wb-stat--green'">
            <div class="wb-stat-head"><span>Need Review</span><i><ShieldCheck :size="15" /></i></div>
            <strong>{{ planningStats.pendingReview }}</strong><small>{{ errorServices > 0 ? `${errorServices} 个 Runtime 异常` : "运行状态正常" }}</small>
          </div>
        </div>

        <section v-if="moduleVisible('attention') && attentionItems.length" class="wb-section wb-surface">
          <div class="wb-section-heading"><div><h3>需要关注</h3><p>只显示真正需要你处理的异常、错误和人工验收。</p></div><AlertTriangle :size="15" class="text-[var(--warning)]" /></div>
          <div class="wb-attention-list">
            <button v-for="item in attentionItems" :key="`${item.workspace.id}-${item.title}`" class="wb-attention-row" type="button" @click="openWorkspace(item.workspace.id)">
              <X v-if="item.level === 'error'" :size="14" class="text-[var(--danger)]" /><AlertTriangle v-else :size="14" class="text-[var(--warning)]" />
              <div class="min-w-0"><strong>{{ item.workspace.name }} · {{ item.title }}</strong><span class="truncate">{{ item.detail }}</span></div><ArrowUpRight :size="12" />
            </button>
          </div>
        </section>

        <section v-if="moduleVisible('workspaces')" class="wb-section wb-surface">
          <div class="wb-section-heading"><div><h3>工作区</h3><p>运行状态、Planning 和 MCP 用量保持在同一视图。</p></div><span class="text-[10px] text-[var(--text-muted)]">{{ workspaceCount }} Workspaces</span></div>
          <div class="wb-workspace-list">
            <div class="wb-workspace-header"><span>Workspace</span><span>Runtime</span><span>Planning</span><span>Tokens</span><span /></div>
            <div v-for="workspace in orderedWorkspaces" :key="workspace.id" class="wb-workspace-row" :class="{ 'is-pinned': dashboardPreferences.pinnedWorkspaceIds.includes(workspace.id) }">
              <div class="wb-workspace-name"><button type="button" class="truncate" @click="openWorkspace(workspace.id)">{{ workspace.name }}</button><small :title="workspace.path">{{ workspace.path }}</small></div>
              <div class="wb-runtime-pill"><span class="wb-runtime-dot" :class="stateClass(mcpRuntimeStates[workspace.id])" /><span>{{ stateLabel(mcpRuntimeStates[workspace.id]) }}</span><span class="font-mono text-[9px]">:{{ workspace.runtime.local_port }}</span></div>
              <div class="wb-mode-pill min-w-0"><GitBranch :size="11" /><span class="truncate" :title="planningSummary(workspace.id)">{{ planningSummary(workspace.id) }}</span></div>
              <div class="wb-workspace-token">{{ formatCount(workspaceUsageTokens(workspace.id)) }}</div>
              <div class="wb-workspace-actions">
                <button class="wb-icon-button !h-7 !w-7 !min-h-7" type="button" @click="togglePinnedWorkspace(workspace.id)"><PinOff v-if="dashboardPreferences.pinnedWorkspaceIds.includes(workspace.id)" :size="11" /><Pin v-else :size="11" /></button>
                <button class="wb-icon-button !h-7 !w-7 !min-h-7" type="button" @click="moveWorkspace(workspace.id, -1, orderedWorkspaces.map((item) => item.id))"><ChevronUp :size="11" /></button>
                <button class="wb-icon-button !h-7 !w-7 !min-h-7" type="button" @click="moveWorkspace(workspace.id, 1, orderedWorkspaces.map((item) => item.id))"><ChevronDown :size="11" /></button>
                <button class="wb-icon-button !h-7 !w-7 !min-h-7" type="button" @click="revealDirectory(workspace.path)"><FolderOpen :size="11" /></button>
                <button class="wb-icon-button !h-7 !w-7 !min-h-7" type="button" @click="copyWorkspacePath(workspace.id, workspace.path)"><Check v-if="copiedPathId === workspace.id" :size="11" class="text-[var(--success)]" /><Copy v-else :size="11" /></button>
                <button class="wb-icon-button !h-7 !w-7 !min-h-7" type="button" @click="toggleWorkspaceMcp(workspace.id)"><RotateCw v-if="busyMap[workspace.id]" :size="11" class="animate-spin" /><Square v-else-if="mcpRuntimeStates[workspace.id] === 'running'" :size="10" /><Play v-else :size="10" /></button>
                <button class="wb-icon-button !h-7 !w-7 !min-h-7" type="button" @click="openWorkspace(workspace.id)"><ArrowUpRight :size="11" /></button>
              </div>
            </div>
          </div>
        </section>

        <div class="wb-grid-two">
          <section v-if="moduleVisible('activity')" class="wb-section wb-surface">
            <div class="wb-section-heading"><div><h3>最近活动</h3><p>汇总各工作区最近的 History Session。</p></div><Activity :size="14" /></div>
            <div v-if="recentActivities.length" class="wb-activity-list">
              <button v-for="item in recentActivities" :key="`${item.workspace.id}-${item.timestamp}-${item.title}`" class="wb-activity-row" type="button" @click="openWorkspace(item.workspace.id)">
                <span class="wb-activity-time">{{ formatRelativeTime(item.timestamp) }}</span><div class="min-w-0"><strong>{{ item.workspace.name }} · {{ item.title }}</strong><p>{{ item.detail }}</p></div><ArrowUpRight :size="11" />
              </button>
            </div>
            <div v-else class="wb-empty-inline">还没有可汇总的 History Session。</div>
          </section>

          <section v-if="moduleVisible('health')" class="wb-section wb-surface">
            <div class="wb-section-heading"><div><h3>系统健康</h3><p>Runtime、连接和 Planning 状态压缩成可扫描信息。</p></div><Gauge :size="14" /></div>
            <div class="wb-health-overview">
              <div class="wb-mix-chart">
                <div class="wb-mix-head"><span>Runtime 分布</span><strong>{{ mcpRunning }}/{{ workspaceCount }} Online</strong></div>
                <div class="wb-mix-track">
                  <i v-for="item in runtimeMix" :key="item.key" :class="`mix-${item.key}`" :style="{ flexGrow: item.value }" />
                </div>
                <div class="wb-mix-legend"><span v-for="item in runtimeMix" :key="item.key"><i :class="`mix-${item.key}`" />{{ item.label }} <strong>{{ item.value }}</strong></span></div>
              </div>
              <div class="wb-mix-chart">
                <div class="wb-mix-head"><span>连接方式</span><strong>{{ workspaceCount }} Workspaces</strong></div>
                <div class="wb-mix-track">
                  <i v-for="item in connectionMix" :key="item.key" :class="`mix-${item.key}`" :style="{ flexGrow: item.value }" />
                </div>
                <div class="wb-mix-legend"><span v-for="item in connectionMix" :key="item.key"><i :class="`mix-${item.key}`" />{{ item.label }} <strong>{{ item.value }}</strong></span></div>
              </div>
              <div class="wb-mix-chart">
                <div class="wb-mix-head"><span>Planning 模式</span><strong>{{ planningStats.activeGoals }} Goals</strong></div>
                <div class="wb-mix-track">
                  <i v-for="item in planningModeMix" :key="item.key" :class="`mix-${item.key}`" :style="{ flexGrow: item.value }" />
                </div>
                <div class="wb-mix-legend"><span v-for="item in planningModeMix" :key="item.key"><i :class="`mix-${item.key}`" />{{ item.label }} <strong>{{ item.value }}</strong></span></div>
              </div>
              <div class="wb-health-signal" :class="errorServices === 0 && usageTotals.errorCount === 0 ? 'healthy' : 'warning'">
                <div><span>Quality Signal</span><strong>{{ errorServices === 0 && usageTotals.errorCount === 0 ? "Passed" : "Needs Attention" }}</strong></div>
                <small>{{ formatCount(usageTotals.errorCount) }} Tool Errors · {{ planningStats.pendingReview }} Waiting Review</small>
              </div>
            </div>
          </section>
        </div>

        <section v-if="moduleVisible('usage')" class="wb-section wb-surface wb-analytics-section">
          <div class="wb-section-heading"><div><h3>Token Analytics</h3><p>MCP JSON 传输量估算，用于观察工具调用趋势。</p></div><Zap :size="14" /></div>
          <div class="wb-grid-two">
            <DashboardUsagePanel :totals="usageTotals" :average-tokens="averageTokens" :chart="usageChart" />
            <div class="wb-ranking">
              <div v-for="row in usageRanking" :key="row.workspace.id" class="wb-rank-row"><span :title="row.workspace.name">{{ row.workspace.name }}</span><div class="wb-rank-track"><i :style="{ width: `${row.percentage}%` }" /></div><strong>{{ formatCount(row.tokens) }}</strong></div>
              <div class="wb-health-row"><span>Input / Output</span><strong>{{ formatCount(usageTotals.estimatedInputTokens) }} / {{ formatCount(usageTotals.estimatedOutputTokens) }}</strong></div>
              <div class="wb-health-row"><span>Avg / Tool Call</span><strong>{{ formatCount(averageTokens) }}</strong></div>
              <div class="wb-health-row"><span>Requests / Calls</span><strong>{{ formatCount(usageTotals.requestCount) }} / {{ formatCount(usageTotals.toolCallCount) }}</strong></div>
            </div>
          </div>
        </section>
      </template>
    </div>
  </section>

  <Teleport to="body">
    <div v-if="commandOpen" class="wb-command-backdrop" @click.self="commandOpen = false">
      <div class="wb-command-palette ios-glass-strong rounded-[24px]" role="dialog" aria-modal="true">
        <div class="wb-command-search"><Search :size="15" /><input ref="commandInput" v-model="commandQuery" placeholder="搜索工作区或操作…" @keydown.enter="commandEntries[0] && runCommand(commandEntries[0].run)" /><span class="wb-kbd">ESC</span></div>
        <div class="wb-command-results">
          <div v-if="commandEntries.length === 0" class="wb-empty-inline">没有匹配的操作。</div>
          <button v-for="(entry, index) in commandEntries" v-else :key="entry.label" class="wb-command-item" :class="{ active: index === 0 }" type="button" @click="runCommand(entry.run)">
            <Settings2 v-if="entry.hint === 'Settings'" :size="14" /><Zap v-else-if="entry.hint === 'Runtime'" :size="14" /><GitBranch v-else :size="14" />
            <span>{{ entry.label }}</span><small>{{ entry.hint }}</small>
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
