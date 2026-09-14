<script lang="ts">
  import { onMount } from "svelte";
  import { readWorkspaceLogs, type LogChunk, type LogService } from "$lib/api/logs";
  import Button from "$lib/components/ui/Button.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import { RefreshCw } from "@lucide/svelte";

  interface Props {
    workspaceId: string;
    service: LogService;
    autoRefresh?: boolean;
    title?: string;
  }

  let { workspaceId, service, autoRefresh = true, title }: Props = $props();

  let chunks = $state<LogChunk[]>([]);
  let busy = $state(false);
  let error = $state("");

  const heading = $derived(title ?? (service === "mcp" ? "MCP 日志" : "Actions 日志"));

  async function refresh() {
    if (busy || !workspaceId) return;
    busy = true;
    error = "";
    try {
      chunks = await readWorkspaceLogs(workspaceId, service);
    } catch (err) {
      error = String(err);
      chunks = [];
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    if (autoRefresh) {
      void refresh();
    }
  });
</script>

<Card class="p-5">
  <div class="flex items-start justify-between gap-3">
    <div>
      <h3 class="text-sm font-semibold text-[var(--text-main)]">{heading}</h3>
      <p class="mt-0.5 text-xs text-[var(--color-text-muted)]">最近 8KB 尾部输出</p>
    </div>
    <Button
      type="button"
      variant="ghost"
      size="sm"
      disabled={busy}
      busy={busy}
      onclick={refresh}
    >
      <RefreshCw size={12} class={busy ? "animate-spin" : ""} />
      {busy ? "刷新中…" : "刷新"}
    </Button>
  </div>

  {#if error}
    <p
      class="mt-4 rounded-xl border border-[var(--danger)]/20 bg-[var(--danger-soft)] px-3.5 py-2.5 text-xs text-[var(--danger)]"
    >
      {error}
    </p>
  {/if}

  {#if chunks.length > 0}
    <div class="mt-4 grid gap-3">
      {#each chunks as chunk (chunk.name)}
        <div class="overflow-hidden rounded-xl border border-[var(--border)] bg-[var(--card-bg)] shadow-sm">
          <p class="border-b border-[var(--border)] px-3.5 py-2 font-mono text-xs font-medium text-[var(--text-secondary)] bg-[var(--surface-main)]">
            {chunk.name}
          </p>
          <pre
            class="max-h-56 overflow-auto whitespace-pre-wrap break-words p-3.5 font-mono text-[11px] leading-relaxed select-text"
          >{chunk.content || "（空）"}</pre>
        </div>
      {/each}
    </div>
  {:else if !busy && !error}
    <p class="mt-4 text-xs text-[var(--color-text-muted)]">当前还没有日志</p>
  {/if}
</Card>
