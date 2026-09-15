<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import TextField from "$src/components/ui/TextField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import { listFrpProfiles, type FrpProfileDto } from "$lib/api/settings";
import { testTunnel } from "$lib/api/tunnel";
import { secretIsSet, setSecret, type WorkspaceSecretKey } from "$lib/api/secrets";
import { showToast } from "$lib/stores/toast";
import type { SaveTunnelOptions, TunnelFormConfig } from "$lib/workspace-page";

const props = defineProps<{ workspaceId: string; config: TunnelFormConfig }>();
const emit = defineEmits<{ save: [config: TunnelFormConfig, options?: SaveTunnelOptions] }>();

const draft = reactive<TunnelFormConfig>({ ...props.config });
const baseline = ref("");
const saving = ref(false);
const testing = ref(false);
const frpProfiles = ref<FrpProfileDto[]>([]);
const tokenDraft = ref("");
const tokenSaved = ref(false);
const legacyOpen = ref(false);

const tunnelOptions = [
  { value: "none", label: "未配置" },
  { value: "frp", label: "FRP" },
  { value: "cloudflare", label: "Cloudflare" },
];
const cloudflareOptions = [
  { value: "quick", label: "Quick Tunnel" },
  { value: "named", label: "Named Tunnel" },
];
const frpOptions = computed(() => [
  { value: "", label: "手动填写（旧版）" },
  ...frpProfiles.value.map((p) => ({ value: p.id, label: `${p.name} · ${p.server}:${p.serverPort}` })),
]);
const selectedProfile = computed(() => frpProfiles.value.find((profile) => profile.id === draft.frp_profile_id) ?? null);
const useGlobalProfile = computed(() => Boolean(draft.frp_profile_id && selectedProfile.value));
const showFrp = computed(() => !draft.use_global_gateway && draft.type === "frp");
const showCloudflare = computed(() => !draft.use_global_gateway && draft.type === "cloudflare");
const showCloudflareToken = computed(() => showCloudflare.value && draft.cloudflare_mode === "named");
const showToken = computed(() => (showFrp.value && !useGlobalProfile.value) || showCloudflareToken.value);
const canTest = computed(() => !draft.use_global_gateway && ["frp", "cloudflare"].includes(draft.type));
const secretKey = computed<WorkspaceSecretKey>(() => draft.type === "frp" ? "frp_token" : "cloudflare_token");
const dirty = computed(() => JSON.stringify(draft) !== baseline.value || tokenDraft.value.trim().length > 0);

function syncConfig() {
  Object.assign(draft, props.config, {
    frp_profile_id: props.config.frp_profile_id ?? "",
    use_proxy: props.config.use_proxy ?? true,
    use_global_gateway: props.config.use_global_gateway ?? false,
  });
  baseline.value = JSON.stringify(draft);
}

watch(() => props.config, syncConfig, { immediate: true, deep: true });
watch([() => props.workspaceId, secretKey], async () => {
  tokenDraft.value = "";
  try { tokenSaved.value = await secretIsSet(props.workspaceId, secretKey.value); }
  catch { tokenSaved.value = false; }
}, { immediate: true });

onMounted(async () => {
  try { frpProfiles.value = await listFrpProfiles(); } catch { frpProfiles.value = []; }
});

function normalized(): TunnelFormConfig {
  const port = Number(draft.frp_server_port);
  if (!Number.isFinite(port) || port < 1 || port > 65535) throw new Error("端口无效：请填写 1-65535 之间的整数");
  return { ...draft, frp_server_port: Math.trunc(port) };
}

async function persist(options?: SaveTunnelOptions) {
  if (showToken.value && tokenDraft.value.trim()) {
    await setSecret(props.workspaceId, secretKey.value, tokenDraft.value.trim());
    tokenSaved.value = true;
    tokenDraft.value = "";
  }
  const payload = normalized();
  emit("save", payload, options);
  Object.assign(draft, payload);
  baseline.value = JSON.stringify(payload);
}

