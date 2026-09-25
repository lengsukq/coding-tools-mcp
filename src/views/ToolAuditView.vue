<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  Activity,
  AlertCircle,
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Clock3,
  Copy,
  ExternalLink,
  FileCode,
  Filter,
  FilterX,
  HardDrive,
  Layers,
  RefreshCw,
  RotateCcw,
  Search,
  Settings2,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  Tag,
  Trash2,
  X,
  Zap,
} from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import ModalDialog from "$src/components/ui/ModalDialog.vue";
import SegmentedControl from "$src/components/ui/SegmentedControl.vue";
import {
  clearToolAuditRecords,
  listToolAuditRecords,
  setToolAuditRetentionDays,
  type ToolAuditFilter,
  type ToolAuditRecord,
} from "$lib/api/tool-audit";
import { showToast } from "$lib/stores/toast";
import { workspaces } from "$lib/stores/app";

const PAGE_SIZE = 50;
const route = useRoute();
const router = useRouter();

// State
const loading = ref(false);
const savingRetention = ref(false);
const clearingRecords = ref(false);
const records = ref<ToolAuditRecord[]>([]);
const total = ref(0);
const errorCount = ref(0);
const workspaceCount = ref(0);
const offset = ref(0);
const retentionDays = ref(30);
const retentionInput = ref(30);
const healthMessage = ref<string | null>(null);

// Filters
const query = ref("");
const workspaceId = ref(typeof route.query.workspaceId === "string" ? route.query.workspaceId : "");
const sessionId = ref("");
const toolName = ref("");
const outcome = ref<string>("");
const timePreset = ref<string>("all");
const fromDate = ref("");
const toDate = ref("");
const showAdvancedFilters = ref(false);

// Modals & UI State
const retentionModalOpen = ref(false);
const clearConfirmOpen = ref(false);
const expandedKeys = ref<Set<string>>(new Set());
const copiedKey = ref<string | null>(null);
let loadSequence = 0;

// Computed lookups & stats
const workspaceNames = computed(() => new Map(workspaces.value.map((w) => [w.id, w.name])));
const selectedWorkspaceName = computed(() => {
  if (!workspaceId.value) return "全部工作区";
  return workspaceNames.value.get(workspaceId.value) || workspaceId.value;
});

const pageNumber = computed(() => Math.floor(offset.value / PAGE_SIZE) + 1);
const pageCount = computed(() => Math.max(1, Math.ceil(total.value / PAGE_SIZE)));
const rangeStart = computed(() => (total.value === 0 ? 0 : offset.value + 1));
const rangeEnd = computed(() => Math.min(offset.value + PAGE_SIZE, total.value));

const successCount = computed(() => Math.max(0, total.value - errorCount.value));
const successRate = computed(() => {
  if (total.value === 0) return 100;
  return Math.round((successCount.value / total.value) * 1000) / 10;
});

const outcomeSegments = [
  { value: "", label: "全部结果" },
  { value: "success", label: "仅成功" },
  { value: "error", label: "仅失败" },
];

const timePresets = [
  { value: "all", label: "全部时间" },
  { value: "1h", label: "1 小时内" },
  { value: "today", label: "今天" },
  { value: "7d", label: "7 天内" },
  { value: "30d", label: "30 天内" },
  { value: "custom", label: "自定义" },
];

const retentionPresets = [7, 14, 30, 90, 180, 365];

// Active filter badges
const activeFilters = computed(() => {
  const list: Array<{ id: string; label: string; clear: () => void }> = [];
  if (query.value.trim()) {
    list.push({
      id: "query",
      label: `关键词: ${query.value.trim()}`,
      clear: () => { query.value = ""; applyFilters(); },
    });
  }
  if (workspaceId.value) {
    list.push({
      id: "workspace",
      label: `工作区: ${selectedWorkspaceName.value}`,
      clear: () => { workspaceId.value = ""; applyFilters(); },
    });
  }
  if (outcome.value) {
    list.push({
      id: "outcome",
      label: outcome.value === "success" ? "状态: 仅成功" : "状态: 仅失败",
      clear: () => { outcome.value = ""; applyFilters(); },
    });
  }
  if (toolName.value.trim()) {
    list.push({
      id: "toolName",
      label: `工具: ${toolName.value.trim()}`,
      clear: () => { toolName.value = ""; applyFilters(); },
    });
  }
  if (sessionId.value.trim()) {
    list.push({
      id: "sessionId",
      label: `Session: ${sessionId.value.trim().slice(0, 12)}…`,
      clear: () => { sessionId.value = ""; applyFilters(); },
    });
  }
  if (timePreset.value !== "all") {
    const match = timePresets.find((p) => p.value === timePreset.value);
    list.push({
      id: "timePreset",
      label: `时间: ${match?.label || timePreset.value}`,
      clear: () => { timePreset.value = "all"; fromDate.value = ""; toDate.value = ""; applyFilters(); },
    });
  }
  return list;
});

// Watch route
watch(
  () => route.query.workspaceId,
  (value) => {
    const nextVal = typeof value === "string" ? value : "";
    if (workspaceId.value !== nextVal) {
      workspaceId.value = nextVal;
      void loadAudit(0);
    }
  },
);

function optionalDate(value: string): number | undefined {
  if (!value) return undefined;
  const timestamp = new Date(value).getTime();
  return Number.isFinite(timestamp) ? timestamp : undefined;
}

function optionalDateEnd(value: string): number | undefined {
  const timestamp = optionalDate(value);
  return timestamp === undefined ? undefined : timestamp + 59_999;
}

function resolveTimeFilter(): { fromMs?: number; toMs?: number } {
  if (timePreset.value === "custom") {
    return {
      fromMs: optionalDate(fromDate.value),
      toMs: optionalDateEnd(toDate.value),
    };
  }
  const now = Date.now();
  if (timePreset.value === "1h") return { fromMs: now - 3600 * 1000 };
  if (timePreset.value === "today") {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    return { fromMs: today.getTime() };
  }
  if (timePreset.value === "7d") return { fromMs: now - 7 * 86400 * 1000 };
  if (timePreset.value === "30d") return { fromMs: now - 30 * 86400 * 1000 };
  return {};
}

