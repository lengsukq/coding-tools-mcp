import type { ServiceUsageStats } from "$lib/api/usage";
import { formatCompactNumber } from "$lib/formatters";

export interface UsagePoint {
  timestamp: number;
  estimatedTokens: number;
  deltaEstimatedTokens: number;
  requestCount: number;
  toolCallCount: number;
  averageTokens: number;
}

export interface ChartPoint {
  x: number;
  y: number;
}

export interface UsageChart {
  path: string;
  areaPath: string;
  points: ChartPoint[];
  max: number;
  latest: number;
  average: number;
  sampleCount: number;
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

export function formatCount(value: number): string {
  return formatCompactNumber(value);
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
    averageTokens: toolCallCount === 0 ? 0 : estimatedTokens / toolCallCount,
  };
}

export function buildUsageChart(history: UsagePoint[]): UsageChart {
  if (history.length === 0) {
    return {
      path: "M 0 32 L 100 32",
      areaPath: "M 0 32 L 100 32 L 100 36 L 0 36 Z",
      points: [],
      max: 0,
      latest: 0,
      average: 0,
      sampleCount: 0,
    };
  }

  const values = history.map((point) => point.deltaEstimatedTokens);
  const max = Math.max(...values, 1);
  const samples = values.length === 1 ? [values[0], values[0]] : values;
  const points = samples.map((value, index) => ({
    x: samples.length === 1 ? 50 : (index / (samples.length - 1)) * 100,
    y: 32 - (value / max) * 25,
  }));
  const path = points
    .map((point, index) => `${index === 0 ? "M" : "L"} ${point.x.toFixed(2)} ${point.y.toFixed(2)}`)
    .join(" ");
  const last = points[points.length - 1];
  const first = points[0];
  return {
    path,
    areaPath: `${path} L ${last.x.toFixed(2)} 36 L ${first.x.toFixed(2)} 36 Z`,
    points,
    max,
    latest: values.at(-1) ?? 0,
    average: values.reduce((sum, value) => sum + value, 0) / values.length,
    sampleCount: values.length,
  };
}
