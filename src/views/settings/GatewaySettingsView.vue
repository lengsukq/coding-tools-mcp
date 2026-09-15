<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { Activity, Globe2, Play, RefreshCw, Save, Square } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import TextField from "$src/components/ui/TextField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import ConnectionSettingsNav from "$src/components/settings/ConnectionSettingsNav.vue";
import SettingsPageHeader from "$src/components/settings/SettingsPageHeader.vue";
import {
  DEFAULT_GLOBAL_GATEWAY,
  checkGlobalGatewayHealth,
  getGlobalGatewayConfig,
  getGlobalGatewayStatus,
  setGlobalGatewayConfig,
  startGlobalGateway,
  stopGlobalGateway,
  type GatewayHealthItemDto,
  type GlobalGatewayConfigDto,
  type GlobalGatewayStatusDto,
} from "$lib/api/global-gateway";
import { listFrpProfiles, type FrpProfileDto } from "$lib/api/settings";
import { listWorkspaces } from "$lib/api/workspaces";
import type { WorkspaceProfile } from "$lib/types";
import { showToast } from "$lib/stores/toast";

const config = reactive<GlobalGatewayConfigDto>({ ...DEFAULT_GLOBAL_GATEWAY });
const baseline = ref("");
const status = ref<GlobalGatewayStatusDto | null>(null);
const health = ref<GatewayHealthItemDto[]>([]);
const workspaces = ref<WorkspaceProfile[]>([]);
const frpProfiles = ref<FrpProfileDto[]>([]);
const loading = ref(true);
const saving = ref(false);
const busy = ref(false);
const checking = ref(false);
const running = computed(() => status.value?.state === "running");
const dirty = computed(() => JSON.stringify(config) !== baseline.value);
const tunnelOptions = [
  { value: "none", label: "仅本地 / 外部反向代理" },
  { value: "frp", label: "FRP" },
  { value: "cloudflare", label: "Cloudflare Quick Tunnel" },
];
const frpOptions = computed(() => [{ value: "", label: "手动填写" }, ...frpProfiles.value.map((item) => ({ value: item.id, label: `${item.name} · ${item.server}:${item.serverPort}` }))]);
const routes = computed(() => workspaces.value.filter((item) => item.tunnel.use_global_gateway).map((item) => ({ workspace: item.name, path: `/w/${item.id}/mcp` })));

async function refresh() {
  loading.value = true;
  try {
    const [nextConfig, nextStatus, nextWorkspaces, nextFrp] = await Promise.all([getGlobalGatewayConfig(), getGlobalGatewayStatus(), listWorkspaces(), listFrpProfiles()]);
    Object.assign(config, DEFAULT_GLOBAL_GATEWAY, nextConfig);
    baseline.value = JSON.stringify(config);
    status.value = nextStatus;
    workspaces.value = nextWorkspaces;
    frpProfiles.value = nextFrp;
  } catch (error) { showToast(String(error), { title: "加载 Global Gateway 失败", kind: "error", duration: 8000 }); }
  finally { loading.value = false; }
}

function normalizePort(value: number, fallback: number) {
  const port = Number(value || fallback);
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error(`端口无效：${value}`);
  return port;
}

async function saveConfig() {
  if (!dirty.value || saving.value) return;
  saving.value = true;
  try {
    const payload: GlobalGatewayConfigDto = {
      ...config,
      localPort: normalizePort(config.localPort, 28765),
      frpServerPort: normalizePort(config.frpServerPort, 7000),
      publicUrl: config.publicUrl.trim(),
      frpServer: config.frpServer.trim(),
      frpSubdomain: config.frpSubdomain.trim(),
    };
    await setGlobalGatewayConfig(payload);
    Object.assign(config, payload);
    baseline.value = JSON.stringify(config);
    showToast("Global Gateway 配置已保存，运行中的 Gateway 需重启后应用。", { kind: "success", duration: 7000 });
  } catch (error) { showToast(String(error), { title: "保存失败", kind: "error" }); }
  finally { saving.value = false; }
}

async function start() {
  if (busy.value) return;
  busy.value = true;
  try { if (dirty.value) await saveConfig(); status.value = await startGlobalGateway(); await runHealth(); showToast("Global Gateway 已启动", { kind: "success" }); }
  catch (error) { showToast(String(error), { title: "启动失败", kind: "error" }); }
  finally { busy.value = false; }
}

async function stop() {
  if (busy.value) return;
  busy.value = true;
  try { await stopGlobalGateway(); status.value = await getGlobalGatewayStatus(); health.value = []; showToast("Global Gateway 已停止", { kind: "info" }); }
  catch (error) { showToast(String(error), { title: "停止失败", kind: "error" }); }
  finally { busy.value = false; }
}

async function runHealth() {
  if (checking.value) return;
  checking.value = true;
  try { health.value = await checkGlobalGatewayHealth(); status.value = await getGlobalGatewayStatus(); }
  catch (error) { showToast(String(error), { title: "健康检查失败", kind: "error" }); }
  finally { checking.value = false; }
}

function fullRoute(path: string) {
  const base = status.value?.publicUrl?.trim().replace(/\/$/, "");
  return base ? `${base}${path}` : path;
}

onMounted(() => void refresh());
</script>

