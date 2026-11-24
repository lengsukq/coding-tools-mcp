<script setup lang="ts">
import { computed } from "vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import type { ServiceUsageStats } from "$lib/api/usage";
import type { UsageChart, UsagePoint, UsageTotals } from "$lib/dashboard";
import type { DashboardWorkspaceRow } from "$lib/dashboard-types";
import { formatCompactNumber } from "$lib/formatters";

const props = defineProps<{
  auxiliaryError: string;
  totals: UsageTotals;
  samples: UsagePoint[];
  chart: UsageChart;
  ordered: DashboardWorkspaceRow[];
  usage: Record<string, ServiceUsageStats[]>;
}>();

const tokensByWorkspace = computed(() => {
  const result = new Map<string, number>();
  for (const [id, stats] of Object.entries(props.usage)) {
    let sum = 0;
    for (const item of stats) sum += item.estimatedTokens;
    result.set(id, sum);
  }
  return result;
});
</script>

<template>
  <GlassCard>
    <div class="ui-section-heading">
      <div>
        <h2>MCP 传输用量分析</h2>
        <p>仅在停留本页期间每 30 秒按需采样 JSON 传输与工具调用估算（不代表模型账单）。</p>
      </div>
      <span class="rounded-full bg-[var(--ui-accent-soft)] px-3 py-1 text-xs font-medium text-[var(--ui-accent)]">
        实时按需采样中
      </span>
    </div>

    <div class="space-y-4">
      <p v-if="auxiliaryError" class="ui-danger text-xs" role="alert">分析更新失败：{{ auxiliaryError }}。保留上次数据。</p>
      <div class="ui-metric-grid">
        <div>
          <span>估算 Tokens</span>
          <strong>{{ formatCompactNumber(totals.estimatedTokens) }}</strong>
        </div>
        <div>
          <span>工具调用</span>
          <strong>{{ formatCompactNumber(totals.toolCallCount) }}</strong>
        </div>
        <div>
          <span>总请求数</span>
          <strong>{{ formatCompactNumber(totals.requestCount) }}</strong>
        </div>
        <div>
          <span>错误响应</span>
          <strong>{{ formatCompactNumber(totals.errorCount) }}</strong>
        </div>
      </div>

      <div class="ui-inset p-4">
        <div class="flex items-center justify-between gap-2">
          <h3 class="text-sm font-semibold">传输量采样趋势</h3>
          <span class="ui-muted">停留期间每 30 秒采样</span>
        </div>
        <svg
          v-if="samples.length > 1"
          viewBox="0 0 100 40"
          preserveAspectRatio="none"
          class="ui-usage-chart"
          role="img"
          aria-label="估算 Token 传输增量趋势"
        >
          <path :d="chart.path" fill="none" stroke="var(--ui-accent)" stroke-width="1.5" vector-effect="non-scaling-stroke" />
        </svg>
        <p v-else class="py-6 text-center text-xs text-[var(--ui-text-muted)]">正在积累趋势样本，下一次采样后自动绘制曲线…</p>
        <p class="ui-muted">
          输入 {{ formatCompactNumber(totals.estimatedInputTokens) }} · 输出 {{ formatCompactNumber(totals.estimatedOutputTokens) }} · 工具载荷
          {{ formatCompactNumber(totals.estimatedToolCallTokens) }}
        </p>
      </div>

      <div v-if="ordered.length" class="space-y-0.5">
        <p v-for="row in ordered" :key="row.workspace.id" class="ui-data-row text-xs">
          <span class="font-medium text-[var(--ui-text-secondary)]">{{ row.workspace.name }}</span>
          <strong class="font-mono">{{ formatCompactNumber(tokensByWorkspace.get(row.workspace.id) ?? 0) }}</strong>
        </p>
      </div>
    </div>
  </GlassCard>
</template>
