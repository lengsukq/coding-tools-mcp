import { getPlanningState, type PlanningStateDto } from "$lib/api/planning";
import { getServiceUsageStats, type ServiceUsageStats } from "$lib/api/usage";
import type { RuntimeState, WorkspaceProfile } from "$lib/types";

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

export function stateLabel(state: RuntimeState | undefined): string {
  switch (state) {
    case "running":
      return "运行中";
    case "starting":
      return "启动中";
    case "stopping":
      return "停止中";
    case "error":
      return "异常";
    default:
      return "已停止";
  }
}

export function stateClass(state: RuntimeState | undefined): string {
  return state ?? "stopped";
}

export function planningLabel(
  planningByWorkspace: Record<string, PlanningStateDto | null>,
  workspaceId: string,
): string {
  const planning = planningByWorkspace[workspaceId];
  if (!planning) return "Planning 未加载";
  const goal = planning.goals.find((item) => item.id === planning.focus_goal_id);
  const plan = planning.plans.find((item) => item.id === planning.focus_plan_id);
  return goal?.title ?? plan?.title ?? `${planning.mode.toUpperCase()} 模式`;
}

export function summarizePlanning(planningByWorkspace: Record<string, PlanningStateDto | null>) {
  let activeGoals = 0;
  let activePlans = 0;
  let pendingReview = 0;
  const modes = { direct: 0, plan: 0, goal: 0 };

  for (const planning of Object.values(planningByWorkspace)) {
    if (!planning) continue;
    modes[planning.mode] += 1;
    activeGoals += planning.goals.filter((goal) => ["active", "paused"].includes(goal.status)).length;
    activePlans += planning.plans.filter((plan) => ["draft", "active", "paused"].includes(plan.status)).length;
    pendingReview += planning.goals.filter((goal) => goal.status === "awaiting_acceptance").length;
    pendingReview += planning.plans.filter((plan) => plan.status === "awaiting_acceptance").length;
  }

  return { activeGoals, activePlans, pendingReview, modes };
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

export async function loadPlanningByWorkspace(
  items: WorkspaceProfile[],
): Promise<Record<string, PlanningStateDto | null>> {
  const entries = await Promise.all(
    items.map(async (workspace) => {
      try {
        return [workspace.id, await getPlanningState(workspace.id)] as const;
      } catch {
        return [workspace.id, null] as const;
      }
    }),
  );
  return Object.fromEntries(entries);
}

export async function loadUsageByWorkspace(
  items: WorkspaceProfile[],
): Promise<Record<string, ServiceUsageStats[]>> {
  const entries = await Promise.all(
    items.map(async (workspace) => {
      try {
        return [workspace.id, await getServiceUsageStats(workspace.id)] as const;
      } catch {
        return [workspace.id, []] as const;
      }
    }),
  );
  return Object.fromEntries(entries);
}

export function formatCount(value: number): string {
  return new Intl.NumberFormat("zh-CN", {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(value);
}

export function formatMillions(value: number): string {
  return `${(value / 1_000_000).toFixed(2)}M`;
}

export function formatMetric(value: number): string {
  if (value >= 1_000_000) return formatMillions(value);
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return Math.round(value).toLocaleString("zh-CN");
}

export function formatEstimatedTokens(value: number): string {
  if (value >= 1_000_000) return formatMillions(value);
  if (value >= 1_000) return `${(value / 1_000).toFixed(value >= 100_000 ? 0 : 1)}K`;
  return Math.round(value).toLocaleString("zh-CN");
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
