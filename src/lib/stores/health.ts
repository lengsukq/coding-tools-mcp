import { computed, ref } from "vue";
import type { HealthItem } from "$lib/api/health";
import { isActionableHealthFailure } from "$lib/ui-model";

const globalHealthItems = ref<HealthItem[]>([]);
const globalHealthError = ref<string | null>(null);

export const globalHealthSummary = computed(() => {
  const failed = globalHealthItems.value.filter(isActionableHealthFailure);
  const total = globalHealthItems.value.length;
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

/** Reuse the connection page's diagnostic result instead of checking twice. */
export function recordConnectionHealth(items: HealthItem[], error: string | null = null) {
  globalHealthItems.value = items;
  globalHealthError.value = error;
}
