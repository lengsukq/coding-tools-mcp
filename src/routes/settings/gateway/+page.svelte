<script lang="ts">
  import { onMount } from "svelte";
  import {
    DEFAULT_GLOBAL_GATEWAY,
    checkGlobalGatewayHealth,
    getGlobalGatewayConfig,
    getGlobalGatewayStatus,
    setGlobalGatewayConfig,
    startGlobalGateway,
    stopGlobalGateway,
    type GatewayHealthItemDto,
    type GlobalGatewayConfigDto,
    type GlobalGatewayStatusDto,
  } from "$lib/api/global-gateway";
  import { listFrpProfiles, type FrpProfileDto } from "$lib/api/settings";
  import { listWorkspaces } from "$lib/api/workspaces";
  import { actionsConfig, type WorkspaceProfile } from "$lib/types";
  import { showToast } from "$lib/stores/toast";

  let config = $state<GlobalGatewayConfigDto>({ ...DEFAULT_GLOBAL_GATEWAY });
  let savedConfig = $state<GlobalGatewayConfigDto>({ ...DEFAULT_GLOBAL_GATEWAY });
  let status = $state<GlobalGatewayStatusDto | null>(null);
  let health = $state<GatewayHealthItemDto[]>([]);
  let workspaces = $state<WorkspaceProfile[]>([]);
  let frpProfiles = $state<FrpProfileDto[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let busy = $state(false);
  let checking = $state(false);

  const running = $derived(status?.state === "running");
  const dirty = $derived(JSON.stringify(config) !== JSON.stringify(savedConfig));
  const gatewayRoutes = $derived(
    workspaces.flatMap((workspace) => {
      const routes: Array<{ workspace: string; service: string; path: string }> = [];
      if (workspace.tunnel.use_global_gateway) {
        routes.push({
          workspace: workspace.name,
          service: "MCP",
          path: `/w/${workspace.id}/mcp`,
        });
      }
      if (actionsConfig(workspace).use_global_gateway) {
        routes.push({
          workspace: workspace.name,
          service: "Actions",
          path: `/w/${workspace.id}/actions`,
        });
      }
      return routes;
    }),
  );

  async function refresh() {
    loading = true;
    try {
      const [nextConfig, nextStatus, nextWorkspaces, nextFrpProfiles] = await Promise.all([
        getGlobalGatewayConfig(),
        getGlobalGatewayStatus(),
        listWorkspaces(),
        listFrpProfiles(),
      ]);
      config = { ...DEFAULT_GLOBAL_GATEWAY, ...nextConfig };
      savedConfig = { ...config };
      status = nextStatus;
      workspaces = nextWorkspaces;
      frpProfiles = nextFrpProfiles;
    } catch (error) {
      showToast(String(error), { title: "加载 Global Gateway 失败", kind: "error", duration: 8000 });
    } finally {
      loading = false;
    }
  }

  async function saveConfig() {
    if (!dirty || saving) return;
    saving = true;
    try {
      await setGlobalGatewayConfig({
        ...config,
        localPort: normalizePort(config.localPort, 28765),
        frpServerPort: normalizePort(config.frpServerPort, 7000),
        publicUrl: config.publicUrl.trim(),
        frpServer: config.frpServer.trim(),
        frpSubdomain: config.frpSubdomain.trim(),
      });
      savedConfig = { ...config };
      showToast("Global Gateway 配置已保存。运行中的 Gateway 需要重新启动后应用新配置。", {
        kind: "success",
      });
    } catch (error) {
      showToast(String(error), { title: "保存失败", kind: "error", duration: 8000 });
    } finally {
      saving = false;
    }
  }

  async function start() {
    if (busy) return;
    busy = true;
    try {
      if (dirty) await saveConfig();
      status = await startGlobalGateway();
      await runHealthCheck();
      showToast("Global Gateway 已启动。", { kind: "success" });
    } catch (error) {
      showToast(String(error), { title: "启动失败", kind: "error", duration: 8000 });
    } finally {
      busy = false;
    }
  }

  async function stop() {
    if (busy) return;
    busy = true;
    try {
      await stopGlobalGateway();
      status = await getGlobalGatewayStatus();
      health = [];
      showToast("Global Gateway 已停止。", { kind: "info" });
    } catch (error) {
      showToast(String(error), { title: "停止失败", kind: "error", duration: 8000 });
    } finally {
      busy = false;
    }
  }

  async function runHealthCheck() {
    if (checking) return;
    checking = true;
    try {
      health = await checkGlobalGatewayHealth();
      status = await getGlobalGatewayStatus();
    } catch (error) {
      showToast(String(error), { title: "健康检查失败", kind: "error", duration: 8000 });
    } finally {
      checking = false;
    }
  }

  function normalizePort(value: number, fallback: number): number {
    const port = Number(value || fallback);
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      throw new Error(`端口无效：${value}`);
    }
    return port;
  }

  function fullRoute(path: string): string {
    const base = status?.publicUrl?.trim().replace(/\/$/, "");
    return base ? `${base}${path}` : path;
  }

  onMount(() => {
    void refresh();
  });
