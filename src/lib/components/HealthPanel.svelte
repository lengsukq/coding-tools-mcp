<script lang="ts">
  import { runHealthChecks, type HealthItem } from "$lib/api/health";
  import Button from "$lib/components/ui/Button.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import StatusBadge from "$lib/components/ui/StatusBadge.svelte";
  import { Activity } from "@lucide/svelte";

  interface Props {
    workspaceId: string;
    onRunCheck?: (workspaceId: string) => Promise<HealthItem[]>;
  }

  let { workspaceId, onRunCheck }: Props = $props();

  let items = $state<HealthItem[]>([]);
  let busy = $state(false);
  let error = $state("");

  async function runCheck() {
    if (busy || !workspaceId) return;
    busy = true;
    error = "";
    try {
      items = onRunCheck ? await onRunCheck(workspaceId) : await runHealthChecks(workspaceId);
    } catch (err) {
      error = String(err);
      items = [];
    } finally {
      busy = false;
    }
  }
</script>

<Card class="p-5">
  <div class="flex items-start justify-between gap-3">
    <div>
      <h3 class="text-sm font-semibold text-[var(--text-main)]">健康检查</h3>
      <p class="mt-0.5 text-xs text-[var(--color-text-muted)]">
        MCP、Actions 本地/公网 endpoint 与 OAuth 元数据
      </p>
    </div>
    <Button
      type="button"
      variant="ghost"
      size="sm"
      disabled={busy}
      busy={busy}
      onclick={runCheck}
    >
      <Activity size={13} />
      {busy ? "检查中…" : "运行健康检查"}
    </Button>
  </div>

  {#if error}
    <p class="mt-4 rounded-xl border border-[var(--danger)]/20 bg-[var(--danger-soft)] px-3.5 py-2.5 text-xs text-[var(--danger)]">
      {error}
    </p>
  {/if}

  {#if items.length > 0}
    <ul class="mt-4 grid gap-2">
      {#each items as item (item.label)}
        <li
          class="flex items-start justify-between gap-3 rounded-xl border border-[var(--border)] bg-[var(--card-bg)] px-3.5 py-2.5 transition-all shadow-sm"
        >
          <div class="min-w-0 flex-1">
            <p class="text-xs font-semibold text-[var(--text-main)]">{item.label}</p>
            <p class="mt-0.5 text-[11px] text-[var(--color-text-muted)] leading-relaxed">{item.detail}</p>
            {#if !item.ok && item.hint}
              <p class="mt-1 text-xs text-[var(--color-accent)] font-mono">{item.hint}</p>
            {/if}
          </div>
          <StatusBadge
            status={item.ok ? "success" : "error"}
            text={item.ok ? "通过" : "失败"}
            size="sm"
          />
        </li>
      {/each}
    </ul>
  {:else if !busy && !error}
    <p class="mt-4 text-xs text-[var(--color-text-muted)]">尚未运行检查。</p>
  {/if}
</Card>
