<script lang="ts">
  import { FolderPlus } from "@lucide/svelte";

  interface Props {
    onAdd?: () => void;
  }

  let { onAdd }: Props = $props();
</script>

<div class="tx-empty-state">
  <div class="tx-empty-state-icon">
    <FolderPlus size={28} class="text-[var(--color-text-muted)]" />
  </div>
  <p class="page-kicker">开始使用</p>
  <h2 class="mt-1 text-2xl font-bold tracking-tight">添加你的第一个工作区</h2>
  <p class="mt-2 max-w-md text-sm leading-6 text-[var(--color-text-muted)]">
    选择项目目录后即可统一管理 MCP、Actions、规划状态与隧道配置。也可以直接使用左侧顶部的「添加工作区」。
  </p>
  {#if onAdd}
    <button type="button" class="tx-btn-primary mt-6" onclick={onAdd}>
      选择目录
    </button>
  {/if}
</div>
