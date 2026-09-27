import { computed, ref } from "vue";
import { runGlobalHealthChecks, type HealthItem } from "$lib/api/health";

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
      label: "运行正常",
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

let monitorTimer = 0;
let monitorActive = false;

export async function refreshGlobalHealth() {
  if (globalHealthBusy.value) return;
  globalHealthBusy.value = true;
  try {
    globalHealthItems.value = await runGlobalHealthChecks();
    globalHealthError.value = null;
  } catch (error) {
    globalHealthError.value = String(error);
  } finally {
    globalHealthLastCheckedAt.value = Date.now();
    globalHealthBusy.value = false;
  }
}

function refreshWhenVisible() {
  if (document.visibilityState === "visible") void refreshGlobalHealth();
}

export function startGlobalHealthMonitor(intervalMs = 10_000) {
  if (monitorActive) return;
  monitorActive = true;
  void refreshGlobalHealth();
  monitorTimer = window.setInterval(refreshWhenVisible, intervalMs);
  document.addEventListener("visibilitychange", refreshWhenVisible);
  window.addEventListener("focus", refreshWhenVisible);
}

export function stopGlobalHealthMonitor() {
  if (!monitorActive) return;
  monitorActive = false;
  window.clearInterval(monitorTimer);
  monitorTimer = 0;
  document.removeEventListener("visibilitychange", refreshWhenVisible);
  window.removeEventListener("focus", refreshWhenVisible);
}
