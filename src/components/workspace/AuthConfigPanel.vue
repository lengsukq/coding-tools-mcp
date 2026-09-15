<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { Save } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import SecretField from "../SecretField.vue";
import TextField from "../ui/TextField.vue";
import ToggleSwitch from "../ui/ToggleSwitch.vue";
import {
  getSharedSecret,
  getWorkspaceSecret,
  regenerateSharedSecret,
  regenerateWorkspaceSecret,
  setSharedSecret,
  type SharedSecretKey,
  type WorkspaceSecretKey,
} from "$lib/api/secrets";
import { showToast } from "$lib/stores/toast";
import type { AuthConfig } from "$lib/types";

const props = defineProps<{ workspaceId: string; auth: AuthConfig }>();
const emit = defineEmits<{ save: [auth: AuthConfig] }>();
const draft = reactive<AuthConfig>({ ...props.auth, use_shared_secrets: !!props.auth.use_shared_secrets });
const secrets = reactive<Partial<Record<WorkspaceSecretKey, string>>>({});
const saving = ref(false);
const regenerating = ref<WorkspaceSecretKey | null>(null);
const showOAuth = computed(() => draft.type === "oauth");
const showBearer = computed(() => draft.type === "bearer");

watch(() => props.auth, (value) => Object.assign(draft, value), { deep: true });

async function loadSecrets() {
  const shared = !!draft.use_shared_secrets;
  if (draft.type === "oauth" && shared) draft.oauth_client_id = await getSharedSecret("oauth_client_id") ?? "";
  const keys: WorkspaceSecretKey[] = draft.type === "oauth" ? ["oauth_client_secret", "oauth_password"] : draft.type === "bearer" ? ["bearer_token"] : [];
  for (const key of keys) {
    secrets[key] = shared ? await getSharedSecret(key as SharedSecretKey) ?? "" : await getWorkspaceSecret(props.workspaceId, key) ?? "";
  }
}

watch(() => [draft.type, draft.use_shared_secrets], () => { void loadSecrets(); });

async function regenerate(key: WorkspaceSecretKey) {
  regenerating.value = key;
  try {
    secrets[key] = draft.use_shared_secrets ? await regenerateSharedSecret(key as SharedSecretKey) : await regenerateWorkspaceSecret(props.workspaceId, key);
    showToast("密钥已重新生成，请同步更新外部客户端。", { kind: "warning", duration: 7000 });
  } catch (error) { showToast(String(error), { title: "重新生成失败", kind: "error" }); }
  finally { regenerating.value = null; }
}

async function save() {
  saving.value = true;
  try {
    if (draft.type === "oauth" && draft.use_shared_secrets) {
      if (!draft.oauth_client_id.trim()) throw new Error("OAuth Client ID 不能为空");
      await setSharedSecret("oauth_client_id", draft.oauth_client_id.trim());
    }
    emit("save", { ...draft });
    showToast("认证配置已保存", { kind: "success" });
  } catch (error) { showToast(String(error), { title: "保存认证失败", kind: "error" }); }
  finally { saving.value = false; }
}

onMounted(() => { void loadSecrets(); });
</script>

<template>
  <div class="grid gap-4">
    <label class="block"><span class="mb-1.5 block text-xs font-semibold text-[var(--text-secondary)]">认证类型</span><select v-model="draft.type" class="h-10 w-full rounded-xl border border-white/70 bg-white/60 px-3 text-sm outline-none dark:border-white/10 dark:bg-[#232329]"><option value="oauth">OAuth</option><option value="bearer">Bearer Token</option><option value="noauth">不启用认证</option></select></label>
    <ToggleSwitch v-model="draft.use_shared_secrets" label="使用全局共享密钥" description="多个工作区复用同一套认证凭据。" />
    <template v-if="showOAuth">
      <TextField v-model="draft.oauth_client_id" label="OAuth Client ID" :disabled="false" />
      <div><p class="mb-1.5 text-xs font-semibold text-[var(--text-secondary)]">OAuth Client Secret</p><SecretField :value="secrets.oauth_client_secret ?? ''" :allow-regenerate="true" :busy="regenerating === 'oauth_client_secret'" @regenerate="regenerate('oauth_client_secret')" /></div>
      <div><p class="mb-1.5 text-xs font-semibold text-[var(--text-secondary)]">授权口令</p><SecretField :value="secrets.oauth_password ?? ''" :allow-regenerate="true" :busy="regenerating === 'oauth_password'" @regenerate="regenerate('oauth_password')" /></div>
    </template>
    <div v-if="showBearer"><p class="mb-1.5 text-xs font-semibold text-[var(--text-secondary)]">Bearer Token</p><SecretField :value="secrets.bearer_token ?? ''" :allow-regenerate="true" :busy="regenerating === 'bearer_token'" @regenerate="regenerate('bearer_token')" /></div>
    <div class="flex justify-end"><BaseButton :busy="saving" @click="save"><Save :size="14" />保存认证</BaseButton></div>
  </div>
</template>
