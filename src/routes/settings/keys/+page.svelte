<script lang="ts">
  import { onMount } from "svelte";
  import { message } from "@tauri-apps/plugin-dialog";
  import SecretInput from "$lib/components/SecretInput.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import ConfirmDialog from "$lib/components/ui/ConfirmDialog.svelte";
  import {
    getSharedSecret,
    setSharedSecret,
    regenerateSharedSecret,
    type SharedSecretKey,
  } from "$lib/api/secrets";

  const MCP_KEYS: { key: SharedSecretKey; label: string }[] = [
    { key: "oauth_client_id", label: "MCP OAuth Client ID" },
    { key: "bearer_token", label: "MCP Bearer Token" },
    { key: "oauth_client_secret", label: "MCP OAuth 客户端密钥" },
    { key: "oauth_password", label: "MCP 授权口令" },
    { key: "oauth_token_secret", label: "MCP Token Secret" },
  ];

  const ALL_KEYS = MCP_KEYS;

  let secrets = $state<Record<string, string>>({});
  let originals = $state<Record<string, string>>({});
  let loading = $state(true);
  let saving = $state(false);
  let regenerating = $state<string | null>(null);
  let regenerateConfirmOpen = $state(false);
  let pendingKey = $state<SharedSecretKey | null>(null);

  const dirty = $derived(ALL_KEYS.some(({ key }) => secrets[key] !== undefined && secrets[key] !== originals[key]));

  async function loadAll() {
    loading = true;
    try {
      const results = await Promise.all(
        ALL_KEYS.map(async ({ key }) => {
          let value = "";
          try {
            value = (await getSharedSecret(key)) ?? "";
          } catch {
            // Individual key load failure — show empty for this key
          }
          return [key, value] as const;
        }),
      );
      for (const [key, value] of results) {
        secrets[key] = value;
        originals[key] = value;
      }
    } finally {
      loading = false;
    }
  }

  function requestRegenerate(key: SharedSecretKey) {
    pendingKey = key;
    regenerateConfirmOpen = true;
  }

  async function handleConfirmRegenerate() {
    if (!pendingKey || regenerating) return;
    const key = pendingKey;
    regenerateConfirmOpen = false;
    regenerating = key;
    try {
      const value = await regenerateSharedSecret(key);
      secrets[key] = value;
    } catch (e) {
      await message(String(e), { title: "重新生成失败", kind: "error" });
    } finally {
      regenerating = null;
      pendingKey = null;
    }
  }

  async function saveAll() {
    saving = true;
    try {
      for (const { key } of ALL_KEYS) {
        if (secrets[key] !== undefined && secrets[key] !== originals[key]) {
          await setSharedSecret(key, secrets[key]);
          originals[key] = secrets[key];
        }
      }
    } catch (e) {
      await message(String(e), { title: "保存失败", kind: "error" });
    } finally {
      saving = false;
    }
  }

  onMount(loadAll);
</script>

<section class="page-scroll">
  <header class="page-header">
    <p class="page-kicker">全局设置</p>
    <h2 class="page-title">共享密钥</h2>
    <p class="mt-2 max-w-2xl text-xs leading-relaxed text-[var(--color-text-muted)]">
      在此统一管理 MCP 共享密钥。各工作区可以选择使用共享密钥或专属密钥；重新生成或修改密钥后，正在运行的 MCP 服务将自动重启以生效。
    </p>
  </header>

  <div class="page-body flex flex-col gap-6">
    <div class="flex flex-col gap-6">
      <!-- MCP keys -->
      <div class="tx-card p-5">
        <h3 class="text-sm font-semibold tracking-tight text-[var(--text-main)]">MCP 认证密钥</h3>
        {#if loading}
          <p class="mt-4 text-xs text-[var(--color-text-muted)]">加载中…</p>
        {:else}
          <div class="mt-4 grid gap-4">
            {#each MCP_KEYS as { key, label }}
              <div class="grid gap-1.5">
                <span class="text-xs font-medium text-[var(--color-text-secondary)]">{label}</span>
                <SecretInput
                  bind:value={secrets[key]}
                  disabled={loading}
                  onRegenerate={() => requestRegenerate(key)}
                  regenerating={regenerating === key}
                />
              </div>
            {/each}
          </div>
        {/if}
      </div>

    </div>

    <div class="flex justify-end pt-2">
      <Button
        variant="primary"
        size="md"
        disabled={!dirty || saving}
        busy={saving}
        onclick={() => saveAll()}
      >
        {saving ? "保存中…" : "保存更改"}
      </Button>
    </div>
  </div>

  <ConfirmDialog
    open={regenerateConfirmOpen}
    title="重新生成密钥确认"
    message="重新生成后，旧密钥将立即作废！所有已配置旧密钥的外部 AI 助手或客户端将断连，您必须在外部重新填入新密钥。确定重新生成吗？"
    detail={pendingKey ? `目标密钥字段：${pendingKey}` : undefined}
    confirmText="确认重新生成"
    cancelText="取消"
    severity="warning"
    onConfirm={handleConfirmRegenerate}
    onCancel={() => {
      regenerateConfirmOpen = false;
      pendingKey = null;
    }}
  />
</section>