<template>
  <div class="mx-auto max-w-[1180px] px-8 py-7">
    <SettingsPageHeader title="Global Gateway" description="用一个共享公网入口承载多个 Workspace，通过 /w/<workspace-id> 前缀隔离 MCP 路由。" />
    <ConnectionSettingsNav />
    <div class="grid gap-4">
      <GlassCard>
        <div class="flex flex-wrap items-start justify-between gap-4"><div class="flex items-center gap-3"><div class="grid h-11 w-11 place-items-center rounded-2xl bg-[#0a84ff]/13 text-[#0a84ff]"><Globe2 :size="20" /></div><div><h2 class="text-sm font-semibold">运行状态</h2><p class="mt-1 text-xs text-[var(--text-muted)]">{{ status?.detail ?? "正在读取状态…" }}</p></div></div><StatusPill :status="running ? 'running' : 'stopped'" /></div>
        <div class="mt-4 grid gap-3 md:grid-cols-2"><div class="ios-glass rounded-2xl p-3"><span class="text-[10px] text-[var(--text-muted)]">本地入口</span><code class="mt-1 block break-all text-xs">{{ status?.localUrl ?? `http://127.0.0.1:${config.localPort}` }}</code></div><div class="ios-glass rounded-2xl p-3"><span class="text-[10px] text-[var(--text-muted)]">公网入口</span><code class="mt-1 block break-all text-xs">{{ status?.publicUrl || config.publicUrl || "尚未获取" }}</code></div></div>
        <div class="mt-4 flex flex-wrap gap-2"><BaseButton :busy="busy && !running" :disabled="running || loading" @click="start"><Play :size="13" />启动 Gateway</BaseButton><BaseButton variant="secondary" :busy="busy && running" :disabled="!running" @click="stop"><Square :size="12" />停止</BaseButton><BaseButton variant="ghost" :busy="checking" @click="runHealth"><Activity :size="13" />健康检查</BaseButton><BaseButton variant="ghost" :busy="loading" @click="refresh"><RefreshCw :size="13" />刷新</BaseButton></div>
      </GlassCard>

      <GlassCard>
        <div class="mb-4"><h2 class="text-sm font-semibold">Gateway 配置</h2><p class="mt-1 text-xs text-[var(--text-muted)]">Workspace 勾选共享入口后，启动 MCP 会自动确保 Gateway 可用。</p></div>
        <div class="grid gap-3">
          <div class="ios-glass rounded-2xl p-3"><ToggleSwitch v-model="config.enabled" label="启用 Global Gateway" description="关闭后 Workspace 无法通过共享 Gateway 暴露公网入口。" /></div>
          <div class="grid gap-3 md:grid-cols-2"><TextField :model-value="String(config.localPort)" label="本地监听端口" type="number" @update:model-value="config.localPort = Number($event)" /><SelectField v-model="config.tunnelType" label="公网方式" :options="tunnelOptions" /></div>
          <template v-if="config.tunnelType === 'frp'">
            <SelectField v-model="config.frpProfileId" label="FRP 配置" :options="frpOptions" />
            <TextField v-model="config.frpSubdomain" label="子域名" placeholder="coding-tools" />
            <div v-if="!config.frpProfileId" class="grid gap-3 md:grid-cols-2"><TextField v-model="config.frpServer" label="FRP 服务器" placeholder="frp.example.com" /><TextField :model-value="String(config.frpServerPort)" label="端口" type="number" @update:model-value="config.frpServerPort = Number($event)" /></div>
          </template>
          <div v-if="config.tunnelType === 'cloudflare'" class="rounded-2xl bg-[#64d2ff]/8 p-3 text-xs leading-5 text-[var(--text-secondary)]">Global Gateway 使用 Cloudflare Quick Tunnel；固定域名建议使用 FRP 或外部反代。</div>
          <TextField v-if="config.tunnelType === 'none'" v-model="config.publicUrl" label="外部公网 URL（可选）" placeholder="https://gateway.example.com" />
          <div v-if="config.tunnelType !== 'none'" class="ios-glass rounded-2xl p-3"><ToggleSwitch v-model="config.useProxy" label="使用全局代理" description="使用通用设置里的网络代理连接公网 Tunnel。" /></div>
          <div class="flex justify-end"><BaseButton :busy="saving" :disabled="!dirty" @click="saveConfig"><Save :size="14" />保存 Gateway 配置</BaseButton></div>
        </div>
      </GlassCard>

      <div class="grid gap-4 lg:grid-cols-2">
        <GlassCard><h2 class="text-sm font-semibold">Workspace Routes</h2><p class="mt-1 text-xs text-[var(--text-muted)]">只显示已启用共享 Gateway 的 Workspace。</p><div class="mt-3 space-y-2"><div v-for="routeItem in routes" :key="routeItem.path" class="ios-glass rounded-2xl p-3"><strong class="block text-xs">{{ routeItem.workspace }}</strong><code class="mt-1 block break-all text-[10px] text-[var(--text-secondary)]">{{ fullRoute(routeItem.path) }}</code></div><p v-if="routes.length === 0" class="py-4 text-center text-xs text-[var(--text-muted)]">当前没有 Workspace 使用 Global Gateway。</p></div></GlassCard>
        <GlassCard><h2 class="text-sm font-semibold">健康检查</h2><div class="mt-3 space-y-2"><div v-for="item in health" :key="item.label" class="ios-glass flex items-start justify-between gap-3 rounded-2xl p-3"><div><strong class="text-xs">{{ item.label }}</strong><p class="mt-1 break-all text-[10px] leading-4 text-[var(--text-muted)]">{{ item.detail }}</p></div><StatusPill :status="item.ok ? 'success' : 'error'" :label="item.ok ? '正常' : '失败'" /></div><p v-if="health.length === 0" class="py-4 text-center text-xs text-[var(--text-muted)]">尚未执行健康检查。</p></div></GlassCard>
      </div>
    </div>
  </div>
</template>
