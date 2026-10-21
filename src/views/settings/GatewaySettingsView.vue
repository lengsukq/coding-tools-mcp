<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { Activity, Check, Copy, Globe2, Play, RefreshCw, RotateCw, Square } from "@lucide/vue";
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
  getGlobalGatewayStatus,
  restartGlobalCloudflareTunnel,
  setGlobalGatewayConfig,
  type GatewayHealthItemDto,
  type GlobalGatewayConfigDto,
  type GlobalGatewayStatusDto,
} from "$lib/api/global-gateway";
import { listFrpProfiles, type FrpProfileDto } from "$lib/api/settings";
import { getGlobalMcpOverview, startRuntime, stopRuntime, type GlobalMcpOverviewDto } from "$lib/api/workspaces";
import { globalMcpRuntimeState } from "$lib/stores/app";
import { recordConnectionHealth } from "$lib/stores/health";
import SaveBar from "$src/components/ui/SaveBar.vue";
import { useUnsavedChanges } from "$lib/unsaved-changes";
import { connectionHealthPresentation } from "$lib/ui-model";
import { showToast } from "$lib/stores/toast";
const config = reactive<GlobalGatewayConfigDto>({ ...DEFAULT_GLOBAL_GATEWAY });
const baseline = ref("");
const error = ref("");
const loadError = ref("");
const healthError = ref("");
const tunnelActionError = ref("");
const status = ref<GlobalMcpOverviewDto | null>(null);
const gatewayStatus = ref<GlobalGatewayStatusDto | null>(null);
const health = ref<GatewayHealthItemDto[]>([]);
const frpProfiles = ref<FrpProfileDto[]>([]);
const loading = ref(true);
const saving = ref(false);
const busy = ref(false);
const checking = ref(false);
const healthCheckFailed = ref(false);
const lastCheckedAt = ref<number | null>(null);
const copiedEndpoint = ref<"local" | "public" | null>(null);
let savePromise: Promise<void> | null = null;
let copyTimer: number | null = null;

const running = computed(() => status.value?.state === "running");
const canForceRestartCloudflare = computed(() => config.enabled && config.tunnelType === "cloudflare" && config.cloudflareMode === "quick");
const dirty = computed(() => !!baseline.value && JSON.stringify(config) !== baseline.value);
const mcpState = computed(() => status.value?.state ?? "unknown");
const mcpDetail = computed(() => {
  if (running.value) return `${status.value?.sessionCount ?? 0} 个会话 · ${status.value?.workspaceCount ?? 0} 个工作区`;
  if (status.value?.state === "stopped") return "本地服务当前未运行";
  return "本地服务状态未知";
});
const tunnelState = computed(() => config.enabled ? (gatewayStatus.value?.state ?? "unknown") : "stopped");
const tunnelLabel = computed(() => !config.enabled ? "未启用" : undefined);
const tunnelDetail = computed(() => {
  if (!config.enabled) return "公网入口未启用，本地 MCP 仍可独立运行";
  return gatewayStatus.value?.detail || "公网隧道状态未知";
});
const localEndpoint = computed(() => status.value?.localEndpoint || gatewayStatus.value?.localUrl || `http://127.0.0.1:${config.localPort}/mcp`);
const publicBaseUrl = computed(() => (gatewayStatus.value?.publicUrl || config.publicUrl).trim());
const publicEndpoint = computed(() => {
  if (!config.enabled) return "未启用公网连接";
  if (!publicBaseUrl.value) return "尚未配置";
  return `${publicBaseUrl.value.replace(/\/$/, "")}/mcp`;
});
const publicEndpointAvailable = computed(() => config.enabled && !!publicBaseUrl.value);
const publicEndpointStale = computed(() => publicEndpointAvailable.value && gatewayStatus.value?.state !== "running");
const healthFailedCount = computed(() => health.value.filter((item) => !item.ok).length);
const healthSummary = computed(() => {
  if (checking.value) return "检查中";
  if (healthCheckFailed.value) return "检查失败";
  if (!health.value.length) return "暂无结果";
  return healthFailedCount.value > 0 ? `${healthFailedCount.value} 项失败` : `${health.value.length}/${health.value.length} 正常`;
});
const healthSummaryStatus = computed(() => {
  if (checking.value) return "checking";
  if (healthCheckFailed.value || healthFailedCount.value > 0) return "error";
  if (health.value.length > 0) return "healthy";
  return "unknown";
});
const lastCheckedLabel = computed(() => {
  if (!lastCheckedAt.value) return "尚未检查";
  return `最近检查 ${new Date(lastCheckedAt.value).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" })}`;
});
const tunnelOptions = [
  { value: "none", label: "仅本地 / 外部反向代理" },
  { value: "frp", label: "FRP" },
  { value: "cloudflare", label: "Cloudflare Quick Tunnel" },
];
const frpOptions = computed(() => [{ value: "", label: "手动填写" }, ...frpProfiles.value.map((item) => ({ value: item.id, label: `${item.name} · ${item.server}:${item.serverPort}` }))]);

