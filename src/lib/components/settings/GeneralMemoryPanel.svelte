<script lang="ts">
  import { RefreshCw } from "@lucide/svelte";
  import Button from "$lib/components/ui/Button.svelte";

  interface Props {
    memoryHint: string | null;
    releasing: boolean;
    onRefresh: () => void | Promise<void>;
    onRelease: () => void | Promise<void>;
  }

  let { memoryHint, releasing, onRefresh, onRelease }: Props = $props();
</script>

<div class="tx-card p-5">
  <h3 class="text-sm font-semibold tracking-tight text-[var(--text-main)]">界面内存</h3>
  <p class="mt-1 text-xs text-[var(--color-text-muted)]">
    长时间运行后 WebView 可能占用较高内存。释放会重建界面进程，不会停止 MCP 或隧道。
  </p>
  {#if memoryHint}
    <p class="mt-2 text-xs text-[var(--color-text-muted)]">{memoryHint}</p>
  {/if}
  <div class="mt-4 flex flex-wrap gap-2">
    <Button variant="ghost" size="md" onclick={onRefresh}>刷新占用</Button>
    <Button variant="primary" size="md" disabled={releasing} busy={releasing} onclick={onRelease}>
      <RefreshCw size={14} strokeWidth={2} class={releasing ? "animate-spin" : ""} />
      <span>{releasing ? "刷新中…" : "释放界面内存"}</span>
    </Button>
  </div>
</div>
