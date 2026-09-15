<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import {
    Activity, AlertTriangle, ArrowUpRight, Check, ChevronDown, ChevronUp,
    Command, Copy, FolderOpen, Gauge, GitBranch, LayoutDashboard, Pin, PinOff,
    Play, RotateCw, Search, Settings2, SlidersHorizontal, Sparkles, Square, X, Zap,
  } from "@lucide/svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import DashboardUsagePanel from "$lib/components/dashboard/DashboardUsagePanel.svelte";
  import { listHistorySessions, type HistorySessionSummary } from "$lib/api/history";
  import type { PlanningStateDto } from "$lib/api/planning";
  import { getLastWorkspaceId } from "$lib/api/settings";
  import type { ServiceUsageStats } from "$lib/api/usage";
  import {
    buildUsageChart, buildUsagePoint, formatCount, loadPlanningByWorkspace,
    loadUsageByWorkspace, stateClass, stateLabel, summarizeConnections,
    summarizePlanning, summarizeUsage, tunnelLabel, type UsagePoint,
  } from "$lib/dashboard";
  import {
    dashboardPreferences, loadDashboardPreferences, moveWorkspace, sortWorkspaceIds,
    toggleDashboardModule, togglePinnedWorkspace, updateDashboardPreferences,
    type DashboardModuleId,
  } from "$lib/dashboard-preferences";
  import { openWorkspaceDirectory, startRuntime, stopRuntime } from "$lib/api/workspaces";
  import { runServiceToggle } from "$lib/runtime/service";
  import { showToast } from "$lib/stores/toast";
  import { mcpRuntimeStates, workspaces } from "$lib/stores/app";
  import type { WorkspaceProfile } from "$lib/types";

  interface FocusItem {
    workspace: WorkspaceProfile;
    title: string;
    detail: string;
    progress: number;
    progressLabel: string;
    mode: string;
  }

  interface AttentionItem {
    workspace: WorkspaceProfile;
    title: string;
    detail: string;
    level: "warning" | "error" | "info";
  }

  interface ActivityItem {
    workspace: WorkspaceProfile;
    title: string;
    detail: string;
    timestamp: number;
  }

  let lastWorkspaceId = $state("");
  let planningByWorkspace = $state<Record<string, PlanningStateDto | null>>({});
  let usageByWorkspace = $state<Record<string, ServiceUsageStats[]>>({});
  let historyByWorkspace = $state<Record<string, HistorySessionSummary[]>>({});
  let usageHistory = $state<UsagePoint[]>([]);
  let usageWorkspaceKey = $state("");
  let planningGeneration = 0;
  let usageGeneration = 0;
  let historyGeneration = 0;
  let mcpBusyMap = $state<Record<string, boolean>>({});
  let copiedPathId = $state<string | null>(null);
  let commandOpen = $state(false);
  let commandQuery = $state("");
  let preferencesOpen = $state(false);
  let commandInput = $state<HTMLInputElement | null>(null);

  const workspaceCount = $derived($workspaces.length);
  const orderedWorkspaces = $derived.by(() => {
    const ids = sortWorkspaceIds($workspaces.map((workspace) => workspace.id), $dashboardPreferences);
    const byId = new Map($workspaces.map((workspace) => [workspace.id, workspace]));
    return ids.map((id) => byId.get(id)).filter((item): item is WorkspaceProfile => Boolean(item));
  });
  const mcpRunning = $derived($workspaces.filter((workspace) => $mcpRuntimeStates[workspace.id] === "running").length);
  const errorServices = $derived($workspaces.filter((workspace) => $mcpRuntimeStates[workspace.id] === "error").length);
  const totalServices = $derived(workspaceCount);
  const serviceHealth = $derived(totalServices === 0 ? 0 : Math.round((mcpRunning / totalServices) * 100));
  const planningStats = $derived.by(() => summarizePlanning(planningByWorkspace));
  const connectionStats = $derived.by(() => summarizeConnections($workspaces));
  const usageTotals = $derived.by(() => summarizeUsage(usageByWorkspace));
  const averageTokens = $derived(usageTotals.toolCallCount === 0 ? 0 : usageTotals.estimatedToolCallTokens / usageTotals.toolCallCount);
  const usageChart = $derived.by(() => buildUsageChart(usageHistory));

  const focusItems = $derived.by((): FocusItem[] => orderedWorkspaces.flatMap((workspace) => {
    const planning = planningByWorkspace[workspace.id];
    if (!planning) return [];
    const goal = planning.goals.find((item) => item.id === planning.focus_goal_id);
    const plan = planning.plans.find((item) => item.id === planning.focus_plan_id);
    if (!goal && !plan) return [];
    if (plan) {
      const completed = plan.steps.filter((step) => ["completed", "skipped"].includes(step.status)).length;
      const total = plan.steps.length;
      return [{ workspace, title: plan.title, detail: goal?.title ?? plan.objective,
        progress: total === 0 ? 0 : Math.round((completed / total) * 100),
        progressLabel: `${completed} / ${total} Steps`, mode: planning.mode.toUpperCase() }];
    }
    const completed = goal!.success_criteria.filter((criterion) => criterion.completed).length;
    const total = goal!.success_criteria.length;
    return [{ workspace, title: goal!.title, detail: goal!.objective,
      progress: total === 0 ? 0 : Math.round((completed / total) * 100),
      progressLabel: `${completed} / ${total} Criteria`, mode: planning.mode.toUpperCase() }];
  }));

  const primaryFocus = $derived.by(() => focusItems.find((item) => item.workspace.id === lastWorkspaceId) ?? focusItems[0] ?? null);

  const attentionItems = $derived.by((): AttentionItem[] => {
    const items: AttentionItem[] = [];
    for (const workspace of orderedWorkspaces) {
      const runtimeState = $mcpRuntimeStates[workspace.id];
      const planning = planningByWorkspace[workspace.id];
      const usage = usageByWorkspace[workspace.id] ?? [];
      if (runtimeState === "error") items.push({ workspace, title: "MCP Runtime 异常", detail: "进入工作区查看诊断和日志。", level: "error" });
      const reviews = planning ? planning.goals.filter((goal) => goal.status === "awaiting_acceptance").length
        + planning.plans.filter((plan) => plan.status === "awaiting_acceptance").length : 0;
      if (reviews > 0) items.push({ workspace, title: `${reviews} 项 Planning 等待验收`, detail: "完成后需要人工确认才能归档。", level: "warning" });
      if (planning?.execution.last_error) items.push({ workspace, title: "最近执行存在错误", detail: planning.execution.last_error, level: "error" });
      const errors = usage.reduce((sum, item) => sum + item.errorCount, 0);
      if (errors > 0) items.push({ workspace, title: `${formatCount(errors)} 次工具请求错误`, detail: "MCP 使用统计检测到失败请求。", level: "warning" });
    }
    return items.slice(0, 6);
  });

  const usageRanking = $derived.by(() => {
    const rows = orderedWorkspaces.map((workspace) => {
      const stats = usageByWorkspace[workspace.id] ?? [];
      return { workspace, tokens: stats.reduce((sum, item) => sum + item.estimatedTokens, 0) };
    }).sort((a, b) => b.tokens - a.tokens);
    const max = Math.max(rows[0]?.tokens ?? 0, 1);
    return rows.map((row) => ({ ...row, percentage: Math.round((row.tokens / max) * 100) }));
  });

  const recentActivities = $derived.by((): ActivityItem[] => {
    const items: ActivityItem[] = [];
    for (const workspace of orderedWorkspaces) {
      for (const session of historyByWorkspace[workspace.id] ?? []) {
        items.push({ workspace, title: session.title || workspace.name,
          detail: session.latest_focus || session.snippets.at(-1)?.text || "History session updated",
          timestamp: parseTimestamp(session.updated_at ?? session.created_at) });
      }
    }
    return items.sort((a, b) => b.timestamp - a.timestamp).slice(0, 8);
  });

  const commandEntries = $derived.by(() => {
    const query = commandQuery.trim().toLowerCase();
    const entries = [
      { label: "添加工作区", hint: "Workspace", run: () => window.dispatchEvent(new CustomEvent("coding-tools:add-workspace")) },
      { label: "打开通用设置", hint: "Settings", run: () => goto("/settings/general") },
      { label: "启动全部 MCP", hint: "Runtime", run: () => void setAllRuntime(true) },
      { label: "停止全部 MCP", hint: "Runtime", run: () => void setAllRuntime(false) },
      ...orderedWorkspaces.map((workspace) => ({ label: `打开 ${workspace.name}`, hint: "Workspace", run: () => openWorkspace(workspace.id) })),
    ];
    return query ? entries.filter((entry) => `${entry.label} ${entry.hint}`.toLowerCase().includes(query)) : entries;
  });

  function moduleVisible(moduleId: DashboardModuleId) { return !$dashboardPreferences.hiddenModules.includes(moduleId); }
  function parseTimestamp(value?: string | null): number {
    if (!value) return 0;
    const numeric = Number(value);
    if (Number.isFinite(numeric)) return numeric < 1_000_000_000_000 ? numeric * 1000 : numeric;
    const parsed = Date.parse(value);
    return Number.isNaN(parsed) ? 0 : parsed;
  }
  function formatRelativeTime(timestamp: number): string {
    if (!timestamp) return "—";
    const minutes = Math.floor(Math.max(0, Date.now() - timestamp) / 60_000);
    if (minutes < 1) return "刚刚";
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.floor(minutes / 60);
    return hours < 24 ? `${hours}h` : `${Math.floor(hours / 24)}d`;
  }
  function workspaceUsageTokens(workspaceId: string) { return (usageByWorkspace[workspaceId] ?? []).reduce((sum, item) => sum + item.estimatedTokens, 0); }
  function planningSummary(workspaceId: string): string {
    const planning = planningByWorkspace[workspaceId];
    if (!planning) return "—";
    const goal = planning.goals.find((item) => item.id === planning.focus_goal_id);
    const plan = planning.plans.find((item) => item.id === planning.focus_plan_id);
    return goal?.title ?? plan?.title ?? planning.mode.toUpperCase();
  }

  async function loadPlanning(items: WorkspaceProfile[]) {
    const generation = ++planningGeneration;
    if (items.length === 0) { planningByWorkspace = {}; return; }
    const next = await loadPlanningByWorkspace(items);
    if (generation === planningGeneration) planningByWorkspace = next;
  }
  async function loadUsage(items: WorkspaceProfile[]) {
    const generation = ++usageGeneration;
    if (items.length === 0) { usageByWorkspace = {}; usageHistory = []; usageWorkspaceKey = ""; return; }
    const workspaceKey = items.map((item) => item.id).sort().join("|");
    if (workspaceKey !== usageWorkspaceKey) { usageWorkspaceKey = workspaceKey; usageHistory = []; }
    const next = await loadUsageByWorkspace(items);
    if (generation !== usageGeneration) return;
    usageByWorkspace = next;
    usageHistory = [...usageHistory, buildUsagePoint(next, usageHistory.at(-1))].slice(-24);
  }
  async function loadHistory(items: WorkspaceProfile[]) {
    const generation = ++historyGeneration;
    const entries = await Promise.all(items.map(async (workspace) => {
      try { const catalog = await listHistorySessions(workspace.id); return [workspace.id, catalog.sessions.slice(0, 5)] as const; }
      catch { return [workspace.id, []] as const; }
    }));
    if (generation === historyGeneration) historyByWorkspace = Object.fromEntries(entries);
  }
  function openWorkspace(id: string) { commandOpen = false; goto(`/workspace/${id}`); }
  async function toggleWorkspaceMcp(id: string) {
    if (mcpBusyMap[id]) return;
    const wasRunning = $mcpRuntimeStates[id] === "running";
    mcpBusyMap = { ...mcpBusyMap, [id]: true };
    try {
      const status = await runServiceToggle(wasRunning, () => startRuntime(id), () => stopRuntime(id), "MCP");
      if (status) mcpRuntimeStates.update((map) => ({ ...map, [id]: status.state }));
    } finally { mcpBusyMap = { ...mcpBusyMap, [id]: false }; }
  }
  async function setAllRuntime(start: boolean) {
    commandOpen = false;
    for (const workspace of orderedWorkspaces) {
      const running = $mcpRuntimeStates[workspace.id] === "running";
      if (start !== running) await toggleWorkspaceMcp(workspace.id);
    }
  }
  async function copyWorkspacePath(id: string, path: string) {
    try {
      await navigator.clipboard.writeText(path); copiedPathId = id;
      setTimeout(() => { if (copiedPathId === id) copiedPathId = null; }, 1500);
      showToast("工作区路径已复制", { kind: "success", duration: 1800 });
    } catch { showToast("复制路径失败", { kind: "error" }); }
  }
  async function revealDirectory(path: string) {
    try { await openWorkspaceDirectory(path); }
    catch (error) { showToast(`打开目录失败: ${error instanceof Error ? error.message : String(error)}`, { kind: "error" }); }
  }
  function runCommand(run: () => void) { commandOpen = false; commandQuery = ""; run(); }

  onMount(() => {
    loadDashboardPreferences();
    const usageTimer = window.setInterval(() => void loadUsage($workspaces), 5000);
    const historyTimer = window.setInterval(() => void loadHistory($workspaces), 30_000);
    void getLastWorkspaceId().then((id) => { lastWorkspaceId = id ?? ""; }).catch(() => { lastWorkspaceId = ""; });
    const handleKeydown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault(); commandOpen = !commandOpen; commandQuery = "";
      } else if (event.key === "Escape") { commandOpen = false; preferencesOpen = false; }
    };
    window.addEventListener("keydown", handleKeydown);
    return () => { window.clearInterval(usageTimer); window.clearInterval(historyTimer); window.removeEventListener("keydown", handleKeydown); };
  });
  $effect(() => { const items = $workspaces; void loadPlanning(items); void loadUsage(items); void loadHistory(items); });
  $effect(() => {
    if (!commandOpen) return;
    requestAnimationFrame(() => commandInput?.focus());
  });
