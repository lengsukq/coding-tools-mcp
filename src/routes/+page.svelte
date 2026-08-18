<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import {
    Activity,
    ArrowUpRight,
    Boxes,
    CircleAlert,
    CircleCheck,
    FolderKanban,
    Gauge,
    GitBranch,
    LayoutDashboard,
    ListChecks,
    Network,
    Radio,
  } from "@lucide/svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import { getPlanningState, type PlanningStateDto } from "$lib/api/planning";
  import { getLastWorkspaceId } from "$lib/api/settings";
  import { getServiceUsageStats, type ServiceUsageStats } from "$lib/api/usage";
  import { actionsRuntimeStates, mcpRuntimeStates, workspaces } from "$lib/stores/app";
  import { actionsConfig, type RuntimeState, type WorkspaceProfile } from "$lib/types";

  interface UsagePoint {
    timestamp: number;
    estimatedTokens: number;
    requestCount: number;
    averageTokens: number;
  }

  interface ChartPoint {
    x: number;
    y: number;
  }

  let lastWorkspaceId = $state("");
  let planningByWorkspace = $state<Record<string, PlanningStateDto | null>>({});
  let usageByWorkspace = $state<Record<string, ServiceUsageStats[]>>({});
  let usageHistory = $state<UsagePoint[]>([]);
  let usageWorkspaceKey = $state("");
  let planningGeneration = 0;
  let usageGeneration = 0;

  const workspaceCount = $derived($workspaces.length);
  const mcpRunning = $derived(
    $workspaces.filter((workspace) => $mcpRuntimeStates[workspace.id] === "running").length,
  );
  const actionsRunning = $derived(
    $workspaces.filter((workspace) => $actionsRuntimeStates[workspace.id] === "running").length,
  );
  const errorServices = $derived(
    $workspaces.reduce((count, workspace) => {
      return count
        + ($mcpRuntimeStates[workspace.id] === "error" ? 1 : 0)
        + ($actionsRuntimeStates[workspace.id] === "error" ? 1 : 0);
    }, 0),
  );
  const totalServices = $derived(workspaceCount * 2);
  const runningServices = $derived(mcpRunning + actionsRunning);
  const serviceHealth = $derived(
    totalServices === 0 ? 0 : Math.round((runningServices / totalServices) * 100),
  );
  const recentWorkspace = $derived(
    $workspaces.find((workspace) => workspace.id === lastWorkspaceId) ?? $workspaces[0] ?? null,
  );

  const planningStats = $derived.by(() => {
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
  });

  const connectionStats = $derived.by(() => {
    const stats = { gateway: 0, frp: 0, cloudflare: 0, local: 0 };
    for (const workspace of $workspaces) {
      const actions = actionsConfig(workspace);
      const services = [
        {
          global: workspace.tunnel.use_global_gateway ?? false,
          type: workspace.tunnel.type,
        },
        {
          global: actions.use_global_gateway ?? false,
          type: actions.tunnel_type,
        },
      ];
      for (const service of services) {
        if (service.global) stats.gateway += 1;
        else if (service.type === "frp") stats.frp += 1;
        else if (service.type === "cloudflare") stats.cloudflare += 1;
        else stats.local += 1;
      }
    }
    return stats;
  });

  const usageTotals = $derived.by(() => {
    const totals = {
      estimatedTokens: 0,
      estimatedInputTokens: 0,
      estimatedOutputTokens: 0,
      requestCount: 0,
      toolCallCount: 0,
      errorCount: 0,
    };
    for (const stats of Object.values(usageByWorkspace)) {
      for (const item of stats) {
        totals.estimatedTokens += item.estimatedTokens;
        totals.estimatedInputTokens += item.estimatedInputTokens;
        totals.estimatedOutputTokens += item.estimatedOutputTokens;
        totals.requestCount += item.requestCount;
        totals.toolCallCount += item.toolCallCount;
        totals.errorCount += item.errorCount;
      }
    }
    return totals;
  });
  const averageTokens = $derived(
    usageTotals.requestCount === 0
      ? 0
      : usageTotals.estimatedTokens / usageTotals.requestCount,
  );
  const usageChart = $derived.by(() => buildUsageChart(usageHistory));

  function stateLabel(state: RuntimeState | undefined): string {
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

  function stateClass(state: RuntimeState | undefined): string {
    return state ?? "stopped";
  }

  function tunnelLabel(workspace: WorkspaceProfile, service: "mcp" | "actions"): string {
    if (service === "mcp") {
      if (workspace.tunnel.use_global_gateway) return "Global Gateway";
      if (workspace.tunnel.type === "frp") return "FRP";
      if (workspace.tunnel.type === "cloudflare") return "Cloudflare";
      return "Local";
    }
    const actions = actionsConfig(workspace);
    if (actions.use_global_gateway) return "Global Gateway";
    if (actions.tunnel_type === "frp") return "FRP";
    if (actions.tunnel_type === "cloudflare") return "Cloudflare";
    return "Local";
  }

  function planningLabel(workspaceId: string): string {
    const planning = planningByWorkspace[workspaceId];
    if (!planning) return "Planning 未加载";
    const goal = planning.goals.find((item) => item.id === planning.focus_goal_id);
    const plan = planning.plans.find((item) => item.id === planning.focus_plan_id);
    return goal?.title ?? plan?.title ?? `${planning.mode.toUpperCase()} 模式`;
  }

  function percentage(count: number): number {
    return totalServices === 0 ? 0 : Math.round((count / totalServices) * 100);
  }

  async function loadPlanning(items: WorkspaceProfile[]) {
    const generation = ++planningGeneration;
    if (items.length === 0) {
      planningByWorkspace = {};
      return;
    }

    const entries = await Promise.all(
      items.map(async (workspace) => {
        try {
          return [workspace.id, await getPlanningState(workspace.id)] as const;
        } catch {
          return [workspace.id, null] as const;
        }
      }),
    );
    if (generation !== planningGeneration) return;
    planningByWorkspace = Object.fromEntries(entries);
  }

  async function loadUsage(items: WorkspaceProfile[]) {
    const generation = ++usageGeneration;
    if (items.length === 0) {
      usageByWorkspace = {};
      usageHistory = [];
      usageWorkspaceKey = "";
      return;
    }

    const workspaceKey = items.map((item) => item.id).sort().join("|");
    if (workspaceKey !== usageWorkspaceKey) {
      usageWorkspaceKey = workspaceKey;
      usageHistory = [];
    }

    const entries = await Promise.all(
      items.map(async (workspace) => {
        try {
          return [workspace.id, await getServiceUsageStats(workspace.id)] as const;
        } catch {
          return [workspace.id, []] as const;
        }
      }),
    );
    if (generation !== usageGeneration) return;
    const nextUsage = Object.fromEntries(entries);
    usageByWorkspace = nextUsage;
    usageHistory = [...usageHistory, buildUsagePoint(nextUsage)].slice(-24);
  }

  function formatCount(value: number): string {
    return new Intl.NumberFormat("zh-CN", {
      notation: "compact",
      maximumFractionDigits: 1,
    }).format(value);
  }

  function formatMillions(value: number): string {
    return `${(value / 1_000_000).toFixed(2)}M`;
  }

  function formatMetric(value: number): string {
    if (value >= 1_000_000) return formatMillions(value);
    if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
    return Math.round(value).toLocaleString("zh-CN");
  }

  function buildUsagePoint(statsByWorkspace: Record<string, ServiceUsageStats[]>): UsagePoint {
    const stats = Object.values(statsByWorkspace).flat();
    const estimatedTokens = stats.reduce((sum, item) => sum + item.estimatedTokens, 0);
    const requestCount = stats.reduce((sum, item) => sum + item.requestCount, 0);
    return {
      timestamp: Date.now(),
      estimatedTokens,
      requestCount,
      averageTokens: requestCount === 0 ? 0 : estimatedTokens / requestCount,
    };
  }

  function buildUsageChart(history: UsagePoint[]): {
    path: string;
    areaPath: string;
    points: ChartPoint[];
    max: number;
  } {
    if (history.length === 0) {
      return {
        path: "M 0 32 L 100 32",
        areaPath: "M 0 32 L 100 32 L 100 36 L 0 36 Z",
        points: [],
        max: 0,
      };
    }

    const values = history.map((point) => point.estimatedTokens);
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
    };
  }

  function openWorkspace(id: string) {
    goto(`/workspace/${id}`);
  }

  onMount(() => {
    const usageTimer = window.setInterval(() => {
      void loadUsage($workspaces);
    }, 5000);
    void getLastWorkspaceId()
      .then((id) => {
        lastWorkspaceId = id ?? "";
      })
      .catch(() => {
        lastWorkspaceId = "";
      });

    return () => window.clearInterval(usageTimer);
  });

  $effect(() => {
    const items = $workspaces;
    void loadPlanning(items);
    void loadUsage(items);
  });
