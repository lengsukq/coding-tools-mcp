<script lang="ts">
  import StatusOrb from "$lib/components/StatusOrb.svelte";
  import type { RuntimeState, WorkspaceProfile } from "$lib/types";

  interface Props {
    workspace: WorkspaceProfile;
    active: boolean;
    mcpState: RuntimeState;
    onClick: () => void;
  }

  let { workspace, active, mcpState, onClick }: Props = $props();

  const isRunning = $derived(mcpState === "running");
  const isError = $derived(mcpState === "error");
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
        <StatusOrb state={mcpState} />
      </div>
      <div class="min-w-0 flex-1 text-left">
        <div class="truncate text-xs font-semibold tracking-tight">{workspace.name}</div>
        <div class="tx-nav-meta truncate text-[11px] leading-tight mt-0.5">
          {#if isError}
            <span class="text-[var(--danger)]">服务异常</span>
          {:else if isRunning}
            <span class="text-[var(--success)]">
              MCP 运行
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
