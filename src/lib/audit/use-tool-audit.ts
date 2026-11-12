import { computed, onMounted, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  clearToolAuditRecords,
  listToolAuditRecords,
  setToolAuditRetentionDays,
  type ToolAuditFilter,
  type ToolAuditRecord,
} from "$lib/api/tool-audit";
import { showToast } from "$lib/stores/toast";
import { workspaces } from "$lib/stores/app";

/** All filter, pagination and mutation state lives outside the visual console.
 * Only this composable calls the audit API; the view remains presentational.
 */
export function useToolAudit() {
const PAGE_SIZE = 50;
const route = useRoute();

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
  if (total.value === 0) return null;
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
  return {
    workspaces, PAGE_SIZE, loading, savingRetention, clearingRecords, records, total, errorCount, workspaceCount, offset, retentionDays, retentionInput, healthMessage, query, workspaceId, sessionId, toolName, outcome, timePreset, fromDate, toDate, showAdvancedFilters, retentionModalOpen, clearConfirmOpen, expandedKeys, copiedKey, workspaceNames, selectedWorkspaceName, pageNumber, pageCount, rangeStart, rangeEnd, successCount, successRate, outcomeSegments, timePresets, retentionPresets, activeFilters, loadAudit, applyFilters, handleTimePresetChange, resetFilters, getRecordKey, toggleExpand, copyText, quickFilterBySession, quickFilterByTool, quickFilterByWorkspace, openRetentionModal, saveRetention, clearRecords, formatTime, formatRelativeTime, formatDuration, formatBytes, authLabel, errorLabel
  };
}