</script>

<section class="page-scroll tx-dashboard-page">
  <header class="page-header tx-dashboard-header">
    <div>
      <div class="flex items-center gap-2">
        <LayoutDashboard size={15} class="text-[var(--primary)]" />
        <p class="page-kicker">全局控制台</p>
      </div>
      <h2 class="page-title">Dashboard</h2>
      <p class="mt-2 max-w-2xl text-sm text-[var(--color-text-muted)]">
        不进入具体工作区，也可以查看服务状态、Token 趋势、连接方式和 AI Planning 进度。
      </p>
    </div>
  </header>

  <div class="page-body tx-dashboard-body">
    {#if workspaceCount === 0}
      <div class="tx-dashboard-empty">
        <EmptyState />
      </div>
    {:else}
      <div class="tx-dashboard-canvas">
        <nav class="tx-dashboard-quick-nav" aria-label="Dashboard 快速导航">
          <span class="tx-dashboard-quick-nav-label">快速跳转</span>
          <a href="#dashboard-overview" aria-label="跳转到运行总览" title="运行总览">
            <Gauge size={15} />
          </a>
          <a href="#dashboard-metrics" aria-label="跳转到关键指标" title="关键指标">
            <Activity size={15} />
          </a>
          <a href="#dashboard-workspaces" aria-label="跳转到工作区运行矩阵" title="运行矩阵">
            <Boxes size={15} />
          </a>
          <a href="#dashboard-usage" aria-label="跳转到 Token 趋势" title="Token 趋势">
            <Network size={15} />
          </a>
          <a href="#dashboard-details" aria-label="跳转到连接与 Planning" title="连接与 Planning">
            <ListChecks size={15} />
          </a>
        </nav>

        <div class="tx-dashboard-content">
      <div id="dashboard-overview" class="tx-dashboard-hero-grid tx-dashboard-anchor">
        <section class="tx-card tx-dashboard-health-card">
          <div class="tx-dashboard-health-copy">
            <div class="flex items-center gap-2">
              <Gauge size={16} class="text-[var(--primary)]" />
              <p class="tx-section-label">全局运行健康度</p>
            </div>
            <strong>{runningServices} / {totalServices} 个服务在线</strong>
            <p>
              {#if errorServices > 0}
                当前有 {errorServices} 个服务处于异常状态，建议优先进入对应工作区查看日志。
              {:else if runningServices === totalServices}
                所有 MCP 与 Actions 服务都处于运行状态。
              {:else}
                当前没有运行时异常，{totalServices - runningServices} 个服务处于停止或切换状态。
              {/if}
            </p>
          </div>
          <div class="tx-dashboard-ring" aria-label={`服务在线率 ${serviceHealth}%`}>
            <svg viewBox="0 0 120 120" role="img" aria-hidden="true">
              <circle class="tx-dashboard-ring-track" cx="60" cy="60" r="48" pathLength="100" />
              <circle
                class="tx-dashboard-ring-value"
                cx="60"
                cy="60"
                r="48"
                pathLength="100"
                stroke-dasharray={`${serviceHealth} 100`}
              />
            </svg>
            <div>
              <strong>{serviceHealth}%</strong>
              <span>在线率</span>
            </div>
          </div>
        </section>

        <section class="tx-card tx-dashboard-recent-card">
          <div class="flex items-center justify-between gap-3">
            <div class="flex items-center gap-2">
              <FolderKanban size={16} class="text-[var(--primary)]" />
              <p class="tx-section-label">最近工作区</p>
            </div>
            {#if recentWorkspace}
              <button class="tx-icon-button" type="button" title="打开工作区" onclick={() => openWorkspace(recentWorkspace!.id)}>
                <ArrowUpRight size={15} />
              </button>
            {/if}
          </div>
          {#if recentWorkspace}
            <div class="tx-dashboard-recent-main">
              <strong>{recentWorkspace.name}</strong>
              <span>{recentWorkspace.path}</span>
            </div>
            <div class="tx-dashboard-service-pair">
              <div class="tx-dashboard-service-state {stateClass($mcpRuntimeStates[recentWorkspace.id])}">
                <Radio size={13} />
                <span>MCP</span>
                <strong>{stateLabel($mcpRuntimeStates[recentWorkspace.id])}</strong>
              </div>
              <div class="tx-dashboard-service-state {stateClass($actionsRuntimeStates[recentWorkspace.id])}">
                <Activity size={13} />
                <span>Actions</span>
                <strong>{stateLabel($actionsRuntimeStates[recentWorkspace.id])}</strong>
              </div>
            </div>
          {/if}
        </section>
      </div>

      <div id="dashboard-metrics" class="tx-dashboard-metrics tx-dashboard-anchor">
        <div class="tx-dashboard-metric-card">
          <span>工作区</span>
          <strong>{workspaceCount}</strong>
          <small>{runningServices > 0 ? `${runningServices} 个服务在线` : "当前全部停止"}</small>
        </div>
        <div class="tx-dashboard-metric-card">
          <span>MCP 在线</span>
          <strong>{mcpRunning}</strong>
          <small>{workspaceCount - mcpRunning} 个未运行</small>
        </div>
        <div class="tx-dashboard-metric-card">
          <span>Actions 在线</span>
          <strong>{actionsRunning}</strong>
          <small>{workspaceCount - actionsRunning} 个未运行</small>
        </div>
        <div class="tx-dashboard-metric-card">
          <span>待人工验收</span>
          <strong>{planningStats.pendingReview}</strong>
          <small>{planningStats.activeGoals} Goal · {planningStats.activePlans} Plan 活跃</small>
        </div>
        <div class="tx-dashboard-metric-card">
          <span>Token 估算</span>
          <strong>{formatCount(usageTotals.estimatedTokens)}</strong>
          <small>{formatCount(usageTotals.requestCount)} 次服务请求 · 当前会话</small>
        </div>
      </div>

      <section id="dashboard-workspaces" class="tx-dashboard-section tx-dashboard-anchor">
        <div class="tx-dashboard-section-heading">
          <div>
            <div class="flex items-center gap-2">
              <Boxes size={16} class="text-[var(--primary)]" />
              <h3>工作区运行矩阵</h3>
            </div>
            <p>把运行状态、端口、连接方式和当前 Planning 焦点放在同一个视图中。</p>
          </div>
          <span class="tx-dashboard-badge">{workspaceCount} Workspaces</span>
        </div>

        <div class="tx-dashboard-workspace-grid">
          {#each $workspaces as workspace (workspace.id)}
            {@const actions = actionsConfig(workspace)}
            {@const planning = planningByWorkspace[workspace.id]}
            <button class="tx-dashboard-workspace-card" type="button" onclick={() => openWorkspace(workspace.id)}>
              <div class="tx-dashboard-workspace-topline">
                <div class="min-w-0">
                  <strong class="truncate">{workspace.name}</strong>
                  <span class="truncate">{workspace.path}</span>
                </div>
                <ArrowUpRight size={15} />
              </div>

              <div class="tx-dashboard-runtime-grid">
                <div class="tx-dashboard-runtime-block">
                  <div class="tx-dashboard-runtime-title">
                    <span class="tx-dashboard-dot {stateClass($mcpRuntimeStates[workspace.id])}"></span>
                    <strong>MCP</strong>
                    <small>{stateLabel($mcpRuntimeStates[workspace.id])}</small>
                  </div>
                  <div class="tx-dashboard-runtime-meta">
                    <span>:{workspace.runtime.local_port}</span>
                    <span>{tunnelLabel(workspace, "mcp")}</span>
                  </div>
                </div>
                <div class="tx-dashboard-runtime-block">
                  <div class="tx-dashboard-runtime-title">
                    <span class="tx-dashboard-dot {stateClass($actionsRuntimeStates[workspace.id])}"></span>
                    <strong>Actions</strong>
                    <small>{stateLabel($actionsRuntimeStates[workspace.id])}</small>
                  </div>
                  <div class="tx-dashboard-runtime-meta">
                    <span>:{actions.local_port}</span>
                    <span>{tunnelLabel(workspace, "actions")}</span>
                  </div>
                </div>
              </div>

              <div class="tx-dashboard-planning-line">
                <GitBranch size={13} />
                <span class="truncate">{planningLabel(workspace.id)}</span>
                {#if planning}
                  <small>{planning.mode.toUpperCase()}</small>
                {/if}
              </div>
            </button>
          {/each}
        </div>
      </section>

      <div id="dashboard-details" class="tx-dashboard-detail-grid tx-dashboard-anchor">
        <section id="dashboard-usage" class="tx-card tx-dashboard-detail-card tx-dashboard-usage-card">
          <div class="tx-dashboard-usage-heading">
            <div>
              <div class="flex items-center gap-2">
                <Activity size={16} class="text-[var(--primary)]" />
                <h3>Token 使用趋势</h3>
              </div>
              <p>基于服务内置统计的累计估算，最近 24 个采样点。</p>
            </div>
            <span class="tx-dashboard-usage-badge">Token / M</span>
          </div>

          <div class="tx-dashboard-usage-layout">
            <div class="tx-dashboard-usage-chart">
              <div class="tx-dashboard-usage-chart-meta">
                <span>累计 Token</span>
                <strong>{formatMillions(usageTotals.estimatedTokens)}</strong>
              </div>
              <svg
                class="tx-dashboard-line-chart"
                viewBox="0 0 100 40"
                role="img"
                aria-label={`Token 使用趋势，当前 ${formatMillions(usageTotals.estimatedTokens)}`}
                preserveAspectRatio="none"
              >
                <line class="tx-dashboard-chart-gridline" x1="0" y1="7" x2="100" y2="7" />
                <line class="tx-dashboard-chart-gridline" x1="0" y1="19.5" x2="100" y2="19.5" />
                <line class="tx-dashboard-chart-gridline" x1="0" y1="32" x2="100" y2="32" />
                <path class="tx-dashboard-chart-area" d={usageChart.areaPath} />
                <path class="tx-dashboard-chart-line" d={usageChart.path} />
                {#if usageChart.points.length > 0}
                  {@const lastPoint = usageChart.points[usageChart.points.length - 1]}
                  <circle class="tx-dashboard-chart-point" cx={lastPoint.x} cy={lastPoint.y} r="1.25" />
                {/if}
              </svg>
              <div class="tx-dashboard-chart-scale">
                <span>0M</span>
                <span>{formatMillions(usageChart.max)}</span>
              </div>
            </div>

            <div class="tx-dashboard-usage-stats">
              <div class="tx-dashboard-usage-stat">
                <span>请求次数</span>
                <strong>{formatMetric(usageTotals.requestCount)}</strong>
                <small>全部服务</small>
              </div>
              <div class="tx-dashboard-usage-stat">
                <span>平均 Token / 请求</span>
                <strong>{formatMetric(averageTokens)}</strong>
                <small>输入 + 输出</small>
              </div>
              <div class="tx-dashboard-usage-stat">
                <span>输入 / 输出</span>
                <strong>{formatMillions(usageTotals.estimatedInputTokens)} / {formatMillions(usageTotals.estimatedOutputTokens)}</strong>
                <small>估算 Token</small>
              </div>
            </div>
          </div>
        </section>

        <section class="tx-card tx-dashboard-detail-card">
          <div class="tx-dashboard-section-heading compact">
            <div>
              <div class="flex items-center gap-2">
                <Network size={16} class="text-[var(--primary)]" />
                <h3>服务连接方式</h3>
              </div>
              <p>统计 MCP 与 Actions 当前配置的公网暴露方式。</p>
            </div>
          </div>
          <div class="tx-dashboard-bars">
            {#each [
              ["Global Gateway", connectionStats.gateway],
              ["FRP", connectionStats.frp],
              ["Cloudflare", connectionStats.cloudflare],
              ["仅本地", connectionStats.local],
            ] as item}
              <div class="tx-dashboard-bar-row">
                <div><span>{item[0]}</span><strong>{item[1]}</strong></div>
                <div class="tx-dashboard-bar-track">
                  <span style={`width: ${percentage(Number(item[1]))}%`}></span>
                </div>
              </div>
            {/each}
          </div>
        </section>

        <section class="tx-card tx-dashboard-detail-card">
          <div class="tx-dashboard-section-heading compact">
            <div>
              <div class="flex items-center gap-2">
                <ListChecks size={16} class="text-[var(--primary)]" />
                <h3>AI Planning</h3>
              </div>
              <p>跨工作区汇总当前执行约束与人工验收队列。</p>
            </div>
          </div>
          <div class="tx-dashboard-planning-stats">
            <div>
              <CircleCheck size={15} />
              <span>活跃 Goal</span>
              <strong>{planningStats.activeGoals}</strong>
            </div>
            <div>
              <GitBranch size={15} />
              <span>活跃 Plan</span>
              <strong>{planningStats.activePlans}</strong>
            </div>
            <div class:attention={planningStats.pendingReview > 0}>
              <CircleAlert size={15} />
              <span>待验收</span>
              <strong>{planningStats.pendingReview}</strong>
            </div>
          </div>
          <div class="tx-dashboard-mode-strip">
            <span><strong>{planningStats.modes.direct}</strong> Direct</span>
            <span><strong>{planningStats.modes.plan}</strong> Plan</span>
            <span><strong>{planningStats.modes.goal}</strong> Goal</span>
          </div>
        </section>

        <section class="tx-card tx-dashboard-detail-card">
          <div class="tx-dashboard-section-heading compact">
            <div>
              <div class="flex items-center gap-2">
                <Activity size={16} class="text-[var(--primary)]" />
                <h3>服务 Token 用量</h3>
              </div>
              <p>由本地 MCP / Actions 服务统计 JSON 请求大小后估算，不保存请求正文。</p>
            </div>
          </div>
          <div class="tx-dashboard-planning-stats">
            <div>
              <span>输入 Token</span>
              <strong>{formatCount(usageTotals.estimatedInputTokens)}</strong>
            </div>
            <div>
              <span>输出 Token</span>
              <strong>{formatCount(usageTotals.estimatedOutputTokens)}</strong>
            </div>
            <div class:attention={usageTotals.errorCount > 0}>
              <span>错误请求</span>
              <strong>{formatCount(usageTotals.errorCount)}</strong>
            </div>
          </div>
          <div class="tx-dashboard-mode-strip">
            <span><strong>{formatCount(usageTotals.toolCallCount)}</strong> 工具调用</span>
            <span><strong>{formatCount(usageTotals.requestCount)}</strong> 请求</span>
            <span>重启后仍保留本次应用会话累计</span>
          </div>
        </section>
      </div>
        </div>
      </div>
    {/if}
  </div>
</section>
