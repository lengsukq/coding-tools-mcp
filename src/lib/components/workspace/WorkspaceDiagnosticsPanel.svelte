<script lang="ts">
  import HealthPanel from "$lib/components/HealthPanel.svelte";
  import LogViewer from "$lib/components/LogViewer.svelte";

  let { workspaceId }: { workspaceId: string } = $props();
</script>

<div class="grid gap-6">
  <div class="flex items-center justify-between gap-4">
    <div>
      <h3 class="text-sm font-semibold text-[var(--text-main)]">实时诊断与健康中心</h3>
      <p class="text-xs text-[var(--color-text-muted)] mt-0.5">本地 endpoint、公网穿透、OAuth 签名连通性与服务输出日志</p>
    </div>
  </div>
  <HealthPanel {workspaceId} />
  <div class="grid gap-3">
    <div class="flex items-center justify-between gap-3">
      <span class="text-xs font-semibold text-[var(--text-secondary)] uppercase tracking-wider">实时服务日志</span>
    </div>
    <LogViewer {workspaceId} service="mcp" />
  </div>
</div>
