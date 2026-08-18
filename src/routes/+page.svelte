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
  import { actionsRuntimeStates, mcpRuntimeStates, workspaces } from "$lib/stores/app";
  import { actionsConfig, type RuntimeState, type WorkspaceProfile } from "$lib/types";

  let lastWorkspaceId = $state("");
  let planningByWorkspace = $state<Record<string, PlanningStateDto | null>>({});
  let planningGeneration = 0;

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

  function openWorkspace(id: string) {
    goto(`/workspace/${id}`);
  }

  onMount(() => {
    void getLastWorkspaceId()
      .then((id) => {
        lastWorkspaceId = id ?? "";
      })
      .catch(() => {
        lastWorkspaceId = "";
      });
  });

  $effect(() => {
    const items = $workspaces;
    void loadPlanning(items);
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
        不进入具体工作区，也可以查看全部服务运行状态、连接方式和 AI Planning 进度。
      </p>
    </div>
  </header>

  <div class="page-body tx-dashboard-body">
    {#if workspaceCount === 0}
      <div class="tx-dashboard-empty">
        <EmptyState />
      </div>
    {:else}
      <div class="tx-dashboard-hero-grid">
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

      <div class="tx-dashboard-metrics">
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
      </div>

      <section class="tx-dashboard-section">
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

      <div class="tx-dashboard-detail-grid">
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
      </div>
    {/if}
  </div>
</section>
