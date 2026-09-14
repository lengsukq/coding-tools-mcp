<script lang="ts">
  import CopyButton from "$lib/components/CopyButton.svelte";
  import StatusOrb from "$lib/components/StatusOrb.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import type { RuntimeState } from "$lib/types";

  interface Props {
    title: string;
    subtitle: string;
    status: RuntimeState;
    statusMessage?: string;
    port: number;
    portEditable?: boolean;
    busy?: boolean;
    tunnelType?: string;
    localEndpoint: string;
    publicEndpoint?: string;
    publicLabel?: string;
    showToggle?: boolean;
    onToggle: () => void | Promise<void>;
    onPortChange?: (port: number) => void | Promise<void>;
  }

  let {
    title,
    subtitle,
    status,
    statusMessage = "",
    port,
    portEditable = false,
    busy = false,
    tunnelType = "none",
    localEndpoint,
    publicEndpoint = "",
    publicLabel = "公网",
    showToggle = true,
    onToggle,
    onPortChange,
  }: Props = $props();

  let draftPort = $state(0);

  $effect(() => {
    draftPort = port;
  });

  const running = $derived(status === "running");
  const showError = $derived(status === "error" && Boolean(statusMessage));
  const canEditPort = $derived(portEditable && !running && status !== "starting");
  const tunnelEnabled = $derived(tunnelType === "cloudflare" || tunnelType === "frp");
  const tunnelLabel = $derived(
    tunnelType === "cloudflare" ? "Cloudflare" : tunnelType === "frp" ? "FRP" : "",
  );

  async function commitPort() {
    if (!onPortChange || draftPort === port) return;
    if (draftPort < 1024 || draftPort > 65535) {
      draftPort = port;
      return;
    }
    await onPortChange(draftPort);
  }
</script>

<article class="tx-card p-5">
  <div class="tx-service-panel-header">
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <StatusOrb state={status} />
        <h3 class="text-sm font-semibold tracking-tight text-[var(--text-main)]">{title}</h3>
      </div>
      <p class="mt-1 text-xs text-[var(--text-muted)]">{subtitle}</p>
      {#if tunnelEnabled}
        <p class="mt-1 text-[11px] text-[var(--text-muted)]">
          {tunnelLabel} 隧道随服务自动连接，停止服务时一并断开
        </p>
      {/if}
    </div>
    {#if showToggle}
      <Button
        variant={running ? "danger" : "primary"}
        size="md"
        class="shrink-0"
        {busy}
        disabled={status === "starting" || status === "stopping"}
        onclick={onToggle}
      >
        {running ? "停止" : "启动"}
      </Button>
    {/if}
  </div>

  {#if showError}
    <div class="mt-4 rounded-xl border border-[var(--danger)]/30 bg-[var(--danger-soft)] p-3 text-xs text-[var(--danger)] leading-relaxed" role="alert">
      {statusMessage}
    </div>
  {/if}

  <div class="tx-service-info-grid mt-5">
    <div class="tx-info-block">
      <div class="tx-info-row">
        <span class="tx-info-label">端口</span>
        {#if canEditPort}
          <input
            type="number"
            min="1024"
            max="65535"
            class="tx-input tx-input-inline font-mono text-xs"
            bind:value={draftPort}
            onchange={commitPort}
          />
        {:else}
          <span class="font-mono text-xs text-[var(--text-main)]">{port}</span>
        {/if}
      </div>
    </div>

    <div class="tx-info-block">
      <div class="tx-info-row">
        <span class="tx-info-label">本地地址</span>
        <CopyButton value={localEndpoint} />
      </div>
      <p class="font-mono mt-1.5 truncate text-xs text-[var(--text-main)]">{localEndpoint}</p>
    </div>

    {#if publicEndpoint || publicLabel}
      <div class="tx-info-block">
        <div class="tx-info-row">
          <span class="tx-info-label">{publicLabel}</span>
          {#if publicEndpoint}
            <CopyButton value={publicEndpoint} />
          {/if}
        </div>
        <p class="font-mono mt-1.5 truncate text-xs text-[var(--text-secondary)]">
          {publicEndpoint || "未配置公网隧道"}
        </p>
      </div>
    {/if}
  </div>
</article>
