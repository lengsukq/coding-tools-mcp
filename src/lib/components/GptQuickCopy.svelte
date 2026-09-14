<script lang="ts">
  import CopyFieldRow from "$lib/components/CopyFieldRow.svelte";
  import { getSecret, getSharedSecret } from "$lib/api/secrets";
  import type { AuthConfig, WorkspaceProfile } from "$lib/types";

  interface Props {
    workspaceId: string;
    profile: WorkspaceProfile;
    publicMcpEndpoint?: string;
  }

  let { workspaceId, profile, publicMcpEndpoint = "" }: Props = $props();

  let loading = $state(true);
  let secrets = $state<Record<string, string>>({});
  let secretsLoadSeq = 0;

  const auth = $derived(profile.auth);

  async function loadSecrets() {
    const seq = ++secretsLoadSeq;
    loading = true;
    try {
      const useShared = auth.use_shared_secrets ?? false;
      const fetchSecret = async (key: string, sharedKey: string) => {
        const value = useShared
          ? await getSharedSecret(sharedKey as Parameters<typeof getSharedSecret>[0])
          : await getSecret(workspaceId, key as Parameters<typeof getSecret>[1]);
        return value ?? "";
      };
      let next: Record<string, string>;
      if (auth.type === "oauth") {
        const clientId = useShared
          ? ((await getSharedSecret("oauth_client_id")) ?? "")
          : auth.oauth_client_id;
        next = {
          oauth_client_id: clientId,
          oauth_client_secret: await fetchSecret("oauth_client_secret", "oauth_client_secret"),
          oauth_password: await fetchSecret("oauth_password", "oauth_password"),
        };
      } else if (auth.type === "bearer") {
        next = {
          bearer_token: await fetchSecret("bearer_token", "bearer_token"),
        };
      } else {
        next = {};
      }
      if (seq !== secretsLoadSeq) return;
      secrets = next;
    } finally {
      if (seq === secretsLoadSeq) loading = false;
    }
  }

  $effect(() => {
    workspaceId;
    auth.type;
    auth.oauth_client_id;
    auth.use_shared_secrets;
    void loadSecrets();
    return () => {
      secretsLoadSeq += 1;
    };
  });
</script>

<article class="tx-card p-5">
  <div class="mb-4">
    <p class="tx-section-label">GPT 配置</p>
    <p class="mt-1 text-xs text-[var(--color-text-muted)]">
      复制以下内容到 ChatGPT → 设置 → 连接器 / MCP
    </p>
  </div>

  <div class="grid gap-3">
    <CopyFieldRow
      label="公网 MCP 地址"
      value={publicMcpEndpoint}
      hint="GPT 连接器里填这个 URL"
    />
    {#if auth.type === "oauth"}
      <CopyFieldRow label="OAuth Client ID" value={secrets.oauth_client_id ?? auth.oauth_client_id} {loading} />
      <CopyFieldRow
        label="OAuth Client Secret"
        value={secrets.oauth_client_secret ?? ""}
        {loading}
      />
      <CopyFieldRow
        label="授权口令"
        value={secrets.oauth_password ?? ""}
        hint="ChatGPT 首次授权时输入"
        {loading}
      />
    {:else if auth.type === "bearer"}
      <CopyFieldRow label="Bearer Token" value={secrets.bearer_token ?? ""} {loading} />
    {:else}
      <p class="text-xs text-[var(--color-text-muted)]">当前未启用认证，仅本机调试可用。</p>
    {/if}
  </div>
</article>