</script>

<section class="page-scroll">
  <header class="page-header">
    <p class="page-kicker">全局设置</p>
    <h2 class="page-title">Global Gateway</h2>
    <p class="mt-2 max-w-3xl text-sm text-[var(--color-text-muted)]">
      使用一个共享公网入口承载多个 Workspace，通过 <code>/w/&lt;workspace-id&gt;</code> 前缀进行隔离。
      Workspace 仍可选择继续使用自己的 FRP 或 Cloudflare Tunnel。
    </p>
  </header>

  <div class="page-body flex flex-col gap-6">
    <div class="tx-card p-4">
      <div class="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h3 class="text-sm font-semibold">运行状态</h3>
          <p class="mt-1 text-xs text-[var(--color-text-muted)]">{status?.detail ?? "正在读取状态…"}</p>
        </div>
        <span class="tx-status-pill" class:active={running}>
          {running ? "运行中" : "已停止"}
        </span>
      </div>

      <div class="mt-4 grid gap-3 md:grid-cols-2">
        <div class="tx-panel p-3">
          <p class="text-xs text-[var(--color-text-muted)]">本地入口</p>
          <p class="mt-1 break-all font-mono text-sm">{status?.localUrl ?? `http://127.0.0.1:${config.localPort}`}</p>
        </div>
        <div class="tx-panel p-3">
          <p class="text-xs text-[var(--color-text-muted)]">公网入口</p>
          <p class="mt-1 break-all font-mono text-sm">{status?.publicUrl || config.publicUrl || "尚未获取"}</p>
        </div>
      </div>

      <div class="mt-4 flex flex-wrap gap-2">
        <button type="button" class="tx-btn-primary" disabled={busy || running || loading} onclick={() => void start()}>
          {busy && !running ? "启动中…" : "启动 Gateway"}
        </button>
        <button type="button" class="tx-btn-ghost" disabled={busy || !running} onclick={() => void stop()}>
          停止 Gateway
        </button>
        <button type="button" class="tx-btn-ghost" disabled={checking || loading} onclick={() => void runHealthCheck()}>
          {checking ? "检查中…" : "运行健康检查"}
        </button>
        <button type="button" class="tx-btn-ghost" disabled={loading} onclick={() => void refresh()}>
          刷新状态
        </button>
      </div>
    </div>

    <div class="tx-card p-4">
      <h3 class="text-sm font-semibold">Gateway 配置</h3>
      <p class="mt-1 text-xs text-[var(--color-text-muted)]">
        开启后，使用 Global Gateway 的 Workspace 在启动 MCP / Actions 时会自动确保 Gateway 已运行。
      </p>

      <form class="mt-4 grid gap-3" onsubmit={(event) => { event.preventDefault(); void saveConfig(); }}>
        <label class="flex items-start gap-2 rounded-md border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-2.5">
          <input type="checkbox" class="mt-0.5 h-4 w-4" bind:checked={config.enabled} />
          <span class="grid gap-0.5">
            <span class="text-xs font-medium">启用 Global Gateway</span>
            <span class="text-[11px] text-[var(--color-text-muted)]">关闭后，Workspace 无法通过共享 Gateway 启动公网入口。</span>
          </span>
        </label>

        <label class="grid gap-1">
          <span class="text-xs text-[var(--color-text-muted)]">本地监听端口</span>
          <input type="number" min="1" max="65535" class="tx-input" bind:value={config.localPort} />
        </label>

        <label class="grid gap-1">
          <span class="text-xs text-[var(--color-text-muted)]">公网方式</span>
          <select class="tx-input" bind:value={config.tunnelType}>
            <option value="none">仅本地 / 外部反向代理</option>
            <option value="frp">FRP</option>
            <option value="cloudflare">Cloudflare Quick Tunnel</option>
          </select>
        </label>

        {#if config.tunnelType === "frp"}
          <label class="grid gap-1">
            <span class="text-xs text-[var(--color-text-muted)]">FRP 配置</span>
            <select class="tx-input" bind:value={config.frpProfileId}>
              <option value="">手动填写</option>
              {#each frpProfiles as profile (profile.id)}
                <option value={profile.id}>{profile.name} · {profile.server}:{profile.serverPort}</option>
              {/each}
            </select>
          </label>

          <label class="grid gap-1">
            <span class="text-xs text-[var(--color-text-muted)]">子域名</span>
            <input class="tx-input tx-mono" placeholder="coding-tools" bind:value={config.frpSubdomain} />
          </label>

          {#if !config.frpProfileId}
            <div class="grid gap-3 md:grid-cols-[1fr_140px]">
              <label class="grid gap-1">
                <span class="text-xs text-[var(--color-text-muted)]">FRP 服务器</span>
                <input class="tx-input tx-mono" placeholder="frp.example.com" bind:value={config.frpServer} />
              </label>
              <label class="grid gap-1">
                <span class="text-xs text-[var(--color-text-muted)]">端口</span>
                <input type="number" min="1" max="65535" class="tx-input" bind:value={config.frpServerPort} />
              </label>
            </div>
          {/if}
        {/if}

        {#if config.tunnelType === "cloudflare"}
          <div class="tx-panel p-3 text-xs text-[var(--color-text-muted)]">
            Global Gateway 当前使用 Cloudflare Quick Tunnel。需要固定域名时建议使用 FRP 或外部反向代理；Workspace 独立 Tunnel 仍保留 Named Cloudflare。
          </div>
        {/if}

        {#if config.tunnelType === "none"}
          <label class="grid gap-1">
            <span class="text-xs text-[var(--color-text-muted)]">外部公网 URL（可选）</span>
            <input type="url" class="tx-input tx-mono" placeholder="https://gateway.example.com" bind:value={config.publicUrl} />
          </label>
        {/if}

        {#if config.tunnelType !== "none"}
          <label class="flex items-start gap-2">
            <input type="checkbox" class="mt-0.5 h-4 w-4" bind:checked={config.useProxy} />
            <span class="text-xs text-[var(--color-text-muted)]">使用「通用」页面中的全局网络代理连接公网 Tunnel。</span>
          </label>
        {/if}

        <div class="flex justify-end">
          <button type="submit" class="tx-btn-primary" disabled={!dirty || saving}>
            {saving ? "保存中…" : "保存 Gateway 配置"}
          </button>
        </div>
      </form>
    </div>

    <div class="tx-card p-4">
      <h3 class="text-sm font-semibold">Workspace Routes</h3>
      <p class="mt-1 text-xs text-[var(--color-text-muted)]">
        只有在 Workspace 的 MCP 或 Actions 隧道配置中勾选「使用全局共享公网入口」的服务才会出现在这里。
      </p>

      {#if gatewayRoutes.length === 0}
        <p class="mt-4 text-sm text-[var(--color-text-muted)]">当前没有 Workspace 使用 Global Gateway。</p>
      {:else}
        <div class="mt-4 grid gap-2">
          {#each gatewayRoutes as route (`${route.workspace}-${route.service}-${route.path}`)}
            <div class="tx-panel grid gap-1 p-3 md:grid-cols-[180px_90px_1fr] md:items-center">
              <span class="text-sm font-medium">{route.workspace}</span>
              <span class="text-xs text-[var(--color-text-muted)]">{route.service}</span>
              <code class="break-all text-xs">{fullRoute(route.path)}</code>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="tx-card p-4">
      <h3 class="text-sm font-semibold">健康检查</h3>
      {#if health.length === 0}
        <p class="mt-3 text-sm text-[var(--color-text-muted)]">尚未执行健康检查。</p>
      {:else}
        <div class="mt-4 grid gap-2">
          {#each health as item (item.label)}
            <div class="tx-panel flex items-start justify-between gap-3 p-3">
              <div>
                <p class="text-sm font-medium">{item.label}</p>
                <p class="mt-1 break-all text-xs text-[var(--color-text-muted)]">{item.detail}</p>
              </div>
              <span class="shrink-0 text-xs font-medium" class:text-green-500={item.ok} class:text-red-400={!item.ok}>
                {item.ok ? "正常" : "失败"}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</section>
