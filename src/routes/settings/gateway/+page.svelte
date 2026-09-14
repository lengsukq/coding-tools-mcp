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
  import type { WorkspaceProfile } from "$lib/types";
  import { showToast } from "$lib/stores/toast";
  import ConnectionSettingsNav from "$lib/components/ConnectionSettingsNav.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Toggle from "$lib/components/ui/Toggle.svelte";
  import Select from "$lib/components/ui/Select.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import StatusBadge from "$lib/components/ui/StatusBadge.svelte";

  const TUNNEL_TYPE_OPTIONS = [
    { value: "none", label: "仅本地 / 外部反向代理" },
    { value: "frp", label: "FRP" },
    { value: "cloudflare", label: "Cloudflare Quick Tunnel" },
  ] as const;

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
  const frpProfileOptions = $derived([
    { value: "", label: "手动填写" },
    ...frpProfiles.map((profile) => ({
      value: profile.id,
      label: `${profile.name} · ${profile.server}:${profile.serverPort}`,
    })),
  ]);

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

  <ConnectionSettingsNav />

  <div class="page-body flex flex-col gap-6">
    <Card class="p-5">
      <div class="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h3 class="text-sm font-semibold text-[var(--text-main)]">运行状态</h3>
          <p class="mt-0.5 text-xs text-[var(--color-text-muted)]">{status?.detail ?? "正在读取状态…"}</p>
        </div>
        <StatusBadge
          status={running ? "running" : "stopped"}
          text={running ? "运行中" : "已停止"}
        />
      </div>

      <div class="mt-4 grid gap-3 md:grid-cols-2">
        <div class="rounded-xl border border-[var(--border)] bg-[var(--card-bg)] p-3.5 shadow-sm">
          <p class="text-xs font-medium text-[var(--color-text-muted)]">本地入口</p>
          <p class="mt-1 break-all font-mono text-xs font-medium text-[var(--text-main)]">{status?.localUrl ?? `http://127.0.0.1:${config.localPort}`}</p>
        </div>
        <div class="rounded-xl border border-[var(--border)] bg-[var(--card-bg)] p-3.5 shadow-sm">
          <p class="text-xs font-medium text-[var(--color-text-muted)]">公网入口</p>
          <p class="mt-1 break-all font-mono text-xs font-medium text-[var(--text-main)]">{status?.publicUrl || config.publicUrl || "尚未获取"}</p>
        </div>
      </div>

      <div class="mt-5 flex flex-wrap items-center gap-2.5 pt-3 border-t border-[var(--border)]">
        <Button
          type="button"
          variant="primary"
          size="sm"
          disabled={busy || running || loading}
          busy={busy && !running}
          onclick={() => void start()}
        >
          {busy && !running ? "启动中…" : "启动 Gateway"}
        </Button>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={busy || !running}
          busy={busy && running}
          onclick={() => void stop()}
        >
          停止 Gateway
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={checking || loading}
          busy={checking}
          onclick={() => void runHealthCheck()}
        >
          {checking ? "检查中…" : "运行健康检查"}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={loading}
          busy={loading}
          onclick={() => void refresh()}
        >
          刷新状态
        </Button>
      </div>
    </Card>

    <Card class="p-5">
      <h3 class="text-sm font-semibold text-[var(--text-main)]">Gateway 配置</h3>
      <p class="mt-0.5 text-xs text-[var(--color-text-muted)]">
        开启后，使用 Global Gateway 的 Workspace 在启动 MCP 时会自动确保 Gateway 已运行。
      </p>

      <form class="mt-4 grid gap-3.5" onsubmit={(event) => { event.preventDefault(); void saveConfig(); }}>
        <div class="rounded-lg border border-[var(--border)] bg-[var(--card-bg)] p-3">
          <Toggle
            bind:checked={config.enabled}
            label="启用 Global Gateway"
            description="关闭后，Workspace 无法通过共享 Gateway 启动公网入口。"
          />
        </div>

        <div class="grid gap-1.5">
          <span class="text-xs font-medium text-[var(--color-text-muted)]">本地监听端口</span>
          <input
            type="number"
            min="1"
            max="65535"
            class="w-full px-3 py-1.5 text-xs rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all"
            bind:value={config.localPort}
          />
        </div>

        <label class="grid gap-1.5">
          <span class="text-xs font-medium text-[var(--color-text-muted)]">公网方式</span>
          <Select
            options={TUNNEL_TYPE_OPTIONS}
            bind:value={config.tunnelType}
          />
        </label>

        {#if config.tunnelType === "frp"}
          <label class="grid gap-1.5">
            <span class="text-xs font-medium text-[var(--color-text-muted)]">FRP 配置</span>
            <Select
              options={frpProfileOptions}
              bind:value={config.frpProfileId}
            />
          </label>

          <div class="grid gap-1.5">
            <span class="text-xs font-medium text-[var(--color-text-muted)]">子域名</span>
            <TextInput
              mono
              placeholder="coding-tools"
              bind:value={config.frpSubdomain}
            />
          </div>

          {#if !config.frpProfileId}
            <div class="grid gap-3 md:grid-cols-[1fr_140px]">
              <div class="grid gap-1.5">
                <span class="text-xs font-medium text-[var(--color-text-muted)]">FRP 服务器</span>
                <TextInput
                  mono
                  placeholder="frp.example.com"
                  bind:value={config.frpServer}
                />
              </div>
              <div class="grid gap-1.5">
                <span class="text-xs font-medium text-[var(--color-text-muted)]">端口</span>
                <input
                  type="number"
                  min="1"
                  max="65535"
                  class="w-full px-3 py-1.5 text-xs rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all"
                  bind:value={config.frpServerPort}
                />
              </div>
            </div>
          {/if}
        {/if}

        {#if config.tunnelType === "cloudflare"}
          <div class="rounded-lg border border-[var(--border)] bg-[var(--surface-main)] p-3 text-xs text-[var(--color-text-muted)]">
            Global Gateway 当前使用 Cloudflare Quick Tunnel。需要固定域名时建议使用 FRP 或外部反向代理；Workspace 独立 Tunnel 仍保留 Named Cloudflare。
          </div>
        {/if}

        {#if config.tunnelType === "none"}
          <div class="grid gap-1.5">
            <span class="text-xs font-medium text-[var(--color-text-muted)]">外部公网 URL（可选）</span>
            <TextInput
              mono
              type="url"
              placeholder="https://gateway.example.com"
              bind:value={config.publicUrl}
            />
          </div>
        {/if}

        {#if config.tunnelType !== "none"}
          <div class="rounded-lg border border-[var(--border)] bg-[var(--card-bg)] p-3">
            <Toggle
              bind:checked={config.useProxy}
              label="使用全局代理"
              description="使用「通用」设置页面中的全局网络代理连接公网 Tunnel。"
            />
          </div>
        {/if}

        <div class="flex justify-end pt-1">
          <Button
            type="submit"
            variant="primary"
            disabled={!dirty || saving}
            busy={saving}
          >
            {saving ? "保存中…" : "保存 Gateway 配置"}
          </Button>
        </div>
      </form>
    </Card>

    <Card class="p-5">
      <h3 class="text-sm font-semibold text-[var(--text-main)]">Workspace Routes</h3>
      <p class="mt-0.5 text-xs text-[var(--color-text-muted)]">
        只有在 Workspace 的 MCP 隧道配置中勾选「使用全局共享公网入口」的服务才会出现在这里。
      </p>

      {#if gatewayRoutes.length === 0}
        <p class="mt-4 text-xs text-[var(--color-text-muted)]">当前没有 Workspace 使用 Global Gateway。</p>
      {:else}
        <div class="mt-4 grid gap-2">
          {#each gatewayRoutes as route (`${route.workspace}-${route.service}-${route.path}`)}
            <div class="grid gap-1.5 rounded-xl border border-[var(--border)] bg-[var(--card-bg)] p-3 shadow-sm md:grid-cols-[180px_90px_1fr] md:items-center">
              <span class="text-xs font-semibold text-[var(--text-main)] truncate">{route.workspace}</span>
              <span class="text-xs font-medium text-[var(--primary)]">{route.service}</span>
              <code class="break-all font-mono text-[11px] text-[var(--text-secondary)]">{fullRoute(route.path)}</code>
            </div>
          {/each}
        </div>
      {/if}
    </Card>

    <Card class="p-5">
      <h3 class="text-sm font-semibold text-[var(--text-main)]">健康检查</h3>
      {#if health.length === 0}
        <p class="mt-3 text-xs text-[var(--color-text-muted)]">尚未执行健康检查。</p>
      {:else}
        <div class="mt-4 grid gap-2">
          {#each health as item (item.label)}
            <div class="flex items-start justify-between gap-3 rounded-xl border border-[var(--border)] bg-[var(--card-bg)] p-3.5 shadow-sm">
              <div class="min-w-0 flex-1">
                <p class="text-xs font-semibold text-[var(--text-main)]">{item.label}</p>
                <p class="mt-0.5 break-all text-[11px] text-[var(--color-text-muted)] leading-relaxed">{item.detail}</p>
              </div>
              <StatusBadge
                status={item.ok ? "success" : "error"}
                text={item.ok ? "正常" : "失败"}
                size="sm"
              />
            </div>
          {/each}
        </div>
      {/if}
    </Card>
  </div>
</section>
