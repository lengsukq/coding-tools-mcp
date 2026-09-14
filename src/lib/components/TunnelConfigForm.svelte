<script lang="ts">
  import { onMount } from "svelte";
  import { listFrpProfiles, type FrpProfileDto } from "$lib/api/settings";
  import { testTunnel as invokeTunnelTest } from "$lib/api/tunnel";
  import SecretTokenField from "$lib/components/SecretTokenField.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Select from "$lib/components/ui/Select.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";
  import Toggle from "$lib/components/ui/Toggle.svelte";
  import { showToast } from "$lib/stores/toast";

  export interface TunnelFormConfig {
    type: string;
    public_url: string;
    frp_server: string;
    frp_subdomain: string;
    frp_profile_id: string;
    frp_server_port: number;
    cloudflare_mode: string;
    use_proxy: boolean;
    use_global_gateway: boolean;
  }

  export interface SaveTunnelOptions {
    skipTunnelRestart?: boolean;
    skipServicePrompt?: boolean;
  }

  interface Props {
    workspaceId: string;
    config: TunnelFormConfig;
    onSave: (config: TunnelFormConfig, options?: SaveTunnelOptions) => void | Promise<void>;
  }

  const TUNNEL_TYPE_OPTIONS = [
    { value: "none", label: "未配置" },
    { value: "frp", label: "FRP" },
    { value: "cloudflare", label: "Cloudflare" },
  ] as const;

  const CLOUDFLARE_MODE_OPTIONS = [
    { value: "quick", label: "Quick Tunnel" },
    { value: "named", label: "Named Tunnel" },
  ] as const;

  let { workspaceId, config, onSave }: Props = $props();

  let draft = $state<TunnelFormConfig>({
    type: "none",
    public_url: "",
    frp_server: "",
    frp_subdomain: "",
    frp_profile_id: "",
    frp_server_port: 7000,
    cloudflare_mode: "quick",
    use_proxy: true,
    use_global_gateway: false,
  });
  let saving = $state(false);
  let testing = $state(false);
  let tokenField = $state<SecretTokenField | null>(null);
  let tokenPending = $state(false);
  let frpProfiles = $state<FrpProfileDto[]>([]);
  let legacyFrpOpen = $state(false);

  const frpProfileOptions = $derived([
    { value: "", label: "手动填写（旧版）" },
    ...frpProfiles.map((p) => ({
      value: p.id,
      label: `${p.name} · ${p.server}:${p.serverPort}`,
    })),
  ]);

  const secretKey = $derived(
    draft.type === "frp" ? ("frp_token" as const) : ("cloudflare_token" as const),
  );

  const selectedProfile = $derived(
    frpProfiles.find((profile) => profile.id === draft.frp_profile_id) ?? null,
  );

  const useGlobalProfile = $derived(Boolean(draft.frp_profile_id && selectedProfile));

  const dirty = $derived(
    draft.type !== config.type ||
      draft.public_url !== config.public_url ||
      draft.frp_server !== config.frp_server ||
      draft.frp_subdomain !== config.frp_subdomain ||
      draft.frp_profile_id !== config.frp_profile_id ||
      draft.frp_server_port !== config.frp_server_port ||
      draft.cloudflare_mode !== config.cloudflare_mode ||
      draft.use_proxy !== config.use_proxy ||
      draft.use_global_gateway !== config.use_global_gateway ||
      tokenPending,
  );

  const showFrp = $derived(!draft.use_global_gateway && draft.type === "frp");
  const showCloudflare = $derived(!draft.use_global_gateway && draft.type === "cloudflare");
  const showCloudflareToken = $derived(showCloudflare && draft.cloudflare_mode === "named");
  const showLegacyFrpToken = $derived(showFrp && !useGlobalProfile);
  const canTest = $derived(!draft.use_global_gateway && (draft.type === "frp" || draft.type === "cloudflare"));

  $effect(() => {
    draft = {
      ...config,
      frp_profile_id: config.frp_profile_id ?? "",
      use_proxy: config.use_proxy ?? true,
      use_global_gateway: config.use_global_gateway ?? false,
    };
  });

  onMount(async () => {
    frpProfiles = await listFrpProfiles();
  });

  async function saveDraft(options?: SaveTunnelOptions) {
    if (tokenField && (showLegacyFrpToken || showCloudflareToken)) {
      await tokenField.saveIfDirty();
    }
    const payload: TunnelFormConfig = {
      ...draft,
      frp_server_port: normalizePort(draft.frp_server_port, 7000),
    };
    await onSave(payload, options);
  }

  function normalizePort(value: number | string | null | undefined, fallback: number): number {
    const parsed = typeof value === "number" ? value : Number(value);
    if (!Number.isFinite(parsed) || parsed < 1 || parsed > 65535) {
      throw new Error(`端口无效：请填写 1-65535 之间的整数（当前：${String(value)}）`);
    }
    return Math.trunc(parsed);
  }

  async function save() {
    if (saving || !dirty) return;
    saving = true;
    try {
      await saveDraft();
      showToast("隧道配置已保存。", { title: "保存成功", kind: "success" });
    } catch (error) {
      showToast(String(error), { title: "保存失败", kind: "error", duration: 8000 });
    } finally {
      saving = false;
    }
  }

  async function testTunnelConnection() {
    if (!canTest || testing) return;
    testing = true;
    try {
      if (dirty) {
        await saveDraft({ skipTunnelRestart: true, skipServicePrompt: true });
      }

      const result = await invokeTunnelTest(workspaceId, "mcp");
      if (result.publicUrl && draft.cloudflare_mode === "quick") {
        draft.public_url = result.publicUrl;
      }

      if (result.success && result.publicUrl) {
        const detail = `${result.message}\n${result.publicUrl}${
          result.keptRunning ? "" : "\n\n如需长期使用，请先启动服务。"
        }`;
        showToast(detail, { title: "测试成功", kind: "success", duration: 8000 });
      } else if (result.success) {
        showToast(result.message, { title: "测试成功", kind: "success" });
      } else {
        showToast(result.message, { title: "测试未完成", kind: "warning", duration: 7000 });
      }
    } catch (error) {
      showToast(String(error), { title: "测试失败", kind: "error", duration: 8000 });
    } finally {
      testing = false;
    }
  }