function applyRuntimeStatus(nextStatus: GlobalMcpOverviewDto) {
  status.value = nextStatus;
  globalMcpRuntimeState.value = nextStatus.state;
}

async function refresh() {
  loading.value = true;
  loadError.value = "";
  try {
    const [nextConfig, nextStatus, nextGateway, nextFrp] = await Promise.all([
      getGlobalGatewayConfig(),
      getGlobalMcpOverview(),
      getGlobalGatewayStatus(),
      listFrpProfiles(),
    ]);
    if (!dirty.value) {
      Object.assign(config, DEFAULT_GLOBAL_GATEWAY, nextConfig);
      baseline.value = JSON.stringify(config);
    }
    applyRuntimeStatus(nextStatus);
    gatewayStatus.value = nextGateway;
    frpProfiles.value = nextFrp;
  } catch (e) {
    loadError.value = String(e);
  } finally {
    loading.value = false;
  }
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

function changedConfigParts() {
  if (!baseline.value) return { localPort: false, tunnel: false };
  const previous = JSON.parse(baseline.value) as GlobalGatewayConfigDto;
  const localPort = previous.localPort !== config.localPort;
  const tunnel = [
    "enabled",
    "tunnelType",
    "publicUrl",
    "cloudflareMode",
    "frpProfileId",
    "frpServer",
    "frpSubdomain",
    "frpServerPort",
    "useProxy",
  ].some((key) => previous[key as keyof GlobalGatewayConfigDto] !== config[key as keyof GlobalGatewayConfigDto]);
  return { localPort, tunnel };
}

async function saveConfig() {
  if (!dirty.value && !savePromise) return;
  const changed = changedConfigParts();
  try {
    error.value = "";
    await persistConfig();
    if (changed.localPort && changed.tunnel) {
      showToast("连接配置已保存；MCP 端口需重启本地服务，公网隧道配置将在下次启动或重连时应用。", { kind: "success", duration: 7000 });
    } else if (changed.localPort) {
      showToast("MCP 端口已保存；重启本地服务后生效。", { kind: "success", duration: 6000 });
    } else if (changed.tunnel) {
      showToast("公网隧道配置已保存；Cloudflare Quick Tunnel 可使用下方操作立即重连。", { kind: "success", duration: 6000 });
    } else {
      showToast("连接配置已保存。", { kind: "success" });
    }
    return true;
  } catch (e) {
    error.value = String(e);
    return false;
  }
}

async function start() {
  if (busy.value) return;
  busy.value = true;
  try {
    await persistConfig();
    await startRuntime();
    const [nextStatus, nextGateway] = await Promise.all([getGlobalMcpOverview(), getGlobalGatewayStatus()]);
    applyRuntimeStatus(nextStatus);
    gatewayStatus.value = nextGateway;
    await runHealth();
    showToast("Global MCP 已启动", { kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "启动失败", kind: "error" });
  } finally {
    busy.value = false;
  }
}

async function stop() {
  if (busy.value) return;
  busy.value = true;
  try {
    await stopRuntime();
    const [nextStatus, nextGateway] = await Promise.all([getGlobalMcpOverview(), getGlobalGatewayStatus()]);
    applyRuntimeStatus(nextStatus);
    gatewayStatus.value = nextGateway;
    health.value = [];
    healthError.value = "";
    healthCheckFailed.value = false;
    lastCheckedAt.value = null;
    recordConnectionHealth([]);
    showToast("Global MCP 已停止", { kind: "info" });
  } catch (error) {
    showToast(String(error), { title: "停止失败", kind: "error" });
  } finally {
    busy.value = false;
  }
}

async function restartCloudflareTunnel() {
  if (busy.value || checking.value || !canForceRestartCloudflare.value) return;
  busy.value = true;
  tunnelActionError.value = "";
  try {
    await persistConfig();
    const nextGateway = await restartGlobalCloudflareTunnel();
    gatewayStatus.value = nextGateway;
    config.publicUrl = nextGateway.publicUrl;
    baseline.value = JSON.stringify(config);
    const nextStatus = await getGlobalMcpOverview();
    applyRuntimeStatus(nextStatus);
    await runHealth();
    showToast("Cloudflare Tunnel 已重连，新公网地址已更新。", { kind: "success" });
  } catch (error) {
    tunnelActionError.value = String(error);
    showToast(String(error), { title: "Cloudflare Tunnel 重启失败", kind: "error" });
    try {
      gatewayStatus.value = await getGlobalGatewayStatus();
    } catch {
      // Keep the action error visible when the follow-up status query also fails.
    }
    await runHealth();
  } finally {
    busy.value = false;
  }
}

async function runHealth() {
  if (checking.value) return;
  checking.value = true;
  healthError.value = "";
  try {
    health.value = await checkGlobalGatewayHealth();
    recordConnectionHealth(health.value.map(item => ({ ...item, hint: "" })));
    healthCheckFailed.value = false;
    lastCheckedAt.value = Date.now();
    const [nextStatus, nextGateway] = await Promise.all([getGlobalMcpOverview(), getGlobalGatewayStatus()]);
    applyRuntimeStatus(nextStatus);
    gatewayStatus.value = nextGateway;
  } catch (e) {
    healthCheckFailed.value = true;
    healthError.value = String(e);
    recordConnectionHealth(health.value.map(item => ({ ...item, hint: "" })), healthError.value);
    lastCheckedAt.value = Date.now();
  } finally {
    checking.value = false;
  }
}

async function copyEndpoint(kind: "local" | "public") {
  const value = kind === "local" ? localEndpoint.value : publicEndpoint.value;
  if ((kind === "public" && !publicEndpointAvailable.value) || !value || value === "尚未配置") return;
  const label = kind === "local" ? "本地端点" : "公网端点";
  try {
    await navigator.clipboard.writeText(value);
    copiedEndpoint.value = kind;
    if (copyTimer) window.clearTimeout(copyTimer);
    copyTimer = window.setTimeout(() => {
      copiedEndpoint.value = null;
    }, 1800);
    showToast(`${label}已复制`, { kind: "success", duration: 1800 });
  } catch (error) {
    showToast(String(error), { title: `复制${label}失败`, kind: "error" });
  }
}

function shouldOfferTunnelRecovery(item: GatewayHealthItemDto) {
  return canForceRestartCloudflare.value && item.label.includes("公网") && !item.ok;
}

useUnsavedChanges("MCP 端点与公网连接", dirty, saveConfig);
onMounted(() => {
  void (async () => {
    await refresh();
    await runHealth();
  })();
});
</script>

<template>
  <div id="settings-gateway" class="ui-stack">
    <div v-if="loadError" class="ui-banner ui-banner--warning" role="alert" aria-live="assertive">
      <span>{{ loadError }}</span>
      <BaseButton variant="secondary" @click="refresh">重试</BaseButton>
    </div>

    <GlassCard>
      <div class="section-heading flex flex-wrap items-start justify-between gap-4">
        <div class="flex min-w-0 items-center gap-3">
          <div class="grid h-11 w-11 shrink-0 place-items-center rounded-[var(--ui-radius-row)] bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]"><Globe2 :size="20" /></div>
          <div class="min-w-0">
            <h2 class="text-base font-semibold">MCP 连接</h2>
            <p class="mt-1 text-xs text-[var(--ui-text-muted)]">本地服务与公网隧道分开管理，连接状态更容易判断。</p>
          </div>
        </div>
        <div class="status-summary-grid" aria-label="连接状态">
          <div class="status-summary">
            <span class="text-xs text-[var(--ui-text-muted)]">本地 MCP</span>
            <StatusPill :status="mcpState" />
            <span class="text-xs text-[var(--ui-text-muted)]">{{ mcpDetail }}</span>
          </div>
          <div class="status-summary">
            <span class="text-xs text-[var(--ui-text-muted)]">公网隧道</span>
            <StatusPill :status="tunnelState" :label="tunnelLabel" />
            <span class="text-xs text-[var(--ui-text-muted)]">{{ tunnelDetail }}</span>
          </div>
        </div>
      </div>

      <div class="mt-4 grid gap-2">
        <div class="endpoint-row ui-inset p-3">
          <div class="min-w-0 flex-1">
            <span class="text-xs text-[var(--ui-text-muted)]">本地 MCP 端点</span>
            <code class="mt-1 block break-all text-xs">{{ localEndpoint }}</code>
          </div>
          <BaseButton variant="ghost" size="sm" :aria-label="copiedEndpoint === 'local' ? '本地端点已复制' : '复制本地端点'" @click="copyEndpoint('local')">
            <Check v-if="copiedEndpoint === 'local'" :size="13" class="text-[var(--ui-success)]" />
            <Copy v-else :size="13" />
            {{ copiedEndpoint === "local" ? "已复制" : "复制" }}
          </BaseButton>
        </div>
        <div class="endpoint-row ui-inset p-3" :class="publicEndpointStale ? 'endpoint-row--stale' : undefined">
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-2">
              <span class="text-xs text-[var(--ui-text-muted)]">公网 MCP 端点</span>
              <span v-if="publicEndpointStale" class="text-[11px] text-[var(--ui-warning)]">最近地址</span>
            </div>
            <code class="mt-1 block break-all text-xs">{{ publicEndpoint }}</code>
          </div>
          <BaseButton v-if="publicEndpointAvailable" variant="ghost" size="sm" :aria-label="copiedEndpoint === 'public' ? '公网端点已复制' : '复制公网端点'" @click="copyEndpoint('public')">
            <Check v-if="copiedEndpoint === 'public'" :size="13" class="text-[var(--ui-success)]" />
            <Copy v-else :size="13" />
            {{ copiedEndpoint === "public" ? "已复制" : "复制" }}
          </BaseButton>
        </div>
      </div>

      <div class="mt-4 flex flex-wrap gap-2">
        <BaseButton :busy="busy && !running" :disabled="running || loading" @click="start"><Play :size="13" />启动 MCP 服务</BaseButton>
        <BaseButton variant="secondary" :busy="busy && running" :disabled="!running || busy" @click="stop"><Square :size="12" />停止 MCP 服务</BaseButton>
        <BaseButton variant="ghost" :busy="checking" :disabled="busy" @click="runHealth"><Activity :size="13" />健康检查</BaseButton>
        <BaseButton variant="ghost" :busy="loading" :disabled="busy" @click="refresh"><RefreshCw :size="13" />刷新状态</BaseButton>
      </div>
    </GlassCard>

    <GlassCard>
      <div class="section-heading mb-4 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 class="text-base font-semibold">公网隧道配置</h2>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">所有工作区共享本地端点；FRP 和 Cloudflare 用于提供公网访问。</p>
        </div>
        <StatusPill :status="tunnelState" :label="tunnelLabel" />
      </div>

      <fieldset :disabled="saving || busy || loading || !!loadError" class="grid gap-3">
        <div class="ui-inset p-3"><ToggleSwitch v-model="config.enabled" label="启用公网连接" description="关闭时仍可正常使用本地 MCP 服务，只是不自动暴露公网入口。" /></div>
        <div class="settings-two-col"><TextField :model-value="String(config.localPort)" label="MCP 本地端口" :error="error && (!Number.isInteger(config.localPort) || config.localPort < 1 || config.localPort > 65535) ? error : undefined" type="number" @update:model-value="config.localPort = Number($event)" /><SelectField v-model="config.tunnelType" label="公网方式" :options="tunnelOptions" /></div>
        <template v-if="config.tunnelType === 'frp'">
          <SelectField v-model="config.frpProfileId" label="FRP 配置" :options="frpOptions" />
          <TextField v-model="config.frpSubdomain" label="子域名" placeholder="coding-tools" />
          <div v-if="!config.frpProfileId" class="settings-two-col"><TextField v-model="config.frpServer" label="FRP 服务器" placeholder="frp.example.com" /><TextField :model-value="String(config.frpServerPort)" label="端口" :error="error && (!Number.isInteger(config.frpServerPort) || config.frpServerPort < 1 || config.frpServerPort > 65535) ? error : undefined" type="number" @update:model-value="config.frpServerPort = Number($event)" /></div>
        </template>
        <div v-if="config.tunnelType === 'cloudflare'" class="ui-banner text-xs leading-5">Cloudflare Quick Tunnel 会分配临时公网地址；地址变化后，需要把新的 `/mcp` 地址重新填入外部 Connector。</div>
        <div v-if="canForceRestartCloudflare" class="tunnel-action-row ui-inset p-3">
          <div class="min-w-0 flex-1">
            <strong class="text-xs">Cloudflare Tunnel 运维</strong>
            <p class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">强制终止当前受管理的隧道进程并获取新的临时公网地址，只重启隧道，不会重启本地 MCP 服务。</p>
            <p v-if="tunnelActionError" class="ui-danger mt-2 break-words text-xs" role="alert" aria-live="assertive">重启失败：{{ tunnelActionError }}</p>
          </div>
          <BaseButton variant="secondary" :busy="busy" :disabled="loading || !!loadError || checking" aria-label="强制重启 Cloudflare Tunnel" @click="restartCloudflareTunnel"><RotateCw :size="13" />强制重启隧道</BaseButton>
        </div>
        <TextField v-if="config.tunnelType === 'none'" v-model="config.publicUrl" label="外部公网 URL（可选）" placeholder="https://gateway.example.com" />
        <div v-if="config.tunnelType !== 'none'" class="ui-inset p-3"><ToggleSwitch v-model="config.useProxy" label="使用网络代理" description="使用网络与工具页的网络代理连接公网隧道。" /></div>
        <SaveBar :dirty="dirty" :busy="saving" :error="error" label="保存连接配置" @save="saveConfig" />
      </fieldset>
    </GlassCard>

    <GlassCard id="settings-health">
      <div class="section-heading flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 class="text-base font-semibold">连接健康</h2>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">检查本地与公网 MCP 端点；进程运行中不等于公网请求一定可达。</p>
        </div>
        <div class="health-summary" aria-live="polite">
          <StatusPill :status="healthSummaryStatus" :label="healthSummary" />
          <span class="text-xs text-[var(--ui-text-muted)]">{{ lastCheckedLabel }}</span>
        </div>
      </div>
      <p v-if="healthError" class="ui-danger mt-3 break-words text-xs" role="alert" aria-live="assertive">检查失败：{{ healthError }}。已有结果会保留，点击健康检查重试。</p>
      <div class="mt-3 space-y-2">
        <div v-for="item in health" :key="item.label" class="health-row ui-inset p-3">
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-2">
              <strong class="text-xs">{{ item.label }}</strong>
              <StatusPill :status="connectionHealthPresentation(item).status" :label="connectionHealthPresentation(item).label" />
            </div>
            <p class="mt-1 break-all text-xs leading-5 text-[var(--ui-text-muted)]">{{ item.detail }}</p>
          </div>
          <BaseButton v-if="shouldOfferTunnelRecovery(item)" variant="ghost" size="sm" :busy="busy" :disabled="checking" @click="restartCloudflareTunnel"><RotateCw :size="12" />重启隧道</BaseButton>
        </div>
        <p v-if="health.length === 0" class="py-4 text-center text-xs text-[var(--ui-text-muted)]" aria-live="polite">{{ checking ? "正在检查连接…" : healthCheckFailed ? "连接健康未知" : "暂无连接检查结果" }}</p>
      </div>
    </GlassCard>
  </div>
</template>

<style scoped>
.section-heading { min-width: 0; }

.status-summary-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(150px, 1fr));
  gap: 8px;
  min-width: min(100%, 340px);
}

