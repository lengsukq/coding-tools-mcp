<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { RefreshCw, Save } from "@lucide/vue";
import BaseButton from "../../components/ui/BaseButton.vue";
import ConfirmDialog from "../../components/ui/ConfirmDialog.vue";
import GlassCard from "../../components/ui/GlassCard.vue";
import SecretField from "../../components/SecretField.vue";
import { getSharedSecret, regenerateSharedSecret, setSharedSecrets, type SharedSecretKey } from "$lib/api/secrets";
import SaveBar from "../../components/ui/SaveBar.vue";
import {useUnsavedChanges} from "$lib/unsaved-changes";
import { showToast } from "$lib/stores/toast";

const keys: Array<{ key: SharedSecretKey; label: string; hint: string }> = [
  { key: "oauth_client_id", label: "MCP OAuth Client ID", hint: "OAuth 客户端公开 ID" },
  { key: "bearer_token", label: "MCP Bearer Token", hint: "Bearer 认证模式使用" },
  { key: "oauth_client_secret", label: "MCP OAuth Client Secret", hint: "OAuth 客户端密钥" },
  { key: "oauth_password", label: "MCP 授权口令", hint: "首次授权时使用" },
  { key: "oauth_token_secret", label: "MCP Token Secret", hint: "OAuth Token 签名密钥" },
];
const secrets = reactive<Partial<Record<SharedSecretKey, string>>>({});
const originals = reactive<Partial<Record<SharedSecretKey, string>>>({});
const loading = ref(true);
const saving = ref(false);
const regenerating = ref<SharedSecretKey | null>(null);
const saveConfirmOpen = ref(false);
const error=ref("");
const loaded=ref(false);
const changedKeys = computed(() => keys.filter((item) => secrets[item.key] !== originals[item.key]));

async function load() {
  loading.value = true;error.value="";
  try {
    for (const item of keys) {
      const value = await getSharedSecret(item.key) ?? "";
      secrets[item.key] = value;
      originals[item.key] = value;
    }
    loaded.value=true;
  } catch (e) { error.value=String(e); }
  finally { loading.value = false; }
}
async function regenerate(key: SharedSecretKey) {
  regenerating.value = key;
  try { secrets[key] = await regenerateSharedSecret(key); showToast("已生成新密钥草稿；点击保存后旧密钥才会失效。", { kind: "warning", duration: 7000 }); }
  catch (error) { showToast(String(error), { title: "重新生成失败", kind: "error" }); }
  finally { regenerating.value = null; }
}
async function save() {
  if (changedKeys.value.length === 0) {
    saveConfirmOpen.value = false;
    showToast("没有需要保存的密钥更改。", { kind: "info" });
    return;
  }
  if(saving.value || !loaded.value) return false;
  error.value="";saving.value = true;
  try {
    const updates = changedKeys.value
      .map((item) => ({ key: item.key, value: secrets[item.key] ?? "" }));
    if (updates.length > 0) {
      await setSharedSecrets(updates);
      for (const update of updates) originals[update.key] = update.value;
    }
    saveConfirmOpen.value = false;
    showToast("共享密钥已保存", { kind: "success" });return true;
  } catch (e) { error.value=String(e);return false; }
  finally { saving.value = false; }
}
useUnsavedChanges("认证密钥（替换后客户端可能需要重新连接）",computed(()=>loaded.value && changedKeys.value.length>0),save);
onMounted(() => { void load(); });
</script>

<template>
  <div class="ui-stack">
    <GlassCard class="animate-fade-in-up delay-100">
      <p v-if="loading" class="py-8 text-center text-xs text-[var(--text-muted)]">加载中…</p>
      <div v-else-if="!loaded" class="ui-banner ui-banner--warning" role="alert"><span>{{error}}</span><BaseButton variant="secondary" @click="load">重试</BaseButton></div>
      <div v-else class="grid gap-4">
        <div v-for="item in keys" :key="item.key" class="key-setting-row">
          <label class="block min-w-0">
            <span class="mb-1.5 block text-xs font-medium text-[var(--ui-text-secondary)]">{{ item.label }}</span>
            <SecretField v-model="secrets[item.key]" :disabled="saving" />
            <span class="mt-1.5 block text-xs leading-4 text-[var(--ui-text-muted)]">{{ item.hint }} · 可点击眼睛显示或复制</span>
          </label>
          <BaseButton variant="secondary" :busy="regenerating === item.key" @click="regenerate(item.key)"><RefreshCw :size="13" />重新生成</BaseButton>
        </div>
        <SaveBar :dirty="changedKeys.length>0" :busy="saving" :error="error" @save="saveConfirmOpen=true"/>
      </div>
    </GlassCard>
  </div>

  <ConfirmDialog
    :open="saveConfirmOpen"
    title="保存 Global MCP 密钥"
    message="确定替换当前 Global MCP 认证凭据？部分已连接客户端可能需要重新认证或重新连接。"
    :detail="changedKeys.map((item) => item.label).join('、')"
    confirm-text="确认保存"
    severity="danger"
    :busy="saving"
    @confirm="save"
    @cancel="saveConfirmOpen = false"
  />
</template>

<style scoped>
.settings-page {
  width: 100%;
  min-width: 0;
  box-sizing: border-box;
}

.key-setting-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: end;
  gap: .5rem;
  min-width: 0;
}

@container app-main (max-width: 620px) {
  .settings-page {
    padding-inline: 16px;
  }

  .key-setting-row {
    grid-template-columns: 1fr;
    align-items: stretch;
  }

  .key-setting-row > :last-child {
    justify-self: start;
  }
}
</style>
