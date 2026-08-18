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
</script>

<div class="tx-nav-item" class:active>
  <button type="button" class="tx-nav-button" onclick={onClick}>
    <div class="flex min-w-0 flex-1 items-center gap-2">
      <ServiceStatusPair mcp={mcpState} actions={actionsState} />
      <div class="min-w-0 flex-1 text-left">
        <div class="truncate text-sm font-medium">{workspace.name}</div>
        <div class="tx-nav-meta truncate">
          {mcpState === "running" ? "MCP运行" : "MCP停止"}
          ·
          {actionsState === "running" ? "Actions运行" : "Actions停止"}
        </div>
      </div>
    </div>
  </button>
</div>
