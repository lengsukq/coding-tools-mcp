<script lang="ts">
  import ServiceStatusPair from "$lib/components/ServiceStatusPair.svelte";
  import type { RuntimeState, WorkspaceProfile } from "$lib/types";

  interface Props {
    workspace: WorkspaceProfile;
    active: boolean;
    mcpState: RuntimeState;
    actionsState: RuntimeState;
    onClick: () => void;
  }

  let { workspace, active, mcpState, actionsState, onClick }: Props = $props();

  const isAnyRunning = $derived(mcpState === "running" || actionsState === "running");
  const isAnyError = $derived(mcpState === "error" || actionsState === "error");
</script>

<div class="tx-nav-item" class:active>
  <button
    type="button"
    class="tx-nav-button select-none group"
    onclick={onClick}
    title={`${workspace.name}\n${workspace.path}`}
  >
    <div class="flex min-w-0 flex-1 items-center gap-2.5">
      <div class="shrink-0 flex items-center">
        <ServiceStatusPair mcp={mcpState} actions={actionsState} />
      </div>
      <div class="min-w-0 flex-1 text-left">
        <div class="truncate text-xs font-semibold tracking-tight">{workspace.name}</div>
        <div class="tx-nav-meta truncate text-[11px] leading-tight mt-0.5">
          {#if isAnyError}
            <span class="text-[var(--danger)]">服务异常</span>
          {:else if isAnyRunning}
            <span class="text-[var(--success)]">
              {mcpState === "running" && actionsState === "running" ? "双服务运行" : mcpState === "running" ? "MCP 运行" : "Actions 运行"}
            </span>
          {:else}
            <span>已就绪</span>
          {/if}
          <span class="text-[var(--sidebar-text-muted)] opacity-60"> · </span>
          <span class="text-[var(--sidebar-text-muted)] truncate">{workspace.path.split(/[\\/]/).filter(Boolean).pop() || ""}</span>
        </div>
      </div>
    </div>
  </button>
</div>
