import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { useRoute } from "vue-router";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  clearToolAuditRecords,
  listToolAuditRecords,
  setToolAuditRetentionDays,
  type ToolAuditFilter,
  type ToolAuditRecord,
} from "$lib/api/tool-audit";
import { useClipboard } from "$lib/composables/use-clipboard";
import { formatFullDateTime } from "$lib/formatters";
import { workspaces } from "$lib/stores/app";
import { showToast } from "$lib/stores/toast";

const AUDIT_PAGE_SIZE = 50;
const DEFAULT_RETENTION_DAYS = 30;
const MIN_RETENTION_DAYS = 1;
const MAX_RETENTION_DAYS = 365;
const COPY_FEEDBACK_MS = 1800;
const ONE_HOUR_MS = 3_600_000;
const ONE_DAY_MS = 86_400_000;

const SHORT_MONTH_DAY_FORMATTER = new Intl.DateTimeFormat("zh-CN", {
  month: "numeric",
  day: "numeric",
});

const OUTCOME_SEGMENTS = [
  { value: "", label: "全部结果" },
  { value: "success", label: "仅成功" },
  { value: "error", label: "仅失败" },
];

const TIME_PRESETS = [
  { value: "all", label: "全部时间" },
  { value: "1h", label: "1 小时内" },
  { value: "today", label: "今天" },
  { value: "7d", label: "7 天内" },
  { value: "30d", label: "30 天内" },
  { value: "custom", label: "自定义" },
] as const;

const RETENTION_PRESETS = [7, 14, 30, 90, 180, 365] as const;

