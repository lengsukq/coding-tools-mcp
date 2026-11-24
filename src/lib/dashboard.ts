import type { ServiceUsageStats } from "$lib/api/usage";

const CHART_BASELINE_Y = 32;
const CHART_PLOT_HEIGHT = 25;
const CHART_WIDTH = 100;

export interface UsagePoint {
  timestamp: number;
  estimatedTokens: number;
  deltaEstimatedTokens: number;
  requestCount: number;
  toolCallCount: number;
}

export interface UsageChart {
  path: string;
}

export interface UsageTotals {
  estimatedTokens: number;
  estimatedInputTokens: number;
  estimatedOutputTokens: number;
  estimatedToolCallTokens: number;
  requestCount: number;
  toolCallCount: number;
  errorCount: number;
}

export function summarizeUsage(usageByWorkspace: Record<string, ServiceUsageStats[]>): UsageTotals {
  const totals: UsageTotals = {
    estimatedTokens: 0,
    estimatedInputTokens: 0,
    estimatedOutputTokens: 0,
    estimatedToolCallTokens: 0,
    requestCount: 0,
    toolCallCount: 0,
    errorCount: 0,
  };
  for (const stats of Object.values(usageByWorkspace)) {
    for (const item of stats) {
      totals.estimatedTokens += item.estimatedTokens;
      totals.estimatedInputTokens += item.estimatedInputTokens;
      totals.estimatedOutputTokens += item.estimatedOutputTokens;
      totals.estimatedToolCallTokens += item.estimatedToolCallTokens;
      totals.requestCount += item.requestCount;
      totals.toolCallCount += item.toolCallCount;
      totals.errorCount += item.errorCount;
    }
  }
  return totals;
}

export function buildUsagePoint(
  statsByWorkspace: Record<string, ServiceUsageStats[]>,
  previous?: UsagePoint,
): UsagePoint {
  const stats = Object.values(statsByWorkspace).flat();
  const estimatedTokens = stats.reduce((sum, item) => sum + item.estimatedTokens, 0);
  const requestCount = stats.reduce((sum, item) => sum + item.requestCount, 0);
  const toolCallCount = stats.reduce((sum, item) => sum + item.toolCallCount, 0);
  const deltaEstimatedTokens = previous
    ? Math.max(0, estimatedTokens - previous.estimatedTokens)
    : 0;
  return {
    timestamp: Date.now(),
    estimatedTokens,
    deltaEstimatedTokens,
    requestCount,
    toolCallCount,
  };
}

export function buildUsageChart(history: UsagePoint[]): UsageChart {
  if (history.length === 0) {
    return { path: `M 0 ${CHART_BASELINE_Y} L ${CHART_WIDTH} ${CHART_BASELINE_Y}` };
  }

  const values = history.map((point) => point.deltaEstimatedTokens);
  const max = Math.max(...values, 1);
  const samples = values.length === 1 ? [values[0], values[0]] : values;
  const path = samples
    .map((value, index) => {
      const x = (index / (samples.length - 1)) * CHART_WIDTH;
      const y = CHART_BASELINE_Y - (value / max) * CHART_PLOT_HEIGHT;
      return `${index === 0 ? "M" : "L"} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(" ");
  return { path };
}
