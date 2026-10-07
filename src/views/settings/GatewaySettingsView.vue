<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { Activity, Globe2, Play, RefreshCw, Save, Square } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import TextField from "$src/components/ui/TextField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import {
  DEFAULT_GLOBAL_GATEWAY,
  checkGlobalGatewayHealth,
  getGlobalGatewayConfig,
  setGlobalGatewayConfig,
  type GatewayHealthItemDto,
  type GlobalGatewayConfigDto,
} from "$lib/api/global-gateway";
import { listFrpProfiles, type FrpProfileDto } from "$lib/api/settings";
import { getGlobalMcpOverview, startRuntime, stopRuntime, type GlobalMcpOverviewDto } from "$lib/api/workspaces";
import { globalMcpRuntimeState } from "$lib/stores/app";
import SaveBar from "$src/components/ui/SaveBar.vue";
import {useUnsavedChanges} from "$lib/unsaved-changes";
import {connectionHealthPresentation} from "$lib/ui-model";
import { showToast } from "$lib/stores/toast";

const config = reactive<GlobalGatewayConfigDto>({ ...DEFAULT_GLOBAL_GATEWAY });
const baseline = ref("");
const error=ref("");const loadError=ref("");const healthError=ref("");
const status = ref<GlobalMcpOverviewDto | null>(null);
const health = ref<GatewayHealthItemDto[]>([]);
const frpProfiles = ref<FrpProfileDto[]>([]);
const loading = ref(true);
const saving = ref(false);
const busy = ref(false);
const checking = ref(false);
let savePromise: Promise<void> | null = null;
const running = computed(() => status.value?.state === "running");
const dirty = computed(() => !!baseline.value && JSON.stringify(config) !== baseline.value);
const tunnelOptions = [
  { value: "none", label: "仅本地 / 外部反向代理" },
  { value: "frp", label: "FRP" },
  { value: "cloudflare", label: "Cloudflare Quick Tunnel" },
];
const frpOptions = computed(() => [{ value: "", label: "手动填写" }, ...frpProfiles.value.map((item) => ({ value: item.id, label: `${item.name} · ${item.server}:${item.serverPort}` }))]);

async function refresh() {
  loading.value = true;loadError.value="";
  try {
    const [nextConfig, nextStatus, nextFrp] = await Promise.all([getGlobalGatewayConfig(), getGlobalMcpOverview(), listFrpProfiles()]);
    if(!dirty.value){Object.assign(config,DEFAULT_GLOBAL_GATEWAY,nextConfig);baseline.value=JSON.stringify(config);}
    status.value = nextStatus;
    globalMcpRuntimeState.value = nextStatus.state;
    frpProfiles.value = nextFrp;
  } catch (e) { loadError.value=String(e); }
  finally { loading.value = false; }
}

function normalizePort(value: number) {
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error(`端口无效：${value}`);
  return port;
}

async function persistConfig() {
  if (savePromise) return savePromise;
  if (!dirty.value) return;
  const payload: GlobalGatewayConfigDto = {
    ...config,
    localPort: normalizePort(config.localPort),
    frpServerPort: normalizePort(config.frpServerPort),
    publicUrl: config.publicUrl.trim(),
    frpServer: config.frpServer.trim(),
    frpSubdomain: config.frpSubdomain.trim(),
  };
  saving.value = true;
  savePromise = (async () => {
    await setGlobalGatewayConfig(payload);
    Object.assign(config, payload);
    baseline.value = JSON.stringify(config);
  })();
  try {
    await savePromise;
  } finally {
    savePromise = null;
    saving.value = false;
  }
}

async function saveConfig() {
  if (!dirty.value && !savePromise) return;
  try {
    error.value="";await persistConfig();
    showToast("连接配置已保存；端口或公网连接变更会在重启 MCP 服务后应用。", { kind: "success", duration: 7000 });
    return true;
  } catch (e) {error.value=String(e);return false;}
}

async function start() {
  if (busy.value) return;
  busy.value = true;
  try { await persistConfig(); await startRuntime(); status.value = await getGlobalMcpOverview(); globalMcpRuntimeState.value = status.value.state; await runHealth(); showToast("Global MCP 已启动", { kind: "success" }); }
  catch (error) { showToast(String(error), { title: "启动失败", kind: "error" }); }
  finally { busy.value = false; }
}

async function stop() {
  if (busy.value) return;
  busy.value = true;
  try { await stopRuntime(); status.value = await getGlobalMcpOverview(); globalMcpRuntimeState.value = status.value.state; health.value = []; showToast("Global MCP 已停止", { kind: "info" }); }
  catch (error) { showToast(String(error), { title: "停止失败", kind: "error" }); }
  finally { busy.value = false; }
}

async function runHealth() {
  if (checking.value) return;
  checking.value = true;healthError.value="";
  try { health.value = await checkGlobalGatewayHealth(); status.value = await getGlobalMcpOverview(); globalMcpRuntimeState.value = status.value.state; }
  catch (e) { healthError.value=String(e);health.value=[]; }
  finally { checking.value = false; }
}

useUnsavedChanges("MCP 端点与公网连接",dirty,saveConfig);
onMounted(() => {
  void (async () => {
    await refresh();
    await runHealth();
  })();
});
</script>

