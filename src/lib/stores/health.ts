import { computed, ref } from "vue";
import { runGlobalHealthChecks, type HealthItem } from "$lib/api/health";

const healthClock = ref(Date.now());
export const globalHealthItems = ref<HealthItem[]>([]);
export const globalHealthBusy = ref(false);
export const globalHealthLastCheckedAt = ref<number | null>(null);
export const globalHealthError = ref<string | null>(null);

export const globalHealthSummary = computed(() => {
  const failed = globalHealthItems.value.filter((item) => !item.ok);
  const total = globalHealthItems.value.length;
  if (globalHealthBusy.value && total === 0) {
    return {
      state: "checking" as const,
      label: "检查中",
      detail: "正在检查 Global MCP 与公网连接",
      failed,
      total,
    };
  }
  if (globalHealthError.value) {
    return {
      state: "error" as const,
      label: "检查失败",
      detail: globalHealthError.value,
      failed,
      total,
    };
  }
  if(globalHealthLastCheckedAt.value && healthClock.value-globalHealthLastCheckedAt.value>=90000){return {state:"unknown" as const,label:"检查已过期",detail:"连接状态待重新检查",failed,total};}
  if (failed.length > 0) {
    return {
      state: "warning" as const,
      label: "需处理",
      detail: `${failed.length} / ${total} 项健康检查异常`,
      failed,
      total,
    };
  }
  if (total > 0) {
    return {
      state: "healthy" as const,
      label: "连接正常",
      detail: `${total} 项健康检查全部通过`,
      failed,
      total,
    };
  }
  return {
    state: "unknown" as const,
    label: "等待检查",
    detail: "尚未获取系统健康状态",
    failed,
    total,
  };
});

const MIN_HEALTH_CHECK_INTERVAL_MS = 30_000;

/** Explicit, user-initiated diagnostics only. No app-wide health polling. */
export async function refreshGlobalHealth(force = false) {
  if (globalHealthBusy.value) return;
  if (!force && globalHealthLastCheckedAt.value !== null &&
    Date.now() - globalHealthLastCheckedAt.value < MIN_HEALTH_CHECK_INTERVAL_MS) return;
  globalHealthBusy.value = true;
  try {
    globalHealthItems.value = await runGlobalHealthChecks();
    globalHealthError.value = null;
  } catch (error) {
    globalHealthError.value = String(error);
  } finally {
    globalHealthLastCheckedAt.value = Date.now();
    healthClock.value = Date.now();
    globalHealthBusy.value = false;
  }
}

/** Reuse the connection page's diagnostic result instead of checking twice. */
export function recordConnectionHealth(items: HealthItem[], error: string | null = null) {
  globalHealthItems.value = items;
  globalHealthError.value = error;
  globalHealthLastCheckedAt.value = Date.now();
  healthClock.value = Date.now();
}