async function save() {
  if (saving.value || !dirty.value) return;
  saving.value = true;
  try {
    await persist();
    showToast("隧道配置已保存。", { title: "保存成功", kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "保存失败", kind: "error", duration: 8000 });
  } finally { saving.value = false; }
}

async function testConnection() {
  if (!canTest.value || testing.value) return;
  testing.value = true;
  try {
    if (dirty.value) await persist({ skipTunnelRestart: true, skipServicePrompt: true });
    const result = await testTunnel(props.workspaceId);
    if (result.publicUrl && draft.cloudflare_mode === "quick") draft.public_url = result.publicUrl;
    showToast(result.publicUrl ? `${result.message}\n${result.publicUrl}` : result.message, {
      title: result.success ? "测试成功" : "测试未完成",
      kind: result.success ? "success" : "warning",
      duration: 8000,
    });
  } catch (error) {
    showToast(String(error), { title: "测试失败", kind: "error", duration: 8000 });
  } finally { testing.value = false; }
}
</script>

<template>
  <form class="grid gap-4" @submit.prevent="save">
    <div class="ios-glass ios-inset-surface p-3"><ToggleSwitch v-model="draft.use_global_gateway" label="使用全局共享公网入口" description="通过 /w/<workspace-id> 前缀转发，不再单独启动公网 Tunnel。" /></div>
    <template v-if="!draft.use_global_gateway">
      <div class="grid gap-3 md:grid-cols-2">
        <SelectField v-model="draft.type" label="隧道类型" :options="tunnelOptions" />
        <div v-if="canTest" class="ios-glass ios-inset-surface p-3"><ToggleSwitch v-model="draft.use_proxy" label="使用网络代理" description="通过通用设置中的全局代理连接隧道" /></div>
      </div>

      <template v-if="showFrp">
        <SelectField v-model="draft.frp_profile_id" label="FRP 配置" :options="frpOptions" hint="推荐使用设置页中的全局 FRP Profile。" />
        <div v-if="useGlobalProfile && selectedProfile" class="rounded-2xl bg-white/35 p-3 text-xs dark:bg-white/4">
          <strong>{{ selectedProfile.name }}</strong><span class="ml-2 font-mono">{{ selectedProfile.server }}:{{ selectedProfile.serverPort }}</span>
          <span class="ml-2 text-[var(--text-muted)]">Token {{ selectedProfile.hasToken ? "已配置" : "未配置" }}</span>
        </div>
        <TextField v-model="draft.frp_subdomain" label="子域名" placeholder="my-mcp" />
        <button v-if="!useGlobalProfile" type="button" class="text-left text-xs text-[var(--primary)]" @click="legacyOpen = !legacyOpen">{{ legacyOpen ? "收起" : "展开" }}手动 FRP 配置</button>
        <div v-if="!useGlobalProfile && legacyOpen" class="grid gap-3 md:grid-cols-2">
          <TextField v-model="draft.frp_server" label="FRP 服务器" placeholder="example.com" />
          <TextField :model-value="String(draft.frp_server_port)" label="FRP 端口" type="number" @update:model-value="draft.frp_server_port = Number($event)" />
        </div>
      </template>

      <SelectField v-if="showCloudflare" v-model="draft.cloudflare_mode" label="Cloudflare 模式" :options="cloudflareOptions" />

      <TextField
        v-if="showToken"
        v-model="tokenDraft"
        :label="draft.type === 'frp' ? 'FRP Token（可选）' : 'Cloudflare Tunnel Token'"
        type="password"
        :placeholder="tokenSaved ? '已保存（输入新值可更新）' : '粘贴 Token'"
      />
      <TextField v-model="draft.public_url" label="公网 URL" type="url" placeholder="https://..." />
    </template>
    <div class="flex justify-end gap-2">
      <BaseButton v-if="canTest" variant="ghost" :busy="testing" :disabled="saving" @click="testConnection">测试连接</BaseButton>
      <BaseButton variant="primary" :busy="saving" :disabled="testing || !dirty" @click="save">保存配置</BaseButton>
    </div>
  </form>
</template>