<template>
  <div id="settings-gateway" class="ui-stack"><div v-if="loadError" class="ui-banner ui-banner--warning" role="alert"><span>{{loadError}}</span><BaseButton variant="secondary" @click="refresh">重试</BaseButton></div>
    <div class="grid gap-4 animate-fade-in-up delay-100">
      <GlassCard>
        <div class="flex flex-wrap items-start justify-between gap-4"><div class="flex items-center gap-3"><div class="grid h-11 w-11 place-items-center rounded-[var(--ui-radius-row)] bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]"><Globe2 :size="20" /></div><div><h2 class="text-base font-semibold">MCP 服务</h2><p class="mt-1 text-xs text-[var(--ui-text-muted)]">{{ running ? `${status?.sessionCount ?? 0} 个 会话 · ${status?.workspaceCount ?? 0} 个 工作区` : status ? "当前未运行" : "运行状态未知" }}</p></div></div><StatusPill :status="loadError ? 'unknown' : status?.state" /></div>
        <div class="settings-two-col mt-4"><div class="ui-inset p-3"><span class="text-xs text-[var(--ui-text-muted)]">本地端点</span><code class="mt-1 block break-all text-xs">{{ status?.localEndpoint ?? `http://127.0.0.1:${config.localPort}/mcp` }}</code></div><div class="ui-inset p-3"><span class="text-xs text-[var(--ui-text-muted)]">公网端点</span><code class="mt-1 block break-all text-xs">{{ status?.publicEndpoint || (config.publicUrl ? `${config.publicUrl.replace(/\/$/, '')}/mcp` : "尚未配置") }}</code></div></div>
        <div class="mt-4 flex flex-wrap gap-2"><BaseButton :busy="busy && !running" :disabled="running || loading" @click="start"><Play :size="13" />启动 MCP 服务</BaseButton><BaseButton variant="secondary" :busy="busy && running" :disabled="!running" @click="stop"><Square :size="12" />停止</BaseButton><BaseButton variant="ghost" :busy="checking" @click="runHealth"><Activity :size="13" />健康检查</BaseButton><BaseButton variant="ghost" :busy="loading" @click="refresh"><RefreshCw :size="13" />刷新</BaseButton></div>
      </GlassCard>

      <GlassCard>
        <div class="mb-4"><h2 class="text-base font-semibold">端点与公网连接</h2><p class="mt-1 text-xs text-[var(--ui-text-muted)]">所有工作区共享此端点；FRP 和 Cloudflare 用于提供公网访问。</p></div>
        <fieldset :disabled="saving||busy||loading||!!loadError" class="grid gap-3">
          <div class="ui-inset p-3"><ToggleSwitch v-model="config.enabled" label="启用公网连接" description="关闭时仍可正常使用本地 MCP 服务；只是不自动暴露公网入口。" /></div>
          <div class="settings-two-col"><TextField :model-value="String(config.localPort)" label="MCP 本地端口" :error="error && (!Number.isInteger(config.localPort) || config.localPort<1 || config.localPort>65535) ? error : undefined" type="number" @update:model-value="config.localPort = Number($event)" /><SelectField v-model="config.tunnelType" label="公网方式" :options="tunnelOptions" /></div>
          <template v-if="config.tunnelType === 'frp'">
            <SelectField v-model="config.frpProfileId" label="FRP 配置" :options="frpOptions" />
            <TextField v-model="config.frpSubdomain" label="子域名" placeholder="coding-tools" />
            <div v-if="!config.frpProfileId" class="settings-two-col"><TextField v-model="config.frpServer" label="FRP 服务器" placeholder="frp.example.com" /><TextField :model-value="String(config.frpServerPort)" label="端口" :error="error && (!Number.isInteger(config.frpServerPort) || config.frpServerPort<1 || config.frpServerPort>65535) ? error : undefined" type="number" @update:model-value="config.frpServerPort = Number($event)" /></div>
          </template>
          <div v-if="config.tunnelType === 'cloudflare'" class="ui-banner text-xs leading-5">Cloudflare 临时隧道会分配公网地址；固定域名可使用 FRP 或已有反向代理。</div>
          <TextField v-if="config.tunnelType === 'none'" v-model="config.publicUrl" label="外部公网 URL（可选）" placeholder="https://gateway.example.com" />
          <div v-if="config.tunnelType !== 'none'" class="ui-inset p-3"><ToggleSwitch v-model="config.useProxy" label="使用网络代理" description="使用网络与工具页的网络代理连接公网隧道。" /></div>
          <SaveBar :dirty="dirty" :busy="saving" :error="error" label="保存连接配置" @save="saveConfig"/>
        </fieldset>
      </GlassCard>

      <GlassCard id="settings-health"><h2 class="text-base font-semibold">健康检查</h2><p class="mt-1 text-xs text-[var(--ui-text-muted)]">进入页面时自动检查唯一 MCP 服务的本地与公网端点；也可随时手动重试。</p><p v-if="healthError" class="ui-danger mt-3" role="alert">检查失败：{{healthError}}。点击健康检查重试。</p><div class="mt-3 space-y-2"><div v-for="item in health" :key="item.label" class="ui-inset flex items-start justify-between gap-3 p-3"><div><strong class="text-xs">{{ item.label }}</strong><p class="mt-1 break-all text-xs leading-5 text-[var(--ui-text-muted)]">{{ item.detail }}</p></div><StatusPill :status="connectionHealthPresentation(item).status" :label="connectionHealthPresentation(item).label" /></div><p v-if="health.length === 0" class="py-4 text-center text-xs text-[var(--ui-text-muted)]">{{checking ? '正在检查连接…' : healthError ? '连接健康未知' : '暂无连接检查结果'}}</p></div></GlassCard>
    </div>
  </div>
</template>

<style scoped>
.settings-page {
  width: 100%;
  min-width: 0;
  box-sizing: border-box;
}

.settings-two-col {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: .75rem;
  min-width: 0;
}

@container app-main (max-width: 700px) {
  .settings-page {
    padding-inline: 16px;
  }

  .settings-two-col {
    grid-template-columns: 1fr;
  }
}
</style>
