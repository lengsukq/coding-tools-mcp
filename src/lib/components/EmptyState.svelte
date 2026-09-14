<script lang="ts">
  import { FolderPlus } from "@lucide/svelte";
  import Button from "$lib/components/ui/Button.svelte";

  interface Props {
    onAdd?: () => void;
  }

  let { onAdd }: Props = $props();
</script>

<div class="tx-empty-state flex flex-col items-center justify-center text-center py-16 px-6">
  <div class="size-16 rounded-2xl flex items-center justify-center bg-[var(--primary-soft)] text-[var(--primary)] border border-[var(--primary)]/20 shadow-sm mb-4">
    <FolderPlus size={30} strokeWidth={1.8} />
  </div>
  <p class="page-kicker">开始使用</p>
  <h2 class="mt-1 text-xl font-bold tracking-tight text-[var(--text-main)]">添加你的第一个工作区</h2>
  <p class="mt-2 max-w-md text-xs leading-relaxed text-[var(--text-muted)]">
    选择项目目录后即可统一管理 MCP、Actions、规划状态与公网隧道配置。也可使用侧栏顶部的「添加工作区」。
  </p>
  {#if onAdd}
    <div class="mt-6">
      <Button variant="primary" size="md" onclick={onAdd}>
        <FolderPlus size={15} strokeWidth={2} />
        <span>选择项目目录</span>
      </Button>
    </div>
  {/if}
</div>
