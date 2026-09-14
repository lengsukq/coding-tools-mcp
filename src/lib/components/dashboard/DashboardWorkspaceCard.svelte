<script lang="ts">
  import {
    ArrowUpRight,
    Check,
    Copy,
    FolderOpen,
    GitBranch,
    Play,
    RotateCw,
    Square,
  } from "@lucide/svelte";
  import type { PlanningStateDto } from "$lib/api/planning";
  import { stateClass, stateLabel, tunnelLabel } from "$lib/dashboard";
  import type { RuntimeState, WorkspaceProfile } from "$lib/types";

  interface Props {
    workspace: WorkspaceProfile;
    planning?: PlanningStateDto | null;
    planningLabel: string;
    runtimeState?: RuntimeState;
    busy?: boolean;
    copied?: boolean;
    onOpen: (id: string) => void;
    onReveal: (path: string) => void | Promise<void>;
    onCopy: (id: string, path: string) => void | Promise<void>;
    onToggle: (id: string) => void | Promise<void>;
  }

  let {
    workspace,
    planning = null,
    planningLabel,
    runtimeState,
    busy = false,
    copied = false,
    onOpen,
    onReveal,
    onCopy,
    onToggle,
  }: Props = $props();

  const running = $derived(runtimeState === "running");
  const transitioning = $derived(runtimeState === "starting" || runtimeState === "stopping");
</script>

<div class="tx-dashboard-workspace-card">
  <div class="tx-dashboard-workspace-topline">
    <div class="min-w-0 flex-1">
      <button
        type="button"
        class="text-left font-bold truncate block hover:text-[var(--primary)] transition-colors cursor-pointer"
        onclick={() => onOpen(workspace.id)}
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
        onclick={() => void onReveal(workspace.path)}
      >
        <FolderOpen size={13} />
      </button>
      <button
        type="button"
        class="tx-dashboard-action-icon"
        title="复制完整路径"
        onclick={() => void onCopy(workspace.id, workspace.path)}
      >
        {#if copied}
          <Check size={13} class="text-[var(--success)]" />
        {:else}
          <Copy size={13} />
        {/if}
      </button>
      <button
        type="button"
        class="tx-dashboard-action-icon primary"
        title="进入工作区"
        onclick={() => onOpen(workspace.id)}
      >
        <ArrowUpRight size={14} />
      </button>
    </div>
  </div>

  <div class="tx-dashboard-runtime-grid">
    <div class="tx-dashboard-runtime-block flex items-center justify-between gap-2">
      <div class="flex items-center gap-2 min-w-0">
        <span class="tx-dashboard-dot {stateClass(runtimeState)}"></span>
        <strong class="text-xs font-semibold text-[var(--text-main)]">MCP</strong>
        <span class="text-[11px] font-medium {running ? 'text-[var(--success)]' : 'text-[var(--text-muted)]'} shrink-0">
          {stateLabel(runtimeState)}
        </span>
        <span class="font-mono text-[10px] text-[var(--text-muted)] shrink-0">:{workspace.runtime.local_port}</span>
        <span class="rounded bg-[var(--surface-hover)] px-1.5 py-0.5 text-[9px] text-[var(--text-secondary)] font-medium shrink-0">
          {tunnelLabel(workspace)}
        </span>
      </div>
      <button
        type="button"
        class="tx-dashboard-quick-toggle shrink-0"
        class:running
        disabled={busy || transitioning}
        onclick={() => void onToggle(workspace.id)}
      >
        {#if busy}
          <RotateCw size={10} class="animate-spin shrink-0" />
        {:else if running}
          <Square size={10} class="shrink-0" />
          <span>停止</span>
        {:else}
          <Play size={10} class="shrink-0" />
          <span>启动</span>
        {/if}
      </button>
    </div>
  </div>

  <div class="tx-dashboard-planning-line">
    <GitBranch size={13} />
    <span class="truncate">{planningLabel}</span>
    {#if planning}
      <small>{planning.mode.toUpperCase()}</small>
    {/if}
  </div>
</div>
