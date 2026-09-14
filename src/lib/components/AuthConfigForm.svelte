<script lang="ts">
  import { message } from "@tauri-apps/plugin-dialog";
  import SecretInput from "$lib/components/SecretInput.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Toggle from "$lib/components/ui/Toggle.svelte";
  import Select from "$lib/components/ui/Select.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";
  import ConfirmDialog from "$lib/components/ui/ConfirmDialog.svelte";
  import {
    getWorkspaceSecret,
    regenerateWorkspaceSecret,
    getSharedSecret,
    setSharedSecret,
    regenerateSharedSecret,
    type WorkspaceSecretKey,
    type SharedSecretKey,
  } from "$lib/api/secrets";
  import type { AuthConfig } from "$lib/types";

  export interface SaveAuthOptions {
    skipRuntimeRestart?: boolean;
  }

  interface Props {
    workspaceId: string;
    auth: AuthConfig;
    onSaveProfile: (auth: AuthConfig, options?: SaveAuthOptions) => void | Promise<void>;
  }

  const AUTH_OPTIONS = [
    { value: "oauth", label: "OAuth" },
    { value: "bearer", label: "Bearer Token" },
    { value: "noauth", label: "不启用认证" },
  ] as const;

  let { workspaceId, auth, onSaveProfile }: Props = $props();

  let draft = $state<AuthConfig>({ type: "oauth", oauth_client_id: "", use_shared_secrets: false });
  let saving = $state(false);
  let secrets = $state<Partial<Record<WorkspaceSecretKey, string>>>({});
  let loadedSecrets = $state<Partial<Record<WorkspaceSecretKey, string>>>({});
  let loadedSharedOauthClientId = $state("");
  let regenerating = $state<WorkspaceSecretKey | null>(null);
  let confirmRegenerateKey = $state<WorkspaceSecretKey | null>(null);
  let secretsLoadSeq = 0;
  let suppressSecretsReload = $state(false);

  const secretsDirty = $derived(
    (Object.keys(secrets) as WorkspaceSecretKey[]).some(
      (k) => secrets[k] !== loadedSecrets[k],
    ),
  );

  const dirty = $derived(
    draft.type !== auth.type ||
      (draft.use_shared_secrets
        ? draft.oauth_client_id !== loadedSharedOauthClientId
        : draft.oauth_client_id !== auth.oauth_client_id) ||
      draft.use_shared_secrets !== !!auth.use_shared_secrets ||
      secretsDirty,
  );

  const showOAuth = $derived(draft.type === "oauth");
  const showBearer = $derived(draft.type === "bearer");

  $effect(() => {
    draft = { type: auth.type, oauth_client_id: auth.oauth_client_id, use_shared_secrets: !!auth.use_shared_secrets };
  });

  $effect(() => {
    if (suppressSecretsReload) return;
    const id = workspaceId;
    const authType = draft.type;
    const useShared = draft.use_shared_secrets ?? false;
    void loadSecrets(id, authType, useShared);
  });

  async function loadSecrets(id: string, authType: string, useShared: boolean) {
    const seq = ++secretsLoadSeq;
    const sharedClientId =
      authType === "oauth" && useShared ? await getSharedSecret("oauth_client_id") : null;
    const keys: WorkspaceSecretKey[] = [];
    if (authType === "oauth") {
      keys.push("oauth_client_secret", "oauth_password");
    } else if (authType === "bearer") {
      keys.push("bearer_token");
    }
    if (keys.length === 0) {
      if (seq !== secretsLoadSeq) return;
      secrets = {};
      loadedSecrets = {};
      return;
    }
    const loaded = await Promise.all(
      keys.map(async (key) => {
        const value = useShared
          ? await getSharedSecret(key as SharedSecretKey)
          : await getWorkspaceSecret(id, key);
        return [key, value ?? ""] as const;
      }),
    );
    if (seq !== secretsLoadSeq) return;
    if (authType === "oauth" && useShared) {
      draft = { ...draft, oauth_client_id: sharedClientId ?? "" };
      loadedSharedOauthClientId = sharedClientId ?? "";
    } else {
      loadedSharedOauthClientId = "";
    }
    secrets = Object.fromEntries(loaded);
    loadedSecrets = Object.fromEntries(loaded);
  }

  async function save() {
    if (saving || !dirty) return;
    saving = true;
    suppressSecretsReload = true;
    try {
      let sharedSecretChanged = false;
      const clientId =
        draft.type === "oauth" && draft.use_shared_secrets
          ? draft.oauth_client_id.trim()
          : "";
      if (draft.type === "oauth" && draft.use_shared_secrets) {
        if (!clientId) throw new Error("OAuth Client ID 不能为空");
        sharedSecretChanged = clientId !== loadedSharedOauthClientId;
      }
      // Persist profile first so secret-triggered restart sees updated flags.
      await onSaveProfile({ ...draft }, { skipRuntimeRestart: sharedSecretChanged });
      if (sharedSecretChanged) {
        await setSharedSecret("oauth_client_id", clientId);
        loadedSharedOauthClientId = clientId;
      }
      // Auth save only persists profile fields; secrets are already stored by regenerate.
      loadedSecrets = { ...secrets };
    } catch (error) {
      await message(String(error), { title: "保存失败", kind: "error" });
    } finally {
      suppressSecretsReload = false;
      saving = false;
    }
  }

  async function handleConfirmRegenerate() {
    if (!confirmRegenerateKey) return;
    const key = confirmRegenerateKey;
    confirmRegenerateKey = null;
    if (regenerating) return;
    regenerating = key;
    try {
      const value = draft.use_shared_secrets
        ? await regenerateSharedSecret(key as SharedSecretKey)
        : await regenerateWorkspaceSecret(workspaceId, key);
      secrets = { ...secrets, [key]: value };
    } catch (error) {
      await message(String(error), { title: "重新生成失败", kind: "error" });
    } finally {
      regenerating = null;
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
  <p class="text-xs text-[var(--color-text-muted)]">
    复制 Client ID / 密钥等请用上方「GPT 配置」卡片；此处可修改认证类型与重新生成密钥。
  </p>

  <label class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">认证类型</span>
    <Select
      options={AUTH_OPTIONS}
      bind:value={draft.type}
    />
  </label>

  <div class="rounded-lg border border-[var(--border)] bg-[var(--card-bg)] p-3">
    <Toggle
      bind:checked={draft.use_shared_secrets}
      label="使用全局共享密钥"
      description="在「设置 → 共享密钥」中统一管理，多个工作区复用相同认证凭据"
    />
  </div>

  {#if showOAuth}
    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">OAuth 客户端 ID</span>
      <TextInput
        mono
        bind:value={draft.oauth_client_id}
        readonly={draft.use_shared_secrets}
      />
    </div>

    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">OAuth 客户端密钥</span>
      <SecretInput
        value={secrets.oauth_client_secret ?? ""}
        placeholder="加载中…"
        readonly
        onRegenerate={() => { confirmRegenerateKey = "oauth_client_secret"; }}
        regenerating={regenerating === "oauth_client_secret"}
      />
    </div>

    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">授权口令</span>
      <SecretInput
        value={secrets.oauth_password ?? ""}
        placeholder="ChatGPT 首次授权时输入这个口令"
        readonly
        onRegenerate={() => { confirmRegenerateKey = "oauth_password"; }}
        regenerating={regenerating === "oauth_password"}
      />
    </div>
  {/if}

  {#if showBearer}
    <div class="grid gap-1.5">
      <span class="text-xs font-medium text-[var(--color-text-muted)]">Bearer Token</span>
      <SecretInput
        value={secrets.bearer_token ?? ""}
        placeholder="加载中…"
        readonly
        onRegenerate={() => { confirmRegenerateKey = "bearer_token"; }}
        regenerating={regenerating === "bearer_token"}
      />
    </div>
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
  open={!!confirmRegenerateKey}
  title="重新生成密钥确认"
  confirmText="确认重新生成"
  severity="warning"
  busy={!!regenerating}
  onConfirm={handleConfirmRegenerate}
  onCancel={() => { confirmRegenerateKey = null; }}
>
  <p>重新生成后，之前配置在外部 AI 客户端（如 ChatGPT）中的密钥将立即失效，需重新填入新密钥后方可恢复连接。</p>
</ConfirmDialog>
