<script lang="ts">
  import { message } from "@tauri-apps/plugin-dialog";
  import CopyButton from "$lib/components/CopyButton.svelte";
  import SecretInput from "$lib/components/SecretInput.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Toggle from "$lib/components/ui/Toggle.svelte";
  import Select from "$lib/components/ui/Select.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";
  import ConfirmDialog from "$lib/components/ui/ConfirmDialog.svelte";
  import { getSecret, regenerateSecret, getSharedSecret, regenerateSharedSecret } from "$lib/api/secrets";
  import type { ActionsAuthDraft } from "$lib/types";

  export const ACTIONS_AUTH_OPTIONS = [
    { value: "api_key", label: "API Key / Bearer" },
    { value: "none", label: "不启用认证" },
    { value: "oauth", label: "OAuth" },
  ] as const;

  export type { ActionsAuthDraft } from "$lib/types";

  interface Props {
    workspaceId: string;
    authType: string;
    oauthClientId: string;
    oauthScopes: string;
    openapiUrl: string;
    privacyUrl: string;
    oauthAuthorizeUrl: string;
    oauthTokenUrl: string;
    useSharedSecrets?: boolean;
    onSave: (draft: ActionsAuthDraft) => void | Promise<void>;
  }

  let {
    workspaceId,
    authType,
    oauthClientId,
    oauthScopes,
    openapiUrl,
    privacyUrl,
    oauthAuthorizeUrl,
    oauthTokenUrl,
    useSharedSecrets = false,
    onSave,
  }: Props = $props();

  let draftAuthType = $state("api_key");
  let draftOauthClientId = $state("");
  let draftOauthScopes = $state("");
  let draftUseShared = $state(false);
  let apiKey = $state("");
  let loadedApiKey = $state("");
  let oauthClientSecret = $state("");
  let loadedOauthClientSecret = $state("");
  let oauthPassword = $state("");
  let loadedOauthPassword = $state("");
  let oauthTokenSecret = $state("");
  let loadedOauthTokenSecret = $state("");
  let loadingKey = $state(true);
  let loadingOAuthSecret = $state(true);
  let loadingOAuthPassword = $state(true);
  let loadingOAuthTokenSecret = $state(true);
  let regenerating = $state(false);
  let regeneratingOAuthSecret = $state(false);
  let regeneratingOAuthPassword = $state(false);
  let regeneratingOAuthTokenSecret = $state(false);
  let confirmRegenerateTarget = $state<"api_key" | "oauth_secret" | "oauth_password" | "oauth_token_secret" | null>(null);
  let saving = $state(false);
  let secretsLoadSeq = 0;
  let suppressSecretsReload = $state(false);

  const secretsDirty = $derived(
    apiKey !== loadedApiKey ||
      oauthClientSecret !== loadedOauthClientSecret ||
      oauthPassword !== loadedOauthPassword ||
      oauthTokenSecret !== loadedOauthTokenSecret,
  );

  const dirty = $derived(
    draftAuthType !== authType ||
      draftOauthClientId !== oauthClientId ||
      draftOauthScopes !== oauthScopes ||
      draftUseShared !== useSharedSecrets ||
      secretsDirty,
  );
  const showApiKey = $derived(draftAuthType === "api_key");
  const showOAuth = $derived(draftAuthType === "oauth");

  $effect(() => {
    draftAuthType = authType;
    draftOauthClientId = oauthClientId;
    draftOauthScopes = oauthScopes;
    draftUseShared = useSharedSecrets;
  });

  $effect(() => {
    if (suppressSecretsReload) return;
    workspaceId;
    draftUseShared;
    void loadSecrets();
  });

  async function loadSecrets() {
    const seq = ++secretsLoadSeq;
    loadingKey = true;
    loadingOAuthSecret = true;
    loadingOAuthPassword = true;
    loadingOAuthTokenSecret = true;
    try {
      const [key, secret, password, tokenSecret] = await Promise.all([
        draftUseShared
          ? getSharedSecret("actions_api_key")
          : getSecret(workspaceId, "actions_api_key"),
        draftUseShared
          ? getSharedSecret("actions_oauth_client_secret")
          : getSecret(workspaceId, "actions_oauth_client_secret"),
        draftUseShared
          ? getSharedSecret("actions_oauth_password")
          : getSecret(workspaceId, "actions_oauth_password"),
        draftUseShared
          ? getSharedSecret("actions_oauth_token_secret")
          : getSecret(workspaceId, "actions_oauth_token_secret"),
      ]);
      if (seq !== secretsLoadSeq) return;
      apiKey = key ?? "";
      loadedApiKey = key ?? "";
      oauthClientSecret = secret ?? "";
      loadedOauthClientSecret = secret ?? "";
      oauthPassword = password ?? "";
      loadedOauthPassword = password ?? "";
      oauthTokenSecret = tokenSecret ?? "";
      loadedOauthTokenSecret = tokenSecret ?? "";
    } finally {
      if (seq !== secretsLoadSeq) return;
      loadingKey = false;
      loadingOAuthSecret = false;
      loadingOAuthPassword = false;
      loadingOAuthTokenSecret = false;
    }
  }

  async function save() {
    if (saving || !dirty) return;
    saving = true;
    suppressSecretsReload = true;
    try {
      await onSave({
        authType: draftAuthType,
        oauthClientId: draftOauthClientId.trim(),
        oauthScopes: draftOauthScopes.trim(),
        useSharedSecrets: draftUseShared,
      });
      loadedApiKey = apiKey;
      loadedOauthClientSecret = oauthClientSecret;
      loadedOauthPassword = oauthPassword;
      loadedOauthTokenSecret = oauthTokenSecret;
    } catch (error) {
      await message(String(error), { title: "保存失败", kind: "error" });
    } finally {
      suppressSecretsReload = false;
      saving = false;
    }
  }

  async function regenerate() {
    if (regenerating) return;
    regenerating = true;
    try {
      apiKey = draftUseShared
        ? await regenerateSharedSecret("actions_api_key")
        : await regenerateSecret(workspaceId, "actions_api_key");
    } catch (error) {
      await message(String(error), { title: "重新生成失败", kind: "error" });
    } finally {
      regenerating = false;
    }
  }

  async function regenerateOAuthSecret() {
    if (regeneratingOAuthSecret) return;
    regeneratingOAuthSecret = true;
    try {
      oauthClientSecret = draftUseShared
        ? await regenerateSharedSecret("actions_oauth_client_secret")
        : await regenerateSecret(workspaceId, "actions_oauth_client_secret");
    } catch (error) {
      await message(String(error), { title: "重新生成失败", kind: "error" });
    } finally {
      regeneratingOAuthSecret = false;
    }
  }

  async function regenerateOAuthPassword() {
    if (regeneratingOAuthPassword) return;
    regeneratingOAuthPassword = true;
    try {
      oauthPassword = draftUseShared
        ? await regenerateSharedSecret("actions_oauth_password")
        : await regenerateSecret(workspaceId, "actions_oauth_password");
    } catch (error) {
      await message(String(error), { title: "重新生成失败", kind: "error" });
    } finally {
      regeneratingOAuthPassword = false;
    }
  }

  async function regenerateOAuthTokenSecret() {
    if (regeneratingOAuthTokenSecret) return;
    regeneratingOAuthTokenSecret = true;
    try {
      oauthTokenSecret = draftUseShared
        ? await regenerateSharedSecret("actions_oauth_token_secret")
        : await regenerateSecret(workspaceId, "actions_oauth_token_secret");
    } catch (error) {
      await message(String(error), { title: "重新生成失败", kind: "error" });
    } finally {
      regeneratingOAuthTokenSecret = false;
    }
  }
  async function handleConfirmRegenerate() {
    if (!confirmRegenerateTarget) return;
    const target = confirmRegenerateTarget;
    confirmRegenerateTarget = null;
    if (target === "api_key") await regenerate();
    else if (target === "oauth_secret") await regenerateOAuthSecret();
    else if (target === "oauth_password") await regenerateOAuthPassword();
    else if (target === "oauth_token_secret") await regenerateOAuthTokenSecret();
  }