function buildFilter(nextOffset: number): ToolAuditFilter {
  const time = resolveTimeFilter();
  return {
    workspaceId: workspaceId.value || undefined,
    fromMs: time.fromMs,
    toMs: time.toMs,
    sessionId: sessionId.value.trim() || undefined,
    toolName: toolName.value.trim() || undefined,
    outcome: outcome.value || undefined,
    search: query.value.trim() || undefined,
    offset: nextOffset,
    limit: PAGE_SIZE,
  };
}

async function loadAudit(nextOffset = offset.value) {
  const sequence = ++loadSequence;
  loading.value = true;
  try {
    const page = await listToolAuditRecords(buildFilter(nextOffset));
    if (sequence !== loadSequence) return;
    records.value = page.records;
    total.value = page.total;
    errorCount.value = page.errorCount;
    workspaceCount.value = page.workspaceCount;
    offset.value = page.offset;
    retentionDays.value = page.retentionDays;
    retentionInput.value = page.retentionDays;
    healthMessage.value = page.healthMessage;
  } catch (error) {
    if (sequence === loadSequence) {
      showToast(String(error), { title: "读取审计记录失败", kind: "error" });
    }
  } finally {
    if (sequence === loadSequence) loading.value = false;
  }
}

function applyFilters() {
  void loadAudit(0);
}

function handleTimePresetChange(preset: string) {
  timePreset.value = preset;
  if (preset !== "custom") {
    fromDate.value = "";
    toDate.value = "";
    applyFilters();
  } else {
    showAdvancedFilters.value = true;
  }
}

function resetFilters() {
  query.value = "";
  sessionId.value = "";
  toolName.value = "";
  outcome.value = "";
  timePreset.value = "all";
  fromDate.value = "";
  toDate.value = "";
  workspaceId.value = "";
  void loadAudit(0);
}

// Row interactions
function getRecordKey(record: ToolAuditRecord): string {
  return `${record.timestampMs}-${record.requestId || ""}-${record.toolName}`;
}

function toggleExpand(key: string) {
  const next = new Set(expandedKeys.value);
  if (next.has(key)) {
    next.delete(key);
  } else {
    next.add(key);
  }
  expandedKeys.value = next;
}

async function copyText(text: string, key: string, label: string) {
  try {
    await navigator.clipboard.writeText(text);
    copiedKey.value = key;
    showToast(`${label}已复制`, { kind: "success", duration: 1800 });
    setTimeout(() => {
      if (copiedKey.value === key) copiedKey.value = null;
    }, 1800);
  } catch {
    showToast(`复制${label}失败`, { kind: "error" });
  }
}

function quickFilterBySession(id: string) {
  sessionId.value = id;
  showAdvancedFilters.value = true;
  applyFilters();
  showToast("已设置 Session 筛选", { kind: "info", duration: 1600 });
}

function quickFilterByTool(name: string) {
  toolName.value = name;
  showAdvancedFilters.value = true;
  applyFilters();
  showToast(`已筛选工具: ${name}`, { kind: "info", duration: 1600 });
}

function quickFilterByWorkspace(id: string) {
  workspaceId.value = id;
  applyFilters();
  showToast(`已筛选工作区: ${workspaceNames.value.get(id) || id}`, { kind: "info", duration: 1600 });
}

// Retention settings
function openRetentionModal() {
  retentionInput.value = retentionDays.value;
  retentionModalOpen.value = true;
}

async function saveRetention() {
  const days = Number(retentionInput.value);
  if (!Number.isInteger(days) || days < 1 || days > 365) {
    showToast("保留期限必须在 1 到 365 天之间。", { title: "期限无效", kind: "warning" });
    return;
  }
  savingRetention.value = true;
  try {
    retentionDays.value = await setToolAuditRetentionDays(days);
    retentionInput.value = retentionDays.value;
    retentionModalOpen.value = false;
    showToast(`审计记录将保留 ${retentionDays.value} 天。`, { kind: "success" });
    await loadAudit(0);
  } catch (error) {
    showToast(String(error), { title: "保存保留期限失败", kind: "error" });
  } finally {
    savingRetention.value = false;
  }
}

// Clear records
async function clearRecords(scope: "workspace" | "all") {
  const scopedWorkspaceId = scope === "workspace" ? workspaceId.value : "";
  const scopeLabel = scope === "workspace" ? selectedWorkspaceName.value : "所有工作区和全局调用";
  const confirmed = await ask(`将永久删除【${scopeLabel}】的本地审计记录，删除后无法恢复。`, {
    title: "清理本地审计记录",
    kind: "warning",
    okLabel: "立即删除",
    cancelLabel: "取消",
  });
  if (!confirmed) return;
  clearingRecords.value = true;
  try {
    await clearToolAuditRecords(scopedWorkspaceId || undefined);
    showToast("审计记录已清理。", { kind: "success" });
    clearConfirmOpen.value = false;
    await loadAudit(0);
  } catch (error) {
    showToast(String(error), { title: "清理审计记录失败", kind: "error" });
  } finally {
    clearingRecords.value = false;
  }
}

// Formatters
function formatTime(timestampMs: number): string {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(timestampMs);
}

