<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import {
    Activity,
    ArrowUpRight,
    Boxes,
    Check,
    CircleAlert,
    CircleCheck,
    Copy,
    FolderKanban,
    FolderOpen,
    Gauge,
    GitBranch,
    LayoutDashboard,
    ListChecks,
    Network,
    Play,
    Radio,
    RotateCw,
    Square,
  } from "@lucide/svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import type { PlanningStateDto } from "$lib/api/planning";
  import { getLastWorkspaceId } from "$lib/api/settings";
  import type { ServiceUsageStats } from "$lib/api/usage";
  import {
    buildUsageChart,
    buildUsagePoint,
    formatCount,
    formatMillions,
    formatMetric,
    loadPlanningByWorkspace,
    loadUsageByWorkspace,
    planningLabel as getPlanningLabel,
    stateClass,
    stateLabel,
    summarizeConnections,
    summarizePlanning,
    summarizeUsage,
    tunnelLabel,
    type UsagePoint,
  } from "$lib/dashboard";
  import {
    openWorkspaceDirectory,
    startRuntime,
    stopRuntime,
  } from "$lib/api/workspaces";
  import { runServiceToggle } from "$lib/runtime/service";
  import { showToast } from "$lib/stores/toast";
  import { mcpRuntimeStates, workspaces } from "$lib/stores/app";
  import type { WorkspaceProfile } from "$lib/types";

  let lastWorkspaceId = $state("");
  let planningByWorkspace = $state<Record<string, PlanningStateDto | null>>({});
  let usageByWorkspace = $state<Record<string, ServiceUsageStats[]>>({});
  let usageHistory = $state<UsagePoint[]>([]);
  let usageWorkspaceKey = $state("");
  let planningGeneration = 0;
  let usageGeneration = 0;
  let mcpBusyMap = $state<Record<string, boolean>>({});
  let copiedPathId = $state<string | null>(null);
  let copiedEndpointId = $state<string | null>(null);

  const workspaceCount = $derived($workspaces.length);
  const mcpRunning = $derived(
    $workspaces.filter((workspace) => $mcpRuntimeStates[workspace.id] === "running").length,
  );
  const errorServices = $derived(
    $workspaces.reduce((count, workspace) => {
      return count + ($mcpRuntimeStates[workspace.id] === "error" ? 1 : 0);
    }, 0),
  );
  const totalServices = $derived(workspaceCount);
  const runningServices = $derived(mcpRunning);
  const serviceHealth = $derived(
    totalServices === 0 ? 0 : Math.round((runningServices / totalServices) * 100),
  );
  const recentWorkspace = $derived(
    $workspaces.find((workspace) => workspace.id === lastWorkspaceId) ?? $workspaces[0] ?? null,
  );

  const planningStats = $derived.by(() => summarizePlanning(planningByWorkspace));
  const connectionStats = $derived.by(() => summarizeConnections($workspaces));
  const usageTotals = $derived.by(() => summarizeUsage(usageByWorkspace));
  const averageTokens = $derived(
    usageTotals.requestCount === 0
      ? 0
      : usageTotals.estimatedTokens / usageTotals.requestCount,
  );
  const usageChart = $derived.by(() => buildUsageChart(usageHistory));

  function planningLabel(workspaceId: string): string {
    return getPlanningLabel(planningByWorkspace, workspaceId);
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

    const nextPlanning = await loadPlanningByWorkspace(items);
    if (generation !== planningGeneration) return;
    planningByWorkspace = nextPlanning;
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

    const nextUsage = await loadUsageByWorkspace(items);
    if (generation !== usageGeneration) return;
    usageByWorkspace = nextUsage;
    usageHistory = [...usageHistory, buildUsagePoint(nextUsage)].slice(-24);
  }

  function openWorkspace(id: string) {
    goto(`/workspace/${id}`);
  }

  async function toggleWorkspaceMcp(id: string) {
    if (mcpBusyMap[id]) return;
    const currentState = $mcpRuntimeStates[id];
    const wasRunning = currentState === "running";
    mcpBusyMap = { ...mcpBusyMap, [id]: true };
    try {
      const status = await runServiceToggle(
        wasRunning,
        () => startRuntime(id),
        () => stopRuntime(id),
        "MCP",
      );
      if (status) {
        mcpRuntimeStates.update((map) => ({ ...map, [id]: status.state }));
      }
    } finally {
      mcpBusyMap = { ...mcpBusyMap, [id]: false };
    }
  }

  async function copyWorkspacePath(id: string, path: string) {
    try {
      await navigator.clipboard.writeText(path);
      copiedPathId = id;
      setTimeout(() => {
        if (copiedPathId === id) copiedPathId = null;
      }, 2000);
      showToast("工作区物理路径已复制", { kind: "success", duration: 2500 });
    } catch {
      showToast("复制路径失败", { kind: "error" });
    }
  }

  async function revealDirectory(path: string) {
    try {
      await openWorkspaceDirectory(path);
    } catch (err) {
      showToast(`打开目录失败: ${err instanceof Error ? err.message : String(err)}`, { kind: "error" });
    }
  }

  let activeSection = $state("dashboard-overview");

  function scrollToAnchor(event: MouseEvent, targetId: string) {
    event.preventDefault();
    const el = document.getElementById(targetId);
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "start" });
      activeSection = targetId;
      try {
        history.replaceState(null, "", `#${targetId}`);
      } catch {
        // ignore
      }
    }
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

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            activeSection = entry.target.id;
          }
        }
      },
      { rootMargin: "-10% 0px -70% 0px" },
    );

    const anchorIds = [
      "dashboard-overview",
      "dashboard-metrics",
      "dashboard-workspaces",
      "dashboard-usage",
      "dashboard-details",
    ];
    for (const id of anchorIds) {
      const el = document.getElementById(id);
      if (el) observer.observe(el);
    }

    return () => {
      window.clearInterval(usageTimer);
      observer.disconnect();
    };
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
          <a
            href="#dashboard-overview"
            class:active={activeSection === "dashboard-overview"}
            aria-label="跳转到运行总览"
            title="运行总览"
            onclick={(e) => scrollToAnchor(e, "dashboard-overview")}
          >
            <Gauge size={15} />
          </a>
          <a
            href="#dashboard-metrics"
            class:active={activeSection === "dashboard-metrics"}
            aria-label="跳转到关键指标"
            title="关键指标"
            onclick={(e) => scrollToAnchor(e, "dashboard-metrics")}
          >
            <Activity size={15} />
          </a>
          <a
            href="#dashboard-workspaces"
            class:active={activeSection === "dashboard-workspaces"}
            aria-label="跳转到工作区运行矩阵"
            title="运行矩阵"
            onclick={(e) => scrollToAnchor(e, "dashboard-workspaces")}
          >
            <Boxes size={15} />
          </a>
          <a
            href="#dashboard-usage"
            class:active={activeSection === "dashboard-usage"}
            aria-label="跳转到 Token 趋势"
            title="Token 趋势"
            onclick={(e) => scrollToAnchor(e, "dashboard-usage")}
          >
            <Network size={15} />
          </a>
          <a
            href="#dashboard-details"
            class:active={activeSection === "dashboard-details"}
            aria-label="跳转到连接与 Planning"
            title="连接与 Planning"
            onclick={(e) => scrollToAnchor(e, "dashboard-details")}
          >
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
                所有 MCP 服务都处于运行状态。
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
              <div class="flex items-center gap-1.5">
                <button
                  class="tx-dashboard-action-icon"
                  type="button"
                  title="在访达/资源管理器中打开"
                  onclick={() => void revealDirectory(recentWorkspace!.path)}
                >
                  <FolderOpen size={13} />
                </button>
                <button
                  class="tx-dashboard-action-icon"
                  type="button"
                  title="复制物理路径"
                  onclick={() => void copyWorkspacePath(recentWorkspace!.id, recentWorkspace!.path)}
                >
                  {#if copiedPathId === recentWorkspace.id}
                    <Check size={13} class="text-[var(--success)]" />
                  {:else}
                    <Copy size={13} />
                  {/if}
                </button>
                <button
                  class="tx-dashboard-action-icon primary"
                  type="button"
                  title="进入工作区"
                  onclick={() => openWorkspace(recentWorkspace!.id)}
                >
                  <ArrowUpRight size={14} />
                </button>
              </div>
            {/if}
          </div>
          {#if recentWorkspace}
            <div class="tx-dashboard-recent-main">
              <button
                type="button"
                class="text-left font-bold truncate block hover:text-[var(--primary)] transition-colors cursor-pointer"
                onclick={() => openWorkspace(recentWorkspace!.id)}
              >
                {recentWorkspace.name}
              </button>
              <span class="truncate">{recentWorkspace.path}</span>
            </div>
            <div class="tx-dashboard-service-pair">
              <div class="tx-dashboard-service-state {stateClass($mcpRuntimeStates[recentWorkspace.id])}">
                <div class="flex items-center gap-2 min-w-0">
                  <Radio size={13} class="shrink-0" />
                  <span class="font-semibold text-[var(--text-main)]">MCP</span>
                  <strong class="truncate">{stateLabel($mcpRuntimeStates[recentWorkspace.id])}</strong>
                  <span class="font-mono text-[10px] text-[var(--text-muted)] shrink-0">:{recentWorkspace.runtime.local_port}</span>
                  <span class="rounded bg-[var(--surface-hover)] px-1.5 py-0.5 text-[9px] text-[var(--text-secondary)] font-medium shrink-0">
                    {tunnelLabel(recentWorkspace)}
                  </span>
                </div>
                <button
                  type="button"
                  class="tx-dashboard-quick-toggle shrink-0"
                  class:running={$mcpRuntimeStates[recentWorkspace.id] === "running"}
                  disabled={mcpBusyMap[recentWorkspace.id] || $mcpRuntimeStates[recentWorkspace.id] === "starting" || $mcpRuntimeStates[recentWorkspace.id] === "stopping"}
                  onclick={() => void toggleWorkspaceMcp(recentWorkspace!.id)}
                >
                  {#if mcpBusyMap[recentWorkspace.id]}
                    <RotateCw size={10} class="animate-spin shrink-0" />
                  {:else if $mcpRuntimeStates[recentWorkspace.id] === "running"}
                    <Square size={10} class="shrink-0" />
                    <span>停止</span>
                  {:else}
                    <Play size={10} class="shrink-0" />
                    <span>启动</span>
                  {/if}
                </button>
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
            {@const planning = planningByWorkspace[workspace.id]}
            {@const isMcpRunning = $mcpRuntimeStates[workspace.id] === "running"}
            <div class="tx-dashboard-workspace-card">
              <!-- Top identity line with name, path, Finder open and jump buttons -->
              <div class="tx-dashboard-workspace-topline">
                <div class="min-w-0 flex-1">
                  <button
                    type="button"
                    class="text-left font-bold truncate block hover:text-[var(--primary)] transition-colors cursor-pointer"
                    onclick={() => openWorkspace(workspace.id)}
                    title="点击进入工作区"
                  >
                    {workspace.name}
                  </button>
                  <span class="truncate block font-mono text-[10px] text-[var(--text-muted)]" title={workspace.path}>
                    {workspace.path}
                  </span>
                </div>

                <div class="flex items-center gap-1.5 shrink-0">
                  <button
                    type="button"
                    class="tx-dashboard-action-icon"
                    title="在访达/资源管理器中打开"
                    onclick={() => void revealDirectory(workspace.path)}
                  >
                    <FolderOpen size={13} />
                  </button>
                  <button
                    type="button"
                    class="tx-dashboard-action-icon"
                    title="复制完整路径"
                    onclick={() => void copyWorkspacePath(workspace.id, workspace.path)}
                  >
                    {#if copiedPathId === workspace.id}
                      <Check size={13} class="text-[var(--success)]" />
                    {:else}
                      <Copy size={13} />
                    {/if}
                  </button>
                  <button
                    type="button"
                    class="tx-dashboard-action-icon primary"
                    title="进入工作区"
                    onclick={() => openWorkspace(workspace.id)}
                  >
                    <ArrowUpRight size={14} />
                  </button>
                </div>
              </div>

              <!-- Runtime Block with Direct Toggle -->
              <div class="tx-dashboard-runtime-grid">
                <div class="tx-dashboard-runtime-block flex items-center justify-between gap-2">
                  <div class="flex items-center gap-2 min-w-0">
                    <span class="tx-dashboard-dot {stateClass($mcpRuntimeStates[workspace.id])}"></span>
                    <strong class="text-xs font-semibold text-[var(--text-main)]">MCP</strong>
                    <span class="text-[11px] font-medium {isMcpRunning ? 'text-[var(--success)]' : 'text-[var(--text-muted)]'} shrink-0">
                      {stateLabel($mcpRuntimeStates[workspace.id])}
                    </span>
                    <span class="font-mono text-[10px] text-[var(--text-muted)] shrink-0">:{workspace.runtime.local_port}</span>
                    <span class="rounded bg-[var(--surface-hover)] px-1.5 py-0.5 text-[9px] text-[var(--text-secondary)] font-medium shrink-0">
                      {tunnelLabel(workspace)}
                    </span>
                  </div>
                  <button
                    type="button"
                    class="tx-dashboard-quick-toggle shrink-0"
                    class:running={isMcpRunning}
                    disabled={mcpBusyMap[workspace.id] || $mcpRuntimeStates[workspace.id] === "starting" || $mcpRuntimeStates[workspace.id] === "stopping"}
                    onclick={() => void toggleWorkspaceMcp(workspace.id)}
                  >
                    {#if mcpBusyMap[workspace.id]}
                      <RotateCw size={10} class="animate-spin shrink-0" />
                    {:else if isMcpRunning}
                      <Square size={10} class="shrink-0" />
                      <span>停止</span>
                    {:else}
                      <Play size={10} class="shrink-0" />
                      <span>启动</span>
                    {/if}
                  </button>
                </div>
              </div>

              <!-- Footer Planning Line -->
              <div class="tx-dashboard-planning-line">
                <GitBranch size={13} />
                <span class="truncate">{planningLabel(workspace.id)}</span>
                {#if planning}
                  <small>{planning.mode.toUpperCase()}</small>
                {/if}
              </div>
            </div>
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
              <p>统计 MCP 当前配置的公网暴露方式。</p>
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
              <p>由本地 MCP 服务统计 JSON 请求大小后估算，不保存请求正文。</p>
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