const ERROR_CATEGORY_LABELS: Record<string, string> = {
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

function parseOptionalDate(value: string): number | undefined {
  if (!value) return undefined;
  const timestamp = new Date(value).getTime();
  return Number.isFinite(timestamp) ? timestamp : undefined;
}

function parseOptionalDateEnd(value: string): number | undefined {
  const timestamp = parseOptionalDate(value);
  return timestamp === undefined ? undefined : timestamp + 59_999;
}

export function formatAuditTime(timestampMs: number): string {
  return formatFullDateTime(timestampMs);
}

export function formatAuditRelativeTime(timestampMs: number): string {
  const diffSec = Math.floor((Date.now() - timestampMs) / 1000);
  if (diffSec < 15) return "刚刚";
  if (diffSec < 60) return `${diffSec}秒前`;
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}分钟前`;
  const diffHours = Math.floor(diffMin / 60);
  if (diffHours < 24) return `${diffHours}小时前`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 30) return `${diffDays}天前`;
  return SHORT_MONTH_DAY_FORMATTER.format(timestampMs);
}

export function formatAuditDuration(durationMs: number): string {
  if (durationMs < 1000) return `${durationMs} ms`;
  return `${(durationMs / 1000).toFixed(durationMs < 10_000 ? 2 : 1)} s`;
}

export function formatAuditBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function formatAuditAuthLabel(record: ToolAuditRecord): { text: string; kind: "oauth" | "bearer" | "none" } {
  if (record.authMethod === "oauth") {
    return {
      text: record.clientId ? `OAuth · ${record.clientId}` : "OAuth · 匿名客户端",
      kind: "oauth",
    };
  }
  if (record.authMethod === "lan_peer") {
    return {
      text: record.sourcePeer ? `局域网对等 · ${record.sourcePeer}` : `局域网对等 · ${record.clientId ?? "已配对节点"}`,
      kind: "oauth",
    };
  }
  if (record.authMethod === "local_loopback") {
    return { text: "本机回环免授权直连", kind: "none" };
  }
  if (record.authMethod === "shared_bearer") {
    return { text: "共享 Bearer", kind: "bearer" };
  }
  return { text: "无认证", kind: "none" };
}

export function formatAuditErrorLabel(category: string | null): string {
  return category ? ERROR_CATEGORY_LABELS[category] || category : "工具执行错误";
}

/** All filter, pagination and mutation state lives outside the visual console.
 * Only this composable calls the audit API; the view remains presentational.
 */
export function useToolAudit() {
  const PAGE_SIZE = AUDIT_PAGE_SIZE;
  const route = useRoute();
  const { copiedKey, copyText: copyToClipboard } = useClipboard(COPY_FEEDBACK_MS);

  const loading = ref(false);
  const savingRetention = ref(false);
  const records = shallowRef<ToolAuditRecord[]>([]);
  const total = ref(0);
  const errorCount = ref(0);
  const workspaceCount = ref(0);
  const offset = ref(0);
  const retentionDays = ref(DEFAULT_RETENTION_DAYS);
  const retentionInput = ref(DEFAULT_RETENTION_DAYS);
  const healthMessage = ref<string | null>(null);

  const query = ref("");
  const workspaceId = ref(typeof route.query.workspaceId === "string" ? route.query.workspaceId : "");
  const sessionId = ref("");
  const toolName = ref("");
  const outcome = ref<string>("");
  const timePreset = ref<string>("all");
  const fromDate = ref("");
  const toDate = ref("");
  const showAdvancedFilters = ref(false);

  const retentionModalOpen = ref(false);
  const expandedKeys = ref<Set<string>>(new Set());
  let loadSequence = 0;

  const workspaceNames = computed(() => new Map(workspaces.value.map((w) => [w.id, w.name])));
  const selectedWorkspaceName = computed(() => {
    if (!workspaceId.value) return "全部工作区";
    return workspaceNames.value.get(workspaceId.value) || workspaceId.value;
  });

  const pageNumber = computed(() => Math.floor(offset.value / PAGE_SIZE) + 1);
  const pageCount = computed(() => Math.max(1, Math.ceil(total.value / PAGE_SIZE)));
  const rangeStart = computed(() => (total.value === 0 ? 0 : offset.value + 1));
  const rangeEnd = computed(() => Math.min(offset.value + PAGE_SIZE, total.value));

  const successRate = computed(() => {
    if (total.value === 0) return null;
    const succeeded = Math.max(0, total.value - errorCount.value);
    return Math.round((succeeded / total.value) * 1000) / 10;
  });

  const activeFilters = computed(() => {
    const list: Array<{ id: string; label: string; clear: () => void }> = [];
    if (query.value.trim()) {
      list.push({
        id: "query",
        label: `关键词: ${query.value.trim()}`,
        clear: () => {
          query.value = "";
          applyFilters();
        },
      });
    }
    if (workspaceId.value) {
      list.push({
        id: "workspace",
        label: `工作区: ${selectedWorkspaceName.value}`,
        clear: () => {
          workspaceId.value = "";
          applyFilters();
        },
      });
    }
    if (outcome.value) {
      list.push({
        id: "outcome",
        label: outcome.value === "success" ? "状态: 仅成功" : "状态: 仅失败",
        clear: () => {
          outcome.value = "";
          applyFilters();
        },
      });
    }
    if (toolName.value.trim()) {
      list.push({
        id: "toolName",
        label: `工具: ${toolName.value.trim()}`,
        clear: () => {
          toolName.value = "";
          applyFilters();
        },
      });
    }
    if (sessionId.value.trim()) {
      list.push({
        id: "sessionId",
        label: `Session: ${sessionId.value.trim().slice(0, 12)}…`,
        clear: () => {
          sessionId.value = "";
          applyFilters();
        },
      });
    }
    if (timePreset.value !== "all") {
      const match = TIME_PRESETS.find((p) => p.value === timePreset.value);
      list.push({
        id: "timePreset",
        label: `时间: ${match?.label || timePreset.value}`,
        clear: () => {
          timePreset.value = "all";
          fromDate.value = "";
          toDate.value = "";
          applyFilters();
        },
      });
    }
    return list;
  });

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

  function resolveTimeFilter(): { fromMs?: number; toMs?: number } {
    if (timePreset.value === "custom") {
      return {
        fromMs: parseOptionalDate(fromDate.value),
        toMs: parseOptionalDateEnd(toDate.value),
      };
    }
    const now = Date.now();
    if (timePreset.value === "1h") return { fromMs: now - ONE_HOUR_MS };
    if (timePreset.value === "today") {
      const today = new Date();
      today.setHours(0, 0, 0, 0);
      return { fromMs: today.getTime() };
    }
    if (timePreset.value === "7d") return { fromMs: now - 7 * ONE_DAY_MS };
    if (timePreset.value === "30d") return { fromMs: now - 30 * ONE_DAY_MS };
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
      if (expandedKeys.value.size > 0) {
        expandedKeys.value = new Set();
      }
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
    expandedKeys.value = new Set();
    void loadAudit(0);
  }

  function getRecordKey(record: ToolAuditRecord): string {
    return `${record.timestampMs}-${record.requestId || ""}-${record.toolName}`;
  }

  function toggleExpand(key: string) {
    const next = new Set(expandedKeys.value);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    expandedKeys.value = next;
  }

  async function copyText(text: string, key: string, label: string) {
    await copyToClipboard(text, { key, label, durationMs: COPY_FEEDBACK_MS });
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

  function openRetentionModal() {
    retentionInput.value = retentionDays.value;
    retentionModalOpen.value = true;
  }

  async function saveRetention() {
    const days = Number(retentionInput.value);
    if (!Number.isInteger(days) || days < MIN_RETENTION_DAYS || days > MAX_RETENTION_DAYS) {
      showToast(`保留期限必须在 ${MIN_RETENTION_DAYS} 到 ${MAX_RETENTION_DAYS} 天之间。`, {
        title: "期限无效",
        kind: "warning",
      });
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
    try {
      await clearToolAuditRecords(scopedWorkspaceId || undefined);
      expandedKeys.value = new Set();
      showToast("审计记录已清理。", { kind: "success" });
      await loadAudit(0);
    } catch (error) {
      showToast(String(error), { title: "清理审计记录失败", kind: "error" });
    }
  }

  onMounted(() => {
    void loadAudit(0);
  });

  onBeforeUnmount(() => {
    loadSequence += 1;
  });

  return {
    workspaces,
    PAGE_SIZE,
    loading,
    savingRetention,
    records,
    total,
    errorCount,
    workspaceCount,
    offset,
    retentionDays,
    retentionInput,
    healthMessage,
    query,
    workspaceId,
    sessionId,
    toolName,
    outcome,
    timePreset,
    fromDate,
    toDate,
    showAdvancedFilters,
    retentionModalOpen,
    expandedKeys,
    copiedKey,
    workspaceNames,
    selectedWorkspaceName,
    pageNumber,
    pageCount,
    rangeStart,
    rangeEnd,
    successRate,
    outcomeSegments: OUTCOME_SEGMENTS,
    timePresets: TIME_PRESETS,
    retentionPresets: RETENTION_PRESETS,
    activeFilters,
    loadAudit,
    applyFilters,
    handleTimePresetChange,
    resetFilters,
    getRecordKey,
    toggleExpand,
    copyText,
    quickFilterBySession,
    quickFilterByTool,
    quickFilterByWorkspace,
    openRetentionModal,
    saveRetention,
    clearRecords,
    formatTime: formatAuditTime,
    formatRelativeTime: formatAuditRelativeTime,
    formatDuration: formatAuditDuration,
    formatBytes: formatAuditBytes,
    authLabel: formatAuditAuthLabel,
    errorLabel: formatAuditErrorLabel,
  };
}