function formatRelativeTime(timestampMs: number): string {
  const diffSec = Math.floor((Date.now() - timestampMs) / 1000);
  if (diffSec < 15) return "刚刚";
  if (diffSec < 60) return `${diffSec}秒前`;
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}分钟前`;
  const diffHours = Math.floor(diffMin / 60);
  if (diffHours < 24) return `${diffHours}小时前`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 30) return `${diffDays}天前`;
  return new Intl.DateTimeFormat("zh-CN", { month: "numeric", day: "numeric" }).format(timestampMs);
}

function formatDuration(durationMs: number): string {
  if (durationMs < 1000) return `${durationMs} ms`;
  return `${(durationMs / 1000).toFixed(durationMs < 10_000 ? 2 : 1)} s`;
}

function formatBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function authLabel(record: ToolAuditRecord): { text: string; kind: "oauth" | "bearer" | "none" } {
  if (record.authMethod === "oauth") {
    return {
      text: record.clientId ? `OAuth · ${record.clientId}` : "OAuth · 匿名客户端",
      kind: "oauth",
    };
  }
  if (record.authMethod === "shared_bearer") {
    return { text: "共享 Bearer", kind: "bearer" };
  }
  return { text: "无认证", kind: "none" };
}

function errorLabel(category: string | null): string {
  const labels: Record<string, string> = {
    policy: "策略拦截",
    permission: "权限拒绝",
    validation: "参数校验失败",
    not_found: "资源不存在",
    runtime: "运行时异常",
    storage: "存储失败",
    filesystem: "文件系统异常",
    protocol: "协议格式错误",
    internal: "内部错误",
    tool: "工具返回错误",
  };
  return category ? labels[category] || category : "工具执行错误";
}

onMounted(() => {
  void loadAudit(0);
});
</script>

<template>
  <section class="page-scroll flex-1 min-h-0">
    <div class="mx-auto w-full max-w-[1480px] px-6 py-7 lg:px-9 space-y-6">

      <!-- Header -->
      <header class="flex flex-wrap items-start justify-between gap-4 border-b border-[var(--border-light)] pb-5 animate-fade-in-up">
        <div>
          <div class="flex items-center gap-2">
            <span class="inline-flex items-center gap-1.5 rounded-full bg-[var(--primary-soft)] px-2.5 py-0.5 text-[11px] font-semibold text-[var(--primary)]">
              <ShieldCheck :size="12" /> 全局可观测与审计
            </span>
            <span
              v-if="healthMessage"
              class="inline-flex items-center gap-1 rounded-full bg-[#ff9f0a]/15 px-2.5 py-0.5 text-[11px] font-medium text-[#c77700] dark:text-[#ff9f0a]"
            >
              <AlertTriangle :size="12" /> 记录写入受限
            </span>
            <span
              v-else
              class="inline-flex items-center gap-1.5 rounded-full bg-[#30d158]/12 px-2.5 py-0.5 text-[11px] font-medium text-[#248a3d] dark:text-[#30d158]"
            >
              <span class="h-1.5 w-1.5 rounded-full bg-[#30d158] animate-pulse" /> 实时记录就绪
            </span>
          </div>
          <h1 class="mt-2 text-2xl font-bold tracking-tight text-[var(--text-main)] sm:text-3xl font-display">
            工具调用审计
          </h1>
          <p class="mt-1.5 max-w-2xl text-xs sm:text-sm leading-relaxed text-[var(--text-secondary)]">
            追溯所有经过已认证 MCP 网关的工具调用、执行状态、响应耗时与数据变更。日志仅在本机持久化。
          </p>
        </div>

        <div class="flex flex-wrap items-center gap-2.5 pt-1">
          <BaseButton variant="secondary" size="sm" :busy="loading" @click="loadAudit(0)">
            <RefreshCw :size="13" :class="{ 'animate-spin': loading }" /> 刷新数据
          </BaseButton>

          <BaseButton variant="ghost" size="sm" @click="openRetentionModal">
            <Clock3 :size="13" /> 保留 {{ retentionDays }} 天
          </BaseButton>

          <BaseButton
            v-if="workspaceId"
            variant="ghost"
            size="sm"
            @click="clearRecords('workspace')"
          >
            <Trash2 :size="13" /> 清理当前工作区
          </BaseButton>

          <BaseButton variant="danger" size="sm" @click="clearRecords('all')">
            <Trash2 :size="13" /> 清空记录
          </BaseButton>
        </div>
      </header>

      <!-- Health Warning Banner -->
      <transition enter-active-class="transition duration-200 ease-out" enter-from-class="opacity-0 -translate-y-2" enter-to-class="opacity-100 translate-y-0">
        <div
          v-if="healthMessage"
          class="flex items-start gap-3 rounded-xl border border-[#ff9f0a]/30 bg-[#ff9f0a]/10 px-4 py-3 text-sm text-[var(--text-main)] shadow-sm backdrop-blur-sm"
          role="status"
        >
          <AlertTriangle :size="18" class="mt-0.5 shrink-0 text-[#ff9f0a]" />
          <div class="min-w-0 flex-1">
            <p class="font-semibold text-xs text-[#b36b00] dark:text-[#ffb340]">审计日志写入异常通知</p>
            <p class="mt-0.5 text-xs leading-5 text-[var(--text-secondary)]">{{ healthMessage }} 提示：工具调用仍在继续正常执行，仅部分日志写盘被推迟。</p>
          </div>
        </div>
      </transition>

      <!-- KPI Metrics Strip -->
      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4 animate-fade-in-up delay-100">
        <!-- 1. Total Calls -->
        <GlassCard :padded="false" hoverable class="p-4 relative overflow-hidden">
          <div class="flex items-center justify-between">
            <span class="text-xs font-medium text-[var(--text-muted)]">匹配调用总数</span>
            <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-[var(--primary-soft)] text-[var(--primary)]">
              <Activity :size="15" />
            </div>
          </div>
          <div class="mt-3 flex items-baseline gap-2">
            <span class="text-2xl sm:text-3xl font-bold tracking-tight text-[var(--text-main)] font-mono font-display">
              {{ total.toLocaleString() }}
            </span>
            <span class="text-xs text-[var(--text-muted)]">次调用</span>
          </div>
          <div class="mt-2 flex items-center justify-between text-[11px] text-[var(--text-muted)]">
            <span>当前筛选范围</span>
            <span class="font-medium text-[var(--text-secondary)]">第 {{ pageNumber }} / {{ pageCount }} 页</span>
          </div>
        </GlassCard>

        <!-- 2. Success Rate -->
        <GlassCard :padded="false" hoverable class="p-4 relative overflow-hidden">
          <div class="flex items-center justify-between">
            <span class="text-xs font-medium text-[var(--text-muted)]">执行成功率</span>
            <div
              class="flex h-7 w-7 items-center justify-center rounded-lg"
              :class="errorCount === 0 ? 'bg-[#30d158]/12 text-[#30d158]' : 'bg-[#ff9f0a]/12 text-[#ff9f0a]'"
            >
              <CheckCircle2 v-if="errorCount === 0" :size="15" />
              <AlertCircle v-else :size="15" />
            </div>
          </div>
          <div class="mt-3 flex items-baseline gap-2">
            <span
              class="text-2xl sm:text-3xl font-bold tracking-tight font-mono font-display"
              :class="errorCount === 0 ? 'text-[#248a3d] dark:text-[#30d158]' : 'text-[#ff9f0a]'"
            >
              {{ successRate }}%
            </span>
            <span class="text-xs text-[var(--text-muted)]">
              {{ errorCount > 0 ? `${errorCount} 次失败` : '全部通过' }}
            </span>
          </div>
          <!-- Progress bar -->
          <div class="mt-2.5 h-1.5 w-full overflow-hidden rounded-full bg-black/[0.06] dark:bg-white/[0.08]">
            <div
              class="h-full rounded-full transition-all duration-500 ease-out"
              :class="successRate >= 95 ? 'bg-[#30d158]' : successRate >= 80 ? 'bg-[#ff9f0a]' : 'bg-[#ff453a]'"
              :style="{ width: `${successRate}%` }"
            />
          </div>
        </GlassCard>

        <!-- 3. Workspaces -->
        <GlassCard :padded="false" hoverable class="p-4 relative overflow-hidden">
          <div class="flex items-center justify-between">
            <span class="text-xs font-medium text-[var(--text-muted)]">覆盖工作区</span>
            <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-[var(--primary-soft)] text-[var(--accent-indigo)]">
              <Layers :size="15" />
            </div>
          </div>
          <div class="mt-3 flex items-baseline gap-2">
            <span class="text-2xl sm:text-3xl font-bold tracking-tight text-[var(--text-main)] font-mono font-display">
              {{ workspaceCount }}
            </span>
            <span class="text-xs text-[var(--text-muted)]">个活跃区域</span>
          </div>
          <p class="mt-2 truncate text-[11px] text-[var(--text-muted)]" :title="selectedWorkspaceName">
            当前焦点: <span class="font-medium text-[var(--text-secondary)]">{{ selectedWorkspaceName }}</span>
          </p>
        </GlassCard>

        <!-- 4. Retention Policy -->
        <GlassCard :padded="false" hoverable class="p-4 relative overflow-hidden">
          <div class="flex items-center justify-between">
            <span class="text-xs font-medium text-[var(--text-muted)]">日志生命周期</span>
            <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-[#af52de]/10 text-[#af52de] dark:bg-[#bf5af2]/15 dark:text-[#bf5af2]">
              <HardDrive :size="15" />
            </div>
          </div>
          <div class="mt-3 flex items-center justify-between">
            <div class="flex items-baseline gap-1.5">
              <span class="text-2xl sm:text-3xl font-bold tracking-tight text-[var(--text-main)] font-mono font-display">
                {{ retentionDays }}
              </span>
              <span class="text-xs text-[var(--text-muted)]">天自动轮转</span>
            </div>
            <button
              type="button"
              class="flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium text-[var(--primary)] transition hover:bg-[var(--primary-soft)]"
              @click="openRetentionModal"
            >
              <Settings2 :size="12" /> 配置
            </button>
          </div>
          <p class="mt-2 text-[11px] text-[var(--text-muted)]">
            本地存储 · 启动时安全清理过期项
          </p>
        </GlassCard>
      </div>

      <!-- Main Audit Explorer Console -->
      <GlassCard :padded="false" class="overflow-hidden border border-[var(--border)] shadow-sm animate-fade-in-up delay-200">
        <!-- Filter Toolbar -->
        <div class="border-b border-[var(--border-light)] bg-black/[0.015] dark:bg-white/[0.015] p-4 lg:p-5 space-y-3.5">
          <!-- Top Row: Search + Outcome Segments + Time Range + Actions -->
          <div class="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <!-- Search bar -->
            <div class="relative flex-1 min-w-[240px] max-w-xl">
              <Search :size="14" class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
              <input
                v-model="query"
                type="text"
                placeholder="搜索工具名称、Request ID、Session ID、客户端…"
                class="w-full h-9 rounded-xl border border-[var(--border)] bg-[var(--card-bg)] pl-9 pr-8 text-xs text-[var(--text-main)] placeholder-[var(--text-muted)] outline-none transition-all focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--primary)]/15"
                @keydown.enter="applyFilters"
              />
              <button
                v-if="query"
                type="button"
                class="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)] hover:text-[var(--text-main)]"
                @click="query = ''; applyFilters()"
              >
                <X :size="13" />
              </button>
            </div>

            <!-- Segments & Quick Presets -->
            <div class="flex flex-wrap items-center gap-2">
              <!-- Outcome Segmented Control -->
              <SegmentedControl
                v-model="outcome"
                :items="outcomeSegments"
                aria-label="执行结果状态切换"
                @update:model-value="applyFilters"
              />

              <!-- Advanced Filter Toggle -->
              <button
                type="button"
                class="inline-flex h-8 items-center gap-1.5 rounded-full border border-[var(--border)] px-3 text-xs font-medium text-[var(--text-secondary)] transition hover:bg-[var(--surface-hover)] hover:text-[var(--text-main)]"
                :class="{ 'border-[var(--primary)] text-[var(--primary)] bg-[var(--primary-soft)]': showAdvancedFilters }"
                @click="showAdvancedFilters = !showAdvancedFilters"
              >
                <Filter :size="13" />
                <span>高级过滤</span>
                <ChevronDown :size="12" class="transition-transform duration-200" :class="{ 'rotate-180': showAdvancedFilters }" />
              </button>

              <BaseButton variant="primary" size="sm" @click="applyFilters">
                <Search :size="13" /> 检索
              </BaseButton>
            </div>
          </div>

          <!-- Time Presets Pills -->
          <div class="flex flex-wrap items-center gap-1.5 pt-0.5">
            <span class="text-[11px] font-medium text-[var(--text-muted)] mr-1">时间范围:</span>
            <button
              v-for="preset in timePresets"
              :key="preset.value"
              type="button"
              class="rounded-full px-3 py-1 text-[11px] font-medium transition-all"
              :class="timePreset === preset.value
                ? 'bg-[image:var(--primary-gradient,var(--accent-gradient))] text-white shadow-sm'
                : 'text-[var(--text-secondary)] border border-[var(--card-border)] hover:bg-[var(--primary-soft)] hover:text-[var(--text-main)]'"
              @click="handleTimePresetChange(preset.value)"
            >
              {{ preset.label }}
            </button>
          </div>

          <!-- Advanced Filters Drawer -->
          <transition
            enter-active-class="transition duration-200 ease-out"
            enter-from-class="opacity-0 -translate-y-2 max-h-0"
            enter-to-class="opacity-100 translate-y-0 max-h-96"
            leave-active-class="transition duration-150 ease-in"
            leave-from-class="opacity-100 translate-y-0 max-h-96"
            leave-to-class="opacity-0 -translate-y-2 max-h-0"
          >
            <div v-if="showAdvancedFilters" class="grid gap-3 pt-2 border-t border-[var(--border-light)] sm:grid-cols-2 lg:grid-cols-4">
              <!-- Workspace Select -->
              <div>
                <label class="block text-[11px] font-medium text-[var(--text-muted)] mb-1">所属工作区</label>
                <select
                  v-model="workspaceId"
                  class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-2.5 text-xs text-[var(--text-main)] outline-none focus:border-[var(--primary)]"
                  @change="applyFilters"
                >
                  <option value="">全部工作区与全局调用</option>
                  <option v-for="ws in workspaces" :key="ws.id" :value="ws.id">
                    {{ ws.name }}
                  </option>
                </select>
              </div>

              <!-- Tool Name Input -->
              <div>
                <label class="block text-[11px] font-medium text-[var(--text-muted)] mb-1">工具名称精确匹配</label>
                <input
                  v-model="toolName"
                  type="text"
                  placeholder="如: edit_file, execute_command"
                  class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-2.5 text-xs text-[var(--text-main)] placeholder-[var(--text-muted)] outline-none focus:border-[var(--primary)]"
                  @keydown.enter="applyFilters"
                />
              </div>

              <!-- Session ID Input -->
              <div>
                <label class="block text-[11px] font-medium text-[var(--text-muted)] mb-1">Session ID 关联</label>
                <input
                  v-model="sessionId"
                  type="text"
                  placeholder="匹配会话会签标识"
                  class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-2.5 text-xs text-[var(--text-main)] placeholder-[var(--text-muted)] outline-none focus:border-[var(--primary)]"
                  @keydown.enter="applyFilters"
                />
              </div>

              <!-- Custom Date Range -->
              <div v-if="timePreset === 'custom'" class="sm:col-span-2 lg:col-span-1">
                <label class="block text-[11px] font-medium text-[var(--text-muted)] mb-1">自定义起止时间</label>
                <div class="flex items-center gap-1.5">
                  <input
                    v-model="fromDate"
                    type="datetime-local"
                    class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-2 text-[10px] text-[var(--text-main)] outline-none focus:border-[var(--primary)]"
                    @change="applyFilters"
                  />
                  <span class="text-[var(--text-muted)] text-xs">至</span>
                  <input
                    v-model="toDate"
                    type="datetime-local"
                    class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-2 text-[10px] text-[var(--text-main)] outline-none focus:border-[var(--primary)]"
                    @change="applyFilters"
                  />
                </div>
              </div>
            </div>
          </transition>

          <!-- Active Filter Pills Bar -->
          <div v-if="activeFilters.length > 0" class="flex flex-wrap items-center gap-1.5 pt-1">
            <span class="text-[11px] font-medium text-[var(--text-muted)]">生效筛选:</span>
            <span
              v-for="filter in activeFilters"
              :key="filter.id"
              class="inline-flex items-center gap-1 rounded-md bg-[var(--surface-hover)] border border-[var(--border)] px-2 py-0.5 text-[11px] text-[var(--text-main)]"
            >
              {{ filter.label }}
              <button
                type="button"
                class="text-[var(--text-muted)] hover:text-[#ff453a] transition-colors"
                @click="filter.clear"
              >
                <X :size="11" />
              </button>
            </span>

            <button
              type="button"
              class="inline-flex items-center gap-1 text-[11px] text-[var(--primary)] hover:underline ml-1"
              @click="resetFilters"
            >
              <RotateCcw :size="11" /> 清空所有筛选
            </button>
          </div>
        </div>

        <!-- Records Feed Area -->
        <!-- Loading State -->
        <div v-if="loading && records.length === 0" class="flex min-h-[320px] flex-col items-center justify-center gap-3 text-sm text-[var(--text-muted)]">
          <div class="flex h-12 w-12 items-center justify-center rounded-2xl bg-[var(--surface-hover)]">
            <RefreshCw :size="22" class="animate-spin text-[var(--primary)]" />
          </div>
          <p class="font-medium">正在读取审计日志记录…</p>
        </div>

        <!-- Empty State -->
        <div v-else-if="records.length === 0" class="flex min-h-[320px] flex-col items-center justify-center px-6 text-center">
          <div class="flex h-14 w-14 items-center justify-center rounded-2xl bg-[var(--surface-hover)] text-[var(--text-muted)] mb-3">
            <FilterX v-if="activeFilters.length > 0" :size="26" />
            <ShieldCheck v-else :size="26" />
          </div>
          <p class="text-sm font-semibold text-[var(--text-main)]">
            {{ activeFilters.length > 0 ? '未匹配到符合条件的审计记录' : '暂无工具调用审计记录' }}
          </p>
          <p class="mt-1.5 max-w-md text-xs leading-relaxed text-[var(--text-muted)]">
            {{ activeFilters.length > 0
              ? '建议调整关键词、时间范围或放宽过滤条件重试。'
              : '当已授权的 MCP 客户端调用工具时，调用记录将自动捕获并列于此处。' }}
          </p>
          <BaseButton
            v-if="activeFilters.length > 0"
            variant="secondary"
            size="sm"
            class="mt-4"
            @click="resetFilters"
          >
            <RotateCcw :size="13" /> 重置所有筛选条件
          </BaseButton>
        </div>

        <!-- Records List -->
        <div v-else class="divide-y divide-[var(--border-light)]">
          <article
            v-for="record in records"
            :key="getRecordKey(record)"
            class="group px-4 py-3.5 transition-colors hover:bg-black/[0.015] dark:hover:bg-white/[0.02] lg:px-5 cursor-pointer"
            @click="toggleExpand(getRecordKey(record))"
          >
            <!-- Primary Row -->
            <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
              <!-- Left: Status + Tool + Wrapper + Tags -->
              <div class="flex flex-wrap items-center gap-2.5 min-w-0">
                <!-- Status icon badge -->
                <div
                  class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-white"
                  :class="record.outcome === 'success'
                    ? 'bg-[#30d158]/15 text-[#248a3d] dark:text-[#30d158]'
                    : 'bg-[#ff453a]/15 text-[#d70015] dark:text-[#ff453a]'"
                >
                  <Check v-if="record.outcome === 'success'" :size="13" stroke-width="2.5" />
                  <AlertCircle v-else :size="13" stroke-width="2.5" />
                </div>

                <!-- Tool Name Badge -->
                <span class="font-mono text-xs font-semibold rounded-md bg-[var(--primary-soft)] text-[var(--primary)] px-2 py-0.5">
                  {{ record.toolName }}
                </span>

                <!-- Wrapper indicator -->
                <span v-if="record.wrapperToolName" class="text-[11px] text-[var(--text-muted)] font-mono">
                  via {{ record.wrapperToolName }}
                </span>

                <!-- Error Category Pill -->
                <span
                  v-if="record.outcome !== 'success'"
                  class="inline-flex items-center gap-1 rounded-full bg-[#ff453a]/10 px-2 py-0.5 text-[10px] font-semibold text-[#d70015] dark:text-[#ff453a]"
                >
                  {{ errorLabel(record.errorCategory) }}
                </span>

                <!-- Change impact pill -->
                <span
                  v-if="record.changeId"
                  class="inline-flex items-center gap-1 rounded-full bg-[#bf5af2]/12 px-2 py-0.5 text-[10px] font-medium text-[#8944ab] dark:text-[#da8fff]"
                  title="引发文件变更"
                >
                  <Zap :size="10" />
                  变更 {{ record.changedFileCount ?? 0 }} 文件
                </span>
              </div>

              <!-- Right: Duration + Time + Payload + Expand Arrow -->
              <div class="flex items-center gap-3 shrink-0 ml-auto text-xs text-[var(--text-muted)]">
                <!-- Payload size -->
                <div class="hidden sm:flex items-center gap-2 text-[11px] font-mono text-[var(--text-secondary)]">
                  <span class="inline-flex items-center gap-0.5" title="请求体积">
                    <ArrowUp :size="10" class="text-[var(--primary)]" /> {{ formatBytes(record.requestBytes) }}
                  </span>
                  <span class="inline-flex items-center gap-0.5" title="响应体积">
                    <ArrowDown :size="10" class="text-[#30d158]" /> {{ formatBytes(record.responseBytes) }}
                  </span>
                </div>

                <!-- Execution Duration -->
                <span
                  class="inline-flex items-center rounded-md px-1.5 py-0.5 text-[11px] font-mono font-medium"
                  :class="record.durationMs < 200
                    ? 'bg-black/[0.04] text-[var(--text-secondary)] dark:bg-white/[0.06]'
                    : record.durationMs < 1000
                      ? 'bg-[var(--primary-soft)] text-[var(--primary)]'
                      : 'bg-[#ff9500]/15 text-[#b36b00] dark:text-[#ff9f0a]'"
                >
                  {{ formatDuration(record.durationMs) }}
                </span>

                <!-- Relative time -->
                <time
                  class="text-[11px] text-[var(--text-muted)] min-w-[50px] text-right"
                  :title="formatTime(record.timestampMs)"
                >
                  {{ formatRelativeTime(record.timestampMs) }}
                </time>

                <!-- Expand Chevron -->
                <div
                  class="flex h-5 w-5 items-center justify-center rounded text-[var(--text-muted)] transition-transform duration-200"
                  :class="{ 'rotate-180': expandedKeys.has(getRecordKey(record)) }"
                >
                  <ChevronDown :size="14" />
                </div>
              </div>
            </div>

            <!-- Secondary Subline: Workspace + Auth + Brief IDs -->
            <div class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-[var(--text-muted)] pl-8.5">
              <span class="flex items-center gap-1 font-medium text-[var(--text-secondary)]">
                <Layers :size="11" class="text-[var(--text-muted)]" />
                {{ record.workspaceId ? workspaceNames.get(record.workspaceId) || record.workspaceId : '全局路由' }}
              </span>

              <span>•</span>

              <span>{{ authLabel(record).text }}</span>

              <template v-if="record.requestId">
                <span>•</span>
                <span class="font-mono text-[10px] text-[var(--text-muted)] truncate max-w-[200px]" :title="record.requestId">
                  Req: {{ record.requestId.slice(0, 14) }}…
                </span>
              </template>

              <template v-if="record.sessionId">
                <span>•</span>
                <span class="font-mono text-[10px] text-[var(--text-muted)] truncate max-w-[180px]" :title="record.sessionId">
                  Sess: {{ record.sessionId.slice(0, 12) }}…
                </span>
              </template>
            </div>

            <!-- Expanded Details Drawer -->
            <transition
              enter-active-class="transition duration-200 ease-out"
              enter-from-class="opacity-0 -translate-y-1"
              enter-to-class="opacity-100 translate-y-0"
              leave-active-class="transition duration-150 ease-in"
              leave-from-class="opacity-100 translate-y-0"
              leave-to-class="opacity-0 -translate-y-1"
            >
              <div
                v-if="expandedKeys.has(getRecordKey(record))"
                class="mt-3.5 rounded-xl border border-[var(--border)] bg-black/[0.025] dark:bg-white/[0.03] p-4 text-xs space-y-3 cursor-default"
                @click.stop
              >
                <!-- Detail Header / Quick Filter actions -->
                <div class="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--border-light)] pb-2.5">
                  <span class="font-semibold text-xs text-[var(--text-main)] flex items-center gap-1.5">
                    <Sparkles :size="13" class="text-[var(--primary)]" /> 调用上下文与排查诊断
                  </span>

                  <!-- Quick Filter Actions -->
                  <div class="flex flex-wrap items-center gap-1.5">
                    <button
                      v-if="record.sessionId"
                      type="button"
                      class="inline-flex items-center gap-1 rounded-full bg-[var(--card-bg)] border border-[var(--card-border)] px-2.5 py-1 text-[11px] font-medium text-[var(--text-secondary)] transition hover:text-[var(--primary)] hover:border-[var(--primary)] shadow-xs"
                      @click="quickFilterBySession(record.sessionId)"
                    >
                      <Filter :size="10" /> 仅看此 Session
                    </button>

                    <button
                      type="button"
                      class="inline-flex items-center gap-1 rounded-full bg-[var(--card-bg)] border border-[var(--card-border)] px-2.5 py-1 text-[11px] font-medium text-[var(--text-secondary)] transition hover:text-[var(--primary)] hover:border-[var(--primary)] shadow-xs"
                      @click="quickFilterByTool(record.toolName)"
                    >
                      <Filter :size="10" /> 仅看此工具
                    </button>

                    <button
                      v-if="record.workspaceId"
                      type="button"
                      class="inline-flex items-center gap-1 rounded-full bg-[var(--card-bg)] border border-[var(--card-border)] px-2.5 py-1 text-[11px] font-medium text-[var(--text-secondary)] transition hover:text-[var(--primary)] hover:border-[var(--primary)] shadow-xs"
                      @click="quickFilterByWorkspace(record.workspaceId)"
                    >
                      <Filter :size="10" /> 过滤当前工作区
                    </button>
                  </div>
                </div>

                <!-- Structured Key-Value Grid -->
                <div class="grid grid-cols-1 gap-2.5 sm:grid-cols-2 lg:grid-cols-3 font-mono text-[11px]">
                  <!-- Request ID -->
                  <div class="rounded-lg bg-[var(--card-bg)] border border-[var(--border)] p-2.5">
                    <div class="flex items-center justify-between text-[10px] font-sans text-[var(--text-muted)] mb-1">
                      <span>Request ID</span>
                      <button
                        v-if="record.requestId"
                        type="button"
                        class="text-[var(--text-muted)] hover:text-[var(--text-main)] transition"
                        title="复制完整 Request ID"
                        @click="copyText(record.requestId, `req-${record.requestId}`, 'Request ID')"
                      >
                        <Check v-if="copiedKey === `req-${record.requestId}`" :size="12" class="text-[#30d158]" />
                        <Copy v-else :size="12" />
                      </button>
                    </div>
                    <span class="break-all select-all text-[var(--text-main)]">
                      {{ record.requestId || '—' }}
                    </span>
                  </div>

                  <!-- Session ID -->
                  <div class="rounded-lg bg-[var(--card-bg)] border border-[var(--border)] p-2.5">
                    <div class="flex items-center justify-between text-[10px] font-sans text-[var(--text-muted)] mb-1">
                      <span>Session ID</span>
                      <button
                        v-if="record.sessionId"
                        type="button"
                        class="text-[var(--text-muted)] hover:text-[var(--text-main)] transition"
                        title="复制 Session ID"
                        @click="copyText(record.sessionId, `sess-${record.sessionId}`, 'Session ID')"
                      >
                        <Check v-if="copiedKey === `sess-${record.sessionId}`" :size="12" class="text-[#30d158]" />
                        <Copy v-else :size="12" />
                      </button>
                    </div>
                    <span class="break-all select-all text-[var(--text-main)]">
                      {{ record.sessionId || '—' }}
                    </span>
                  </div>

                  <!-- Client ID / Auth Method -->
                  <div class="rounded-lg bg-[var(--card-bg)] border border-[var(--border)] p-2.5">
                    <div class="flex items-center justify-between text-[10px] font-sans text-[var(--text-muted)] mb-1">
                      <span>认证与客户端</span>
                      <button
                        v-if="record.clientId"
                        type="button"
                        class="text-[var(--text-muted)] hover:text-[var(--text-main)] transition"
                        title="复制 Client ID"
                        @click="copyText(record.clientId, `cli-${record.clientId}`, 'Client ID')"
                      >
                        <Check v-if="copiedKey === `cli-${record.clientId}`" :size="12" class="text-[#30d158]" />
                        <Copy v-else :size="12" />
                      </button>
                    </div>
                    <span class="break-all select-all text-[var(--text-main)]">
                      {{ record.clientId ? `${record.authMethod}: ${record.clientId}` : record.authMethod }}
                    </span>
                  </div>

                  <!-- Change ID -->
                  <div v-if="record.changeId" class="rounded-lg bg-[var(--card-bg)] border border-[var(--border)] p-2.5 sm:col-span-2">
                    <div class="flex items-center justify-between text-[10px] font-sans text-[var(--text-muted)] mb-1">
                      <span class="flex items-center gap-1 text-[#bf5af2]">
                        <Zap :size="11" /> 状态变更摘要 (Change ID)
                      </span>
                      <button
                        type="button"
                        class="text-[var(--text-muted)] hover:text-[var(--text-main)] transition"
                        title="复制 Change ID"
                        @click="copyText(record.changeId!, `chg-${record.changeId}`, 'Change ID')"
                      >
                        <Check v-if="copiedKey === `chg-${record.changeId}`" :size="12" class="text-[#30d158]" />
                        <Copy v-else :size="12" />
                      </button>
                    </div>
                    <div class="flex items-baseline justify-between gap-2">
                      <code class="break-all select-all text-[#bf5af2]">{{ record.changeId }}</code>
                      <span class="shrink-0 text-[10px] font-sans text-[var(--text-muted)]">
                        涉及 {{ record.changedFileCount ?? 0 }} 个受影响文件
                      </span>
                    </div>
                  </div>

                  <!-- Execution Timestamp & Payload Details -->
                  <div class="rounded-lg bg-[var(--card-bg)] border border-[var(--border)] p-2.5 font-sans" :class="{ 'sm:col-span-2 lg:col-span-1': !record.changeId }">
                    <div class="text-[10px] text-[var(--text-muted)] mb-1">执行绝对时间与体积</div>
                    <div class="space-y-0.5 text-[11px] text-[var(--text-secondary)] font-mono">
                      <p>时间戳: {{ formatTime(record.timestampMs) }}</p>
                      <p>请求体: {{ formatBytes(record.requestBytes) }} ({{ record.requestBytes }} B)</p>
                      <p>响应体: {{ formatBytes(record.responseBytes) }} ({{ record.responseBytes }} B)</p>
                    </div>
                  </div>
                </div>
              </div>
            </transition>
          </article>
        </div>

        <!-- Pagination Footer -->
        <footer class="flex flex-wrap items-center justify-between gap-3 border-t border-[var(--border-light)] bg-black/[0.015] dark:bg-white/[0.015] px-4 py-3 lg:px-5">
          <div class="flex items-center gap-2 text-xs text-[var(--text-muted)]">
            <span>显示第 <strong class="font-medium text-[var(--text-main)]">{{ rangeStart }}–{{ rangeEnd }}</strong> 条</span>
            <span>/</span>
            <span>共 <strong class="font-medium text-[var(--text-main)]">{{ total.toLocaleString() }}</strong> 条记录</span>
          </div>

          <div class="flex items-center gap-2">
            <BaseButton
              variant="secondary"
              size="sm"
              :disabled="offset === 0 || loading"
              @click="loadAudit(Math.max(0, offset - PAGE_SIZE))"
            >
              <ChevronLeft :size="14" /> 上一页
            </BaseButton>

            <span class="min-w-16 text-center text-xs font-mono font-medium text-[var(--text-secondary)]">
              {{ pageNumber }} / {{ pageCount }}
            </span>

            <BaseButton
              variant="secondary"
              size="sm"
              :disabled="offset + PAGE_SIZE >= total || loading"
              @click="loadAudit(offset + PAGE_SIZE)"
            >
              下一页 <ChevronRight :size="14" />
            </BaseButton>
          </div>
        </footer>
      </GlassCard>

      <!-- Bottom System Notes -->
      <footer class="flex items-center justify-between px-1 text-[11px] leading-relaxed text-[var(--text-muted)]">
        <p>
          💡 审计日志安全说明：记录保存在本机应用数据目录。为保护用户隐私，仅持久化工具名称、耗时与变更摘要，不保存请求提示词参数与完整模型输出内容。
        </p>
        <span class="shrink-0 font-mono text-[10px]">MCP Audit v2.4</span>
      </footer>

    </div>

    <!-- Retention Configuration Modal -->
    <ModalDialog
      :open="retentionModalOpen"
      title="配置审计日志保留期限"
      description="超过保留天数的历史工具调用记录将在应用启动或定时轮转中自动安全清理。"
      @close="retentionModalOpen = false"
    >
      <div class="space-y-4 pt-1">
        <!-- Preset Days Selection -->
        <div>
          <label class="block text-xs font-semibold text-[var(--text-main)] mb-2">快速预设周期</label>
          <div class="grid grid-cols-3 gap-2">
            <button
              v-for="days in retentionPresets"
              :key="days"
              type="button"
              class="rounded-xl border px-3 py-2 text-center transition-all"
              :class="retentionInput === days
                ? 'border-[var(--primary)] bg-[var(--primary-soft)] font-bold text-[var(--primary)] shadow-sm'
                : 'border-[var(--border)] bg-[var(--surface)] text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]'"
              @click="retentionInput = days"
            >
              <span class="block text-sm font-semibold">{{ days }} 天</span>
              <span class="block text-[10px] text-[var(--text-muted)] mt-0.5">
                {{ days === 30 ? '默认推荐' : days === 365 ? '长期合规' : `${Math.round(days / 7)} 周` }}
              </span>
            </button>
          </div>
        </div>

        <!-- Custom Days Input -->
        <div class="rounded-xl border border-[var(--border)] bg-[var(--surface-hover)] p-3">
          <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1.5">自定义保留天数 (1 ~ 365 天)</label>
          <div class="flex items-center gap-2">
            <input
              v-model.number="retentionInput"
              type="number"
              min="1"
              max="365"
              class="w-28 h-9 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-3 text-sm font-semibold font-mono text-[var(--text-main)] outline-none focus:border-[var(--primary)]"
            />
            <span class="text-xs text-[var(--text-muted)]">天</span>
          </div>
        </div>

        <!-- Actions -->
        <div class="flex items-center justify-end gap-2 pt-2 border-t border-[var(--border-light)]">
          <BaseButton variant="ghost" size="sm" @click="retentionModalOpen = false">
            取消
          </BaseButton>
          <BaseButton variant="primary" size="sm" :busy="savingRetention" @click="saveRetention">
            保存保留策略
          </BaseButton>
        </div>
      </div>
    </ModalDialog>
  </section>
</template>
