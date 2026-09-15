<script setup lang="ts">
import { ref, watch } from "vue";
import CopyField from "$src/components/ui/CopyField.vue";
import { getSecret, getSharedSecret } from "$lib/api/secrets";
import type { WorkspaceProfile } from "$lib/types";

const props = withDefaults(defineProps<{
  workspaceId: string;
  profile: WorkspaceProfile;
  publicMcpEndpoint?: string;
}>(), { publicMcpEndpoint: "" });

const loading = ref(true);
const secrets = ref<Record<string, string>>({});
let loadSeq = 0;

async function loadSecrets() {
  const seq = ++loadSeq;
  loading.value = true;
  try {
    const auth = props.profile.auth;
    const useShared = auth.use_shared_secrets ?? false;
    const fetchSecret = async (key: Parameters<typeof getSecret>[1], sharedKey: Parameters<typeof getSharedSecret>[0]) => {
      const value = useShared ? await getSharedSecret(sharedKey) : await getSecret(props.workspaceId, key);
      return value ?? "";
    };
    let next: Record<string, string> = {};
    if (auth.type === "oauth") {
      const clientId = useShared ? ((await getSharedSecret("oauth_client_id")) ?? "") : auth.oauth_client_id;
      next = {
        oauth_client_id: clientId,
        oauth_client_secret: await fetchSecret("oauth_client_secret", "oauth_client_secret"),
        oauth_password: await fetchSecret("oauth_password", "oauth_password"),
      };
    } else if (auth.type === "bearer") {
      next = { bearer_token: await fetchSecret("bearer_token", "bearer_token") };
    }
    if (seq === loadSeq) secrets.value = next;
  } finally {
    if (seq === loadSeq) loading.value = false;
  }
}

watch(
  () => [props.workspaceId, props.profile.auth.type, props.profile.auth.oauth_client_id, props.profile.auth.use_shared_secrets],
  () => void loadSecrets(),
  { immediate: true },
);
</script>

<template>
  <article class="ios-glass ios-card-surface p-4">
    <div class="mb-3">
      <p class="text-xs font-semibold text-[var(--text-main)]">GPT 配置</p>
      <p class="mt-1 text-[11px] text-[var(--text-muted)]">复制到 ChatGPT → 设置 → 连接器 / MCP</p>
    </div>
    <div class="grid gap-2.5 md:grid-cols-2">
      <CopyField label="公网 MCP 地址" :value="publicMcpEndpoint" hint="GPT 连接器里填这个 URL" />
      <template v-if="profile.auth.type === 'oauth'">
        <CopyField label="OAuth Client ID" :value="secrets.oauth_client_id ?? profile.auth.oauth_client_id" :loading="loading" />
        <CopyField label="OAuth Client Secret" :value="secrets.oauth_client_secret ?? ''" :loading="loading" />
        <CopyField label="授权口令" :value="secrets.oauth_password ?? ''" :loading="loading" hint="ChatGPT 首次授权时输入" />
      </template>
      <CopyField v-else-if="profile.auth.type === 'bearer'" label="Bearer Token" :value="secrets.bearer_token ?? ''" :loading="loading" />
      <p v-else class="text-xs text-[var(--text-muted)]">当前未启用认证，仅建议本机调试。</p>
    </div>
  </article>
</template>