</script>

<section class="page-scroll wb-dashboard" data-density={$dashboardPreferences.density}>
  <header class="page-header wb-dashboard-header">
    <div>
      <div class="wb-dashboard-title-row">
        <LayoutDashboard size={18} class="text-[var(--primary)]" />
        <h1 class="wb-dashboard-title">工作台</h1>
      </div>
      <p class="wb-dashboard-subtitle">继续上一次 Coding 工作、处理需要关注的状态，并快速控制所有 Workspace Runtime。</p>
    </div>
    <div class="wb-header-actions">
      <button class="wb-command-button" type="button" onclick={() => { commandOpen = true; commandQuery = ""; }}>
        <Command size={14} />
        <span>Quick Actions</span>
        <span class="wb-kbd">⌘K</span>
      </button>
      <div class="wb-preferences-wrap">
        <button class="wb-icon-button" type="button" title="工作台偏好" onclick={() => preferencesOpen = !preferencesOpen}>
          <SlidersHorizontal size={14} />
        </button>
        {#if preferencesOpen}
          <div class="wb-preferences-popover">
            <p class="wb-pref-title">工作台偏好</p>
            <div class="wb-pref-row">
              <span>信息密度</span>
              <button type="button" onclick={() => updateDashboardPreferences((current) => ({ ...current, density: current.density === "compact" ? "comfortable" : "compact" }))}>
                {$dashboardPreferences.density === "compact" ? "紧凑" : "舒适"}
              </button>
            </div>
            {#each [
              ["focus", "当前 Focus"], ["attention", "需要关注"], ["workspaces", "工作区"],
              ["activity", "最近活动"], ["usage", "Token Analytics"], ["health", "系统健康"],
            ] as item}
              <div class="wb-pref-row">
                <span>{item[1]}</span>
                <button type="button" onclick={() => toggleDashboardModule(item[0] as DashboardModuleId)}>
                  {moduleVisible(item[0] as DashboardModuleId) ? "隐藏" : "显示"}
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </header>

  <div class="page-body wb-dashboard-main">
    {#if workspaceCount === 0}
      <EmptyState />
    {:else}
      {#if moduleVisible("focus")}
        <section class="wb-hero">
          <div class="wb-focus">
            <div class="wb-eyebrow"><Sparkles size={13} /> Continue Working</div>
            {#if primaryFocus}
              <h2>{primaryFocus.title}</h2>
              <p>{primaryFocus.detail}</p>
              <div class="wb-progress" aria-label={primaryFocus.progressLabel}><span style={`width:${primaryFocus.progress}%`}></span></div>
              <div class="wb-focus-meta">
                <span>{primaryFocus.workspace.name}</span>
                <span>{primaryFocus.mode}</span>
                <span>{primaryFocus.progressLabel}</span>
                <span>{tunnelLabel(primaryFocus.workspace)}</span>
              </div>
              <div class="wb-focus-actions">
                <button class="wb-primary-button" type="button" onclick={() => openWorkspace(primaryFocus!.workspace.id)}>
                  继续工作 <ArrowUpRight size={13} />
                </button>
                <button class="wb-soft-button" type="button" onclick={() => void toggleWorkspaceMcp(primaryFocus!.workspace.id)}>
                  {#if mcpBusyMap[primaryFocus.workspace.id]}
                    <RotateCw size={12} class="animate-spin" />
                  {:else if $mcpRuntimeStates[primaryFocus.workspace.id] === "running"}
                    <Square size={11} /> 停止 MCP
                  {:else}
                    <Play size={11} /> 启动 MCP
                  {/if}
                </button>
              </div>
            {:else}
              <h2>选择一个工作区开始</h2>
              <p>当前没有聚焦的 Goal 或 Plan。你仍然可以从下面的工作区继续工作。</p>
            {/if}
          </div>
          <div class="wb-health-orb">
            <div class="wb-health-ring" style={`--health-angle:${serviceHealth * 3.6}deg`}>
              <div class="wb-health-ring-content">
                <strong>{serviceHealth}%</strong>
                <span>Runtime</span>
                <small>Online</small>
              </div>
            </div>
          </div>
        </section>
      {/if}

      <div class="wb-stat-strip">
        <div class="wb-stat"><span>Workspaces</span><strong>{workspaceCount}</strong><small>{mcpRunning} 个在线</small></div>
        <div class="wb-stat"><span>MCP Tokens</span><strong>{formatCount(usageTotals.estimatedTokens)}</strong><small>{formatCount(usageTotals.toolCallCount)} 次工具调用</small></div>
        <div class="wb-stat"><span>Active Goals</span><strong>{planningStats.activeGoals}</strong><small>{planningStats.activePlans} 个 Plan</small></div>
        <div class="wb-stat"><span>Need Review</span><strong>{planningStats.pendingReview}</strong><small>{errorServices > 0 ? `${errorServices} 个 Runtime 异常` : "无 Runtime 异常"}</small></div>
      </div>

      {#if moduleVisible("attention") && attentionItems.length > 0}
        <section class="wb-section">
          <div class="wb-section-heading">
            <div><h3>需要关注</h3><p>只显示真正需要你处理的异常、错误和人工验收。</p></div>
            <AlertTriangle size={15} class="text-[var(--warning)]" />
          </div>
          <div class="wb-attention-list">
            {#each attentionItems as item}
              <button class="wb-attention-row text-left border-0 bg-transparent cursor-pointer" type="button" onclick={() => openWorkspace(item.workspace.id)}>
                {#if item.level === "error"}<X size={14} class="text-[var(--danger)]" />{:else}<AlertTriangle size={14} class="text-[var(--warning)]" />{/if}
                <div class="min-w-0"><strong>{item.workspace.name} · {item.title}</strong><span class="truncate">{item.detail}</span></div>
                <ArrowUpRight size={12} class="text-[var(--text-muted)]" />
              </button>
            {/each}
          </div>
        </section>
      {/if}

      {#if moduleVisible("workspaces")}
        <section class="wb-section">
          <div class="wb-section-heading">
            <div><h3>工作区</h3><p>运行状态、当前 Planning 和 MCP 用量放在同一行；Pin 与排序会同步到 Sidebar。</p></div>
            <span class="text-[10px] text-[var(--text-muted)]">{workspaceCount} Workspaces</span>
          </div>
          <div class="wb-workspace-list">
            <div class="wb-workspace-header"><span>Workspace</span><span>Runtime</span><span>Planning</span><span>Tokens</span><span></span></div>
            {#each orderedWorkspaces as workspace (workspace.id)}
              <div class="wb-workspace-row" class:is-pinned={$dashboardPreferences.pinnedWorkspaceIds.includes(workspace.id)}>
                <div class="wb-workspace-name">
                  <button type="button" class="truncate" onclick={() => openWorkspace(workspace.id)}>{workspace.name}</button>
                  <small title={workspace.path}>{workspace.path}</small>
                </div>
                <div class="wb-runtime-pill">
                  <span class="wb-runtime-dot {stateClass($mcpRuntimeStates[workspace.id])}"></span>
                  <span>{stateLabel($mcpRuntimeStates[workspace.id])}</span>
                  <span class="font-mono text-[9px]">:{workspace.runtime.local_port}</span>
                </div>
                <div class="wb-mode-pill min-w-0"><GitBranch size={11} /><span class="truncate" title={planningSummary(workspace.id)}>{planningSummary(workspace.id)}</span></div>
                <div class="wb-workspace-token">{formatCount(workspaceUsageTokens(workspace.id))}</div>
                <div class="wb-workspace-actions">
                  <button class="wb-icon-button !w-7 !min-h-7" type="button" title={$dashboardPreferences.pinnedWorkspaceIds.includes(workspace.id) ? "取消置顶" : "置顶"} onclick={() => togglePinnedWorkspace(workspace.id)}>
                    {#if $dashboardPreferences.pinnedWorkspaceIds.includes(workspace.id)}<PinOff size={11} />{:else}<Pin size={11} />{/if}
                  </button>
                  <button class="wb-icon-button !w-7 !min-h-7" type="button" title="上移" onclick={() => moveWorkspace(workspace.id, -1, orderedWorkspaces.map((item) => item.id))}><ChevronUp size={11} /></button>
                  <button class="wb-icon-button !w-7 !min-h-7" type="button" title="下移" onclick={() => moveWorkspace(workspace.id, 1, orderedWorkspaces.map((item) => item.id))}><ChevronDown size={11} /></button>
                  <button class="wb-icon-button !w-7 !min-h-7" type="button" title="打开目录" onclick={() => void revealDirectory(workspace.path)}><FolderOpen size={11} /></button>
                  <button class="wb-icon-button !w-7 !min-h-7" type="button" title="复制路径" onclick={() => void copyWorkspacePath(workspace.id, workspace.path)}>
                    {#if copiedPathId === workspace.id}<Check size={11} class="text-[var(--success)]" />{:else}<Copy size={11} />{/if}
                  </button>
                  <button class="wb-icon-button !w-7 !min-h-7" type="button" title={$mcpRuntimeStates[workspace.id] === "running" ? "停止 MCP" : "启动 MCP"} onclick={() => void toggleWorkspaceMcp(workspace.id)}>
                    {#if mcpBusyMap[workspace.id]}<RotateCw size={11} class="animate-spin" />{:else if $mcpRuntimeStates[workspace.id] === "running"}<Square size={10} />{:else}<Play size={10} />{/if}
                  </button>
                  <button class="wb-icon-button !w-7 !min-h-7" type="button" title="进入工作区" onclick={() => openWorkspace(workspace.id)}><ArrowUpRight size={11} /></button>
                </div>
              </div>
            {/each}
          </div>
        </section>
      {/if}

      <div class="wb-grid-two">
        {#if moduleVisible("activity")}
          <section class="wb-section">
            <div class="wb-section-heading">
              <div><h3>最近活动</h3><p>从各工作区 History Session 汇总最近的 Coding 上下文。</p></div>
              <Activity size={14} />
            </div>
            {#if recentActivities.length > 0}
              <div class="wb-activity-list">
                {#each recentActivities as item}
                  <button class="wb-activity-row text-left border-0 bg-transparent cursor-pointer" type="button" onclick={() => openWorkspace(item.workspace.id)}>
                    <span class="wb-activity-time">{formatRelativeTime(item.timestamp)}</span>
                    <div class="min-w-0"><strong>{item.workspace.name} · {item.title}</strong><p>{item.detail}</p></div>
                    <ArrowUpRight size={11} class="text-[var(--text-muted)]" />
                  </button>
                {/each}
              </div>
            {:else}
              <div class="wb-empty-inline">还没有可汇总的 History Session。</div>
            {/if}
          </section>
        {/if}

        {#if moduleVisible("health")}
          <section class="wb-section">
            <div class="wb-section-heading">
              <div><h3>系统健康</h3><p>把 Runtime、连接和 Planning 状态压缩成可扫描信息。</p></div>
              <Gauge size={14} />
            </div>
            <div class="wb-health-list">
              <div class="wb-health-row"><span>Runtime</span><strong>{mcpRunning}/{workspaceCount} Running</strong></div>
              <div class="wb-health-row"><span>Planning</span><strong>{planningStats.pendingReview > 0 ? `${planningStats.pendingReview} Waiting Review` : "Healthy"}</strong></div>
              <div class="wb-health-row"><span>Tool Errors</span><strong>{usageTotals.errorCount > 0 ? formatCount(usageTotals.errorCount) : "0"}</strong></div>
              <div class="wb-health-row"><span>Global Gateway</span><strong>{connectionStats.gateway}</strong></div>
              <div class="wb-health-row"><span>FRP / Cloudflare / Local</span><strong>{connectionStats.frp} / {connectionStats.cloudflare} / {connectionStats.local}</strong></div>
              <div class="wb-health-row"><span>Quality Signal</span><strong>{errorServices === 0 && usageTotals.errorCount === 0 ? "Passed" : "Needs Attention"}</strong></div>
            </div>
          </section>
        {/if}
      </div>

      {#if moduleVisible("usage")}
        <section class="wb-section">
          <div class="wb-section-heading">
            <div><h3>Token Analytics</h3><p>MCP JSON 传输量估算；用于观察工具调用趋势，不等同于模型账单 Token。</p></div>
            <Zap size={14} />
          </div>
          <div class="wb-grid-two">
            <DashboardUsagePanel totals={usageTotals} {averageTokens} chart={usageChart} />
            <div class="wb-ranking">
              {#each usageRanking as row}
                <div class="wb-rank-row">
                  <span title={row.workspace.name}>{row.workspace.name}</span>
                  <div class="wb-rank-track"><i style={`width:${row.percentage}%`}></i></div>
                  <strong>{formatCount(row.tokens)}</strong>
                </div>
              {/each}
              <div class="wb-health-row"><span>Input / Output</span><strong>{formatCount(usageTotals.estimatedInputTokens)} / {formatCount(usageTotals.estimatedOutputTokens)}</strong></div>
              <div class="wb-health-row"><span>Avg / Tool Call</span><strong>{formatCount(averageTokens)}</strong></div>
              <div class="wb-health-row"><span>Requests / Calls</span><strong>{formatCount(usageTotals.requestCount)} / {formatCount(usageTotals.toolCallCount)}</strong></div>
            </div>
          </div>
        </section>
      {/if}
    {/if}
  </div>
</section>

{#if commandOpen}
  <div class="wb-command-backdrop" role="presentation" onclick={(event) => { if (event.currentTarget === event.target) commandOpen = false; }}>
    <div class="wb-command-palette" role="dialog" aria-modal="true" aria-label="Quick Actions">
      <div class="wb-command-search">
        <Search size={15} class="text-[var(--text-muted)]" />
        <input
          bind:this={commandInput}
          bind:value={commandQuery}
          placeholder="搜索工作区或操作…"
          onkeydown={(event) => {
            if (event.key === "Enter" && commandEntries[0]) runCommand(commandEntries[0].run);
          }}
        />
        <span class="wb-kbd">ESC</span>
      </div>
      <div class="wb-command-results">
        {#if commandEntries.length === 0}
          <div class="wb-empty-inline">没有匹配的操作。</div>
        {:else}
          {#each commandEntries as entry, index}
            <button class="wb-command-item" class:active={index === 0} type="button" onclick={() => runCommand(entry.run)}>
              {#if entry.hint === "Settings"}
                <Settings2 size={14} />
              {:else if entry.hint === "Runtime"}
                <Zap size={14} />
              {:else}
                <GitBranch size={14} />
              {/if}
              <span>{entry.label}</span><small>{entry.hint}</small>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}
