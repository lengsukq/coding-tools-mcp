<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { RefreshCw, Save } from "@lucide/vue";
import BaseButton from "../../components/ui/BaseButton.vue";
import GlassCard from "../../components/ui/GlassCard.vue";
import SecretField from "../../components/SecretField.vue";
import SettingsPageHeader from "../../components/settings/SettingsPageHeader.vue";
import { getSharedSecret, regenerateSharedSecret, setSharedSecret, type SharedSecretKey } from "$lib/api/secrets";
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

async function load() {
  loading.value = true;
  try {
    for (const item of keys) {
      const value = await getSharedSecret(item.key) ?? "";
      secrets[item.key] = value;
      originals[item.key] = value;
    }
  } catch (error) { showToast(String(error), { title: "加载共享密钥失败", kind: "error" }); }
  finally { loading.value = false; }
}
async function regenerate(key: SharedSecretKey) {
  regenerating.value = key;
  try { secrets[key] = await regenerateSharedSecret(key); showToast("已生成新密钥，保存后旧密钥将失效。", { kind: "warning", duration: 7000 }); }
  catch (error) { showToast(String(error), { title: "重新生成失败", kind: "error" }); }
  finally { regenerating.value = null; }
}
async function save() {
  saving.value = true;
  try {
    for (const item of keys) {
      if (secrets[item.key] !== originals[item.key]) {
        await setSharedSecret(item.key, secrets[item.key] ?? "");
        originals[item.key] = secrets[item.key] ?? "";
      }
    }
    showToast("共享密钥已保存", { kind: "success" });
  } catch (error) { showToast(String(error), { title: "保存失败", kind: "error" }); }
  finally { saving.value = false; }
}
onMounted(() => { void load(); });
</script>

<template>
  <div class="mx-auto max-w-[1180px] px-8 py-7">
    <SettingsPageHeader title="Global MCP 密钥" description="唯一 MCP Endpoint 的认证凭据。所有 Workspace 共用这一组连接认证，项目数据与执行策略仍彼此隔离。" />
    <GlassCard>
      <p v-if="loading" class="py-8 text-center text-xs text-[var(--text-muted)]">加载中…</p>
      <div v-else class="grid gap-4">
        <div v-for="item in keys" :key="item.key" class="grid grid-cols-[minmax(0,1fr)_auto] items-end gap-2">
          <label class="block min-w-0">
            <span class="mb-1.5 block text-xs font-semibold text-[var(--text-secondary)]">{{ item.label }}</span>
            <SecretField v-model="secrets[item.key]" />
            <span class="mt-1.5 block text-[11px] leading-4 text-[var(--text-muted)]">{{ item.hint }} · 可点击眼睛显示或复制</span>
          </label>
          <BaseButton variant="secondary" :busy="regenerating === item.key" @click="regenerate(item.key)"><RefreshCw :size="13" />重新生成</BaseButton>
        </div>
        <div class="flex justify-end border-t border-black/[.05] pt-4 dark:border-white/[.06]"><BaseButton :busy="saving" @click="save"><Save :size="14" />保存更改</BaseButton></div>
      </div>
    </GlassCard>
  </div>
</template>