.status-summary {
  display: grid;
  grid-template-columns: auto auto;
  align-items: center;
  gap: 4px 8px;
  min-width: 0;
  padding: 10px 12px;
  border: 1px solid var(--ui-line);
  border-radius: var(--ui-radius-row);
  background: var(--ui-surface-subtle);
}

.status-summary > span:last-child { grid-column: 1 / -1; overflow-wrap: anywhere; }

.endpoint-row,
.tunnel-action-row,
.health-row {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.endpoint-row--stale { border-color: color-mix(in srgb, var(--ui-warning) 35%, var(--ui-line)); }

.health-row { align-items: flex-start; }

.health-summary { display: flex; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: 8px; }

.settings-two-col {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: .75rem;
  min-width: 0;
}

@container app-main (max-width: 700px) {
  .settings-two-col { grid-template-columns: 1fr; }
  .status-summary-grid { grid-template-columns: 1fr; min-width: 0; width: 100%; }
  .health-summary { justify-content: flex-start; }
}

@container app-main (max-width: 480px) {
  .endpoint-row,
  .tunnel-action-row,
  .health-row { align-items: flex-start; flex-wrap: wrap; }
  .endpoint-row > button,
  .tunnel-action-row > button,
  .health-row > button { margin-left: auto; }
}
</style>
