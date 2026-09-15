<script lang="ts">
  import { Activity } from "@lucide/svelte";
  import {
    formatEstimatedTokens,
    formatMetric,
    type UsageChart,
    type UsageTotals,
  } from "$lib/dashboard";

  interface Props {
    totals: UsageTotals;
    averageTokens: number;
    chart: UsageChart;
  }

  let { totals, averageTokens, chart }: Props = $props();
</script>

<section id="dashboard-usage" class="wb-usage-panel">
  <div class="wb-usage-heading">
    <div>
      <div class="flex items-center gap-2">
        <Activity size={16} class="text-[var(--primary)]" />
        <h3>MCP Token 估算趋势</h3>
      </div>
      <p>基于 MCP JSON 请求/响应字节数估算，图表展示最近 24 个采样周期的新增量。</p>
    </div>
    <span class="wb-usage-badge">估算 Token</span>
  </div>

  <div class="wb-usage-layout">
    <div class="wb-usage-chart">
      <div class="wb-usage-chart-meta">
        <span>累计 MCP Token 估算</span>
        <strong>{formatEstimatedTokens(totals.estimatedTokens)}</strong>
      </div>
      <svg
        class="wb-usage-line-chart"
        viewBox="0 0 100 40"
        role="img"
        aria-label={`MCP Token 估算趋势，当前累计 ${formatEstimatedTokens(totals.estimatedTokens)}`}
        preserveAspectRatio="none"
      >
        <line class="wb-usage-gridline" x1="0" y1="7" x2="100" y2="7" />
        <line class="wb-usage-gridline" x1="0" y1="19.5" x2="100" y2="19.5" />
        <line class="wb-usage-gridline" x1="0" y1="32" x2="100" y2="32" />
        <path class="wb-usage-area" d={chart.areaPath} />
        <path class="wb-usage-line" d={chart.path} />
        {#if chart.points.length > 0}
          {@const lastPoint = chart.points[chart.points.length - 1]}
          <circle class="wb-usage-point" cx={lastPoint.x} cy={lastPoint.y} r="1.25" />
        {/if}
      </svg>
      <div class="wb-usage-scale">
        <span>0</span>
        <span>{formatEstimatedTokens(chart.max)} / 采样</span>
      </div>
    </div>

    <div class="wb-usage-stats">
      <div class="wb-usage-stat">
        <span>工具调用</span>
        <strong>{formatMetric(totals.toolCallCount)}</strong>
        <small>tools/call</small>
      </div>
      <div class="wb-usage-stat">
        <span>平均估算 Token / 工具调用</span>
        <strong>{formatMetric(averageTokens)}</strong>
        <small>仅统计 tools/call 输入 + 输出</small>
      </div>
      <div class="wb-usage-stat">
        <span>估算输入 / 输出</span>
        <strong>{formatEstimatedTokens(totals.estimatedInputTokens)} / {formatEstimatedTokens(totals.estimatedOutputTokens)}</strong>
        <small>按 JSON UTF-8 字节估算</small>
      </div>
      <div class="wb-usage-stat">
        <span>全部 MCP 请求</span>
        <strong>{formatMetric(totals.requestCount)}</strong>
        <small>含 initialize / tools/list 等协议请求</small>
      </div>
    </div>
  </div>
</section>