</script>

<form
  class="grid gap-3.5"
  onsubmit={(event) => {
    event.preventDefault();
    void save();
  }}
>
  <div class="rounded-lg border border-[var(--border)] bg-[var(--card-bg)] p-3">
    <Toggle
      bind:checked={draft.use_global_gateway}
      label="使用全局共享公网入口"
      description="使用 /w/<workspace-id> 前缀转发，不再为该 Workspace 单独启动公网 Tunnel。"
    />
  </div>

  {#if !draft.use_global_gateway}
    <label class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">隧道类型</span>
      <Select
        options={TUNNEL_TYPE_OPTIONS}
        bind:value={draft.type}
      />
    </label>

    {#if canTest}
      <div class="rounded-lg border border-[var(--border)] bg-[var(--card-bg)] p-3">
        <Toggle
          bind:checked={draft.use_proxy}
          label="使用网络代理"
          description="启用后通过「设置 → 通用」中的全局代理连接隧道；关闭则直连（适合海外或已全局翻墙的环境）。"
        />
      </div>
    {/if}

    {#if showFrp}
      <label class="grid gap-1.5">
        <span class="text-xs font-medium text-[var(--color-text-muted)]">FRP 配置</span>
        <Select
          options={frpProfileOptions}
          bind:value={draft.frp_profile_id}
        />
        {#if frpProfiles.length === 0}
          <p class="text-[11px] text-[var(--color-text-muted)]">
            请先在侧边栏「FRP 配置」中添加全局服务器配置。
          </p>
        {/if}
      </label>

      {#if useGlobalProfile && selectedProfile}
        <div class="rounded-lg border border-[var(--border)] bg-[var(--surface-main)] px-3.5 py-2.5 text-xs">
          <p class="font-medium text-[var(--color-text-secondary)]">
            服务器：<span class="font-mono text-[var(--text-main)]">{selectedProfile.server}:{selectedProfile.serverPort}</span>
          </p>
          <p class="mt-1 text-[var(--color-text-muted)]">
            Token：{selectedProfile.hasToken ? "已配置" : "未配置"}
          </p>
        </div>
      {/if}

      <div class="grid gap-1.5">
        <span class="text-xs font-medium text-[var(--color-text-muted)]">子域名</span>
        <TextInput
          mono
          placeholder="my-mcp"
          bind:value={draft.frp_subdomain}
        />
        <p class="text-[11px] text-[var(--color-text-muted)]">
          每个工作区使用独立子域名；保存后若隧道已连接会自动重启 frpc。
        </p>
      </div>

      {#if !useGlobalProfile}
        <button
          type="button"
          class="text-left text-xs text-[var(--color-accent)] hover:underline"
          onclick={() => {
            legacyFrpOpen = !legacyFrpOpen;
          }}
        >
          {legacyFrpOpen ? "收起" : "展开"}手动 FRP 配置
        </button>
      {/if}

      {#if !useGlobalProfile && legacyFrpOpen}
        <div class="grid gap-1.5">
          <span class="text-xs font-medium text-[var(--color-text-muted)]">FRP 服务器</span>
          <TextInput
            mono
            placeholder="example.com"
            bind:value={draft.frp_server}
          />
        </div>

        <div class="grid gap-1.5">
          <span class="text-xs font-medium text-[var(--color-text-muted)]">FRP 服务器端口</span>
          <input
            type="number"
            min="1"
            max="65535"
            class="w-full px-3 py-1.5 text-xs rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all"
            bind:value={draft.frp_server_port}
          />
        </div>

        {#if showLegacyFrpToken}
          <SecretTokenField
            bind:this={tokenField}
            bind:hasPending={tokenPending}
            {workspaceId}
            secretKey={secretKey}
            label="FRP Token（可选）"
          />
        {/if}
      {/if}
    {/if}

    {#if showCloudflare}
      <label class="grid gap-1.5">
        <span class="text-xs font-medium text-[var(--color-text-muted)]">Cloudflare 模式</span>
        <Select
          options={CLOUDFLARE_MODE_OPTIONS}
          bind:value={draft.cloudflare_mode}
        />
      </label>

      {#if showCloudflareToken}
        <SecretTokenField
          bind:this={tokenField}
          bind:hasPending={tokenPending}
          {workspaceId}
          secretKey={secretKey}
        />
      {/if}
    {/if}

    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">
        公网 URL
      </span>
      <TextInput
        type="url"
        mono
        placeholder="https://..."
        bind:value={draft.public_url}
      />
    </div>
  {/if}

  <div class="flex justify-end gap-2.5 pt-1">
    {#if canTest}
      <Button
        type="button"
        variant="ghost"
        disabled={testing || saving}
        busy={testing}
        onclick={() => void testTunnelConnection()}
      >
        {testing ? "测试中…" : "测试连接"}
      </Button>
    {/if}
    <Button
      type="submit"
      variant="primary"
      disabled={saving || testing || !dirty}
      busy={saving}
    >
      {saving ? "保存中…" : "保存配置"}
    </Button>
  </div>
</form>