</script>

<form
  class="grid gap-3.5"
  onsubmit={(event) => {
    event.preventDefault();
    void save();
  }}
>
  <p class="text-xs text-[var(--color-text-muted)]">
    复制 OpenAPI、密钥等请用上方「GPT 配置」卡片；此处仅修改认证方式与密钥。
  </p>

  <label class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">认证方式</span>
    <Select
      options={ACTIONS_AUTH_OPTIONS}
      bind:value={draftAuthType}
    />
  </label>

  <div class="rounded-lg border border-[var(--border)] bg-[var(--card-bg)] p-3">
    <Toggle
      bind:checked={draftUseShared}
      label="使用全局共享密钥"
      description="在「设置 → 共享密钥」中统一管理，多个工作区复用相同 Actions 凭据"
    />
  </div>

  {#if showApiKey}
    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">API Key（Bearer）</span>
      <SecretInput
        value={loadingKey ? "加载中…" : apiKey}
        readonly
        disabled={loadingKey}
        showCopy={!!apiKey}
        onRegenerate={() => { confirmRegenerateTarget = "api_key"; }}
        regenerating={regenerating}
      />
    </div>
    <p class="text-xs text-[var(--color-text-muted)]">
      在 GPT Actions 认证里选 API Key → Bearer，Key 填这里的值。
    </p>
  {:else if showOAuth}
    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">OAuth Client ID（填到 GPT）</span>
      <div class="flex gap-2">
        <TextInput
          mono
          class="flex-1 min-w-0"
          bind:value={draftOauthClientId}
        />
        {#if draftOauthClientId}
          <CopyButton value={draftOauthClientId} label="复制" />
        {/if}
      </div>
    </div>
    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">OAuth Client Secret（填到 GPT）</span>
      <SecretInput
        value={loadingOAuthSecret ? "加载中…" : oauthClientSecret}
        readonly
        disabled={loadingOAuthSecret}
        showCopy={!!oauthClientSecret}
        onRegenerate={() => { confirmRegenerateTarget = "oauth_secret"; }}
        regenerating={regeneratingOAuthSecret}
      />
    </div>
    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">OAuth Password（服务端校验）</span>
      <SecretInput
        value={loadingOAuthPassword ? "加载中…" : oauthPassword}
        readonly
        disabled={loadingOAuthPassword}
        showCopy={!!oauthPassword}
        onRegenerate={() => { confirmRegenerateTarget = "oauth_password"; }}
        regenerating={regeneratingOAuthPassword}
      />
    </div>
    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">OAuth Token Secret（JWT 签名）</span>
      <SecretInput
        value={loadingOAuthTokenSecret ? "加载中…" : oauthTokenSecret}
        readonly
        disabled={loadingOAuthTokenSecret}
        showCopy={!!oauthTokenSecret}
        onRegenerate={() => { confirmRegenerateTarget = "oauth_token_secret"; }}
        regenerating={regeneratingOAuthTokenSecret}
      />
    </div>
    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">Authorization URL（填到 GPT）</span>
      <div class="flex gap-2">
        <TextInput
          readonly
          mono
          class="flex-1 min-w-0"
          value={oauthAuthorizeUrl}
        />
        {#if oauthAuthorizeUrl}
          <CopyButton value={oauthAuthorizeUrl} label="复制" />
        {/if}
      </div>
    </div>
    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">Token URL（填到 GPT）</span>
      <div class="flex gap-2">
        <TextInput
          readonly
          mono
          class="flex-1 min-w-0"
          value={oauthTokenUrl}
        />
        {#if oauthTokenUrl}
          <CopyButton value={oauthTokenUrl} label="复制" />
        {/if}
      </div>
    </div>
    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">Scope（填到 GPT，空格分隔）</span>
      <TextInput
        placeholder="例如：coding-tools"
        bind:value={draftOauthScopes}
      />
    </div>
    <p class="text-xs text-[var(--color-text-muted)]">
      GPT 编辑器会生成 Callback URL（<code>https://chatgpt.com/aip/g-…/oauth/callback</code>），无需在本应用配置。Token
      交换方式选默认即可。
    </p>
  {:else}
    <p class="text-xs text-[var(--color-text-muted)]">
      不校验请求认证；GPT 侧选 None。仅建议本机调试，公网暴露请用 API Key 或 OAuth。
    </p>
  {/if}

  <div class="flex justify-end pt-1">
    <Button
      type="submit"
      variant="primary"
      busy={saving}
      disabled={saving || !dirty}
    >
      {saving ? "保存中…" : "保存配置"}
    </Button>
  </div>
</form>

<ConfirmDialog
  open={!!confirmRegenerateTarget}
  title="重新生成 Actions 凭据确认"
  confirmText="确认重新生成"
  severity="warning"
  busy={regenerating || regeneratingOAuthSecret || regeneratingOAuthPassword || regeneratingOAuthTokenSecret}
  onConfirm={handleConfirmRegenerate}
  onCancel={() => { confirmRegenerateTarget = null; }}
>
  <p>重新生成后，之前配置在外部 GPT Actions 中的密钥将立即失效，需同步更新至 ChatGPT 编辑器后方可恢复调用。</p>
</ConfirmDialog>
