<script setup lang="ts">
import { Activity, ArrowUpRight, CircleAlert, MousePointerClick, Sigma, Waves } from "@lucide/vue";
import { computed } from "vue";
import { formatCount, type UsageChart } from "$lib/dashboard";

const props = defineProps<{
  totals: {
    estimatedTokens: number;
    estimatedInputTokens: number;
    estimatedOutputTokens: number;
    estimatedToolCallTokens: number;
    toolCallCount: number;
    requestCount: number;
    errorCount: number;
  };
  averageTokens: number;
  chart: UsageChart;
}>();

const points = computed(() => props.chart?.points ?? []);
const totalIoTokens = computed(() => props.totals.estimatedInputTokens + props.totals.estimatedOutputTokens);
const inputShare = computed(() => totalIoTokens.value > 0 ? Math.round((props.totals.estimatedInputTokens / totalIoTokens.value) * 100) : 0);
const outputShare = computed(() => totalIoTokens.value > 0 ? Math.max(0, 100 - inputShare.value) : 0);
const errorRate = computed(() => props.totals.requestCount > 0 ? (props.totals.errorCount / props.totals.requestCount) * 100 : 0);
const latestLabel = computed(() => props.chart.latest > 0 ? `+${formatCount(props.chart.latest)}` : "0");
</script>

<template>
  <article class="wb-analytics-card">
    <div class="wb-analytics-head">
      <div>
        <div class="wb-analytics-eyebrow"><Activity :size="13" /> MCP Token Flow</div>
        <div class="wb-analytics-total">{{ formatCount(totals.estimatedTokens) }}</div>
        <p>累计估算 Tokens · JSON 传输量口径</p>
      </div>
      <div class="wb-live-delta" :class="{ active: chart.latest > 0 }">
        <ArrowUpRight :size="12" />
        <div><span>最近采样</span><strong>{{ latestLabel }}</strong></div>
      </div>
    </div>

    <div class="wb-token-chart-shell">
      <div class="wb-token-y-axis">
        <span>{{ formatCount(chart.max) }}</span>
        <span>{{ formatCount(chart.max / 2) }}</span>
        <span>0</span>
      </div>
      <div class="wb-token-chart-main">
        <svg v-if="points.length > 1" viewBox="0 0 100 40" preserveAspectRatio="none" class="wb-token-chart">
          <defs>
            <linearGradient id="usageAreaGradient" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0" stop-color="var(--coral-accent, var(--primary))" stop-opacity=".32" />
              <stop offset=".56" stop-color="var(--coral-peach, var(--primary))" stop-opacity=".12" />
              <stop offset="1" stop-color="var(--coral-primary, var(--primary))" stop-opacity="0" />
            </linearGradient>
            <linearGradient id="usageLineGradient" x1="0" x2="1" y1="0" y2="0">
              <stop offset="0" stop-color="var(--pal-primary-light, #FCB6AD)" />
              <stop offset=".55" stop-color="var(--pal-primary, #E5665B)" />
              <stop offset="1" stop-color="var(--pal-primary-dark, #e2574c)" />
            </linearGradient>
          </defs>
          <line v-for="y in [7, 19.5, 32]" :key="y" x1="0" x2="100" :y1="y" :y2="y" class="wb-token-gridline" />
          <path :d="chart.areaPath" fill="url(#usageAreaGradient)" />
          <path :d="chart.path" fill="none" stroke="url(#usageLineGradient)" class="wb-token-line" />
          <circle
            v-for="(point, index) in points"
            :key="`${point.x}-${index}`"
            :cx="point.x"
            :cy="point.y"
            :r="index === points.length - 1 ? 1.7 : .8"
            :class="index === points.length - 1 ? 'wb-token-point wb-token-point--latest' : 'wb-token-point'"
          />
        </svg>
        <div v-else class="wb-token-empty"><Waves :size="18" /><span>正在积累趋势样本</span></div>
        <div class="wb-token-x-axis"><span>较早</span><span>{{ chart.sampleCount }} 个样本</span><span>现在</span></div>
      </div>
    </div>

    <div class="wb-analytics-metrics">
      <div><MousePointerClick :size="14" /><span>Tool Calls</span><strong>{{ formatCount(totals.toolCallCount) }}</strong></div>
      <div><Waves :size="14" /><span>Requests</span><strong>{{ formatCount(totals.requestCount) }}</strong></div>
      <div><Sigma :size="14" /><span>Avg / Call</span><strong>{{ formatCount(averageTokens) }}</strong></div>
      <div :class="{ alert: totals.errorCount > 0 }"><CircleAlert :size="14" /><span>Error Rate</span><strong>{{ errorRate.toFixed(errorRate >= 10 ? 0 : 1) }}%</strong></div>
    </div>

    <div class="wb-token-split">
      <div class="wb-token-split-head"><span>Token I/O 分布</span><small>{{ formatCount(totalIoTokens) }} I/O Tokens</small></div>
      <div class="wb-token-split-track">
        <i class="input" :style="{ width: `${inputShare}%` }" />
        <i class="output" :style="{ width: `${outputShare}%` }" />
      </div>
      <div class="wb-token-split-legend">
        <span><i class="input" />Input <strong>{{ formatCount(totals.estimatedInputTokens) }}</strong><small>{{ inputShare }}%</small></span>
        <span><i class="output" />Output <strong>{{ formatCount(totals.estimatedOutputTokens) }}</strong><small>{{ outputShare }}%</small></span>
      </div>
    </div>
  </article>
</template>
