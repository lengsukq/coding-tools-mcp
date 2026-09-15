<script lang="ts">
  import type { RuntimeState, WorkspaceProfile } from "$lib/types";

  interface Props {
    workspace: WorkspaceProfile;
    active: boolean;
    mcpState: RuntimeState;
    onClick: () => void;
  }

  let { workspace, active, mcpState, onClick }: Props = $props();

  const directoryName = $derived(workspace.path.split(/[\\/]/).filter(Boolean).pop() || workspace.path);
  const stateText = $derived(
    mcpState === "running"
      ? "MCP 运行中"
      : mcpState === "error"
        ? "MCP 异常"
        : mcpState === "starting"
          ? "MCP 启动中"
          : mcpState === "stopping"
            ? "MCP 停止中"
            : "MCP 已停止",
  );
</script>

<div class="tx-nav-item" class:active>
  <button
    type="button"
    class="tx-nav-button wb-sidebar-workspace-row select-none group"
    onclick={onClick}
    title={`${workspace.name}\n${workspace.path}`}
  >
    <div class="min-w-0 flex-1 text-left">
      <div class="wb-sidebar-workspace-name truncate">{workspace.name}</div>
      <div class="wb-sidebar-workspace-path truncate">{directoryName}</div>
    </div>
    <span class="wb-sidebar-runtime-dot {mcpState}" title={stateText} aria-label={stateText}></span>
  </button>
</div>
