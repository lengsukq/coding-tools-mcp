import { computed, onMounted, reactive, ref, watch } from "vue";
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
import type { HealthItem } from "$lib/api/health";
import { listFrpProfiles, type FrpProfileDto, type GlobalRuntimeSettingsDto } from "$lib/api/settings";
import { getGlobalMcpOverview, startRuntime, stopRuntime, type GlobalMcpOverviewDto } from "$lib/api/workspaces";
import { useClipboard } from "$lib/composables/use-clipboard";
import { formatErrorMessage, formatTimeOnly } from "$lib/formatters";
import { globalMcpRuntimeState } from "$lib/stores/app";
import { showToast } from "$lib/stores/toast";
import { useUnsavedChanges } from "$lib/unsaved-changes";
import { isActionableHealthFailure } from "$lib/ui-model";
import { isValidPort } from "$lib/validators";

const GATEWAY_CONFIG_KEYS: ReadonlyArray<keyof GlobalGatewayConfigDto> = [
  "enabled",
  "localPort",
  "tunnelType",
  "publicUrl",
  "cloudflareMode",
  "cloudflareHttp2",
  "frpProfileId",
  "frpServer",
  "frpSubdomain",
  "frpServerPort",
  "frpTls",
  "useProxy",
];

const TUNNEL_CONFIG_KEYS: ReadonlyArray<keyof GlobalGatewayConfigDto> = [
  "enabled",
  "tunnelType",
  "publicUrl",
  "cloudflareMode",
  "cloudflareHttp2",
  "frpProfileId",
  "frpServer",
  "frpSubdomain",
  "frpServerPort",
  "frpTls",
  "useProxy",
];

const TUNNEL_OPTIONS = [
  { value: "none", label: "仅本地 / 外部反向代理" },
  { value: "frp", label: "FRP" },
  { value: "cloudflare", label: "Cloudflare Quick Tunnel" },
];

function isGatewayConfigEqual(a: GlobalGatewayConfigDto, b: GlobalGatewayConfigDto): boolean {
  return GATEWAY_CONFIG_KEYS.every((key) => a[key] === b[key]);
}

export interface UseGatewaySettingsOptions {
  panel: () => "gateway" | "gemini";
  loadRuntimeSettings: () => Promise<GlobalRuntimeSettingsDto>;
  onRecordHealth: (items: HealthItem[], error?: string | null) => void;
}

export function useGatewaySettings(options: UseGatewaySettingsOptions) {
  const config = reactive<GlobalGatewayConfigDto>({ ...DEFAULT_GLOBAL_GATEWAY });
  const baseline = ref<GlobalGatewayConfigDto | null>(null);
  const error = ref("");
  const loadError = ref("");
  const healthError = ref("");
  const tunnelActionError = ref("");
  const status = ref<GlobalMcpOverviewDto | null>(null);
  const gatewayStatus = ref<GlobalGatewayStatusDto | null>(null);
  const health = ref<GatewayHealthItemDto[]>([]);
  const authType = ref<string | null>(null);
  const mcpServerName = ref("");
  const frpProfiles = ref<FrpProfileDto[]>([]);
  const frpProfilesLoaded = ref(false);
  const loading = ref(true);
  const saving = ref(false);
  const busy = ref(false);
  const checking = ref(false);
  const healthCheckFailed = ref(false);
  const lastCheckedAt = ref<number | null>(null);
  let savePromise: Promise<void> | null = null;

  const { copiedKey, copyText } = useClipboard(1800);
  const copiedEndpoint = computed<"local" | "public" | null>(() =>
    copiedKey.value === "local" || copiedKey.value === "public" ? copiedKey.value : null,
  );

  const running = computed(() => status.value?.state === "running");
  const canForceRestartCloudflare = computed(
    () => config.enabled && config.tunnelType === "cloudflare" && config.cloudflareMode === "quick",
  );
  const dirty = computed(() => baseline.value !== null && !isGatewayConfigEqual(config, baseline.value));
  const mcpState = computed(() => status.value?.state ?? "unknown");
  const mcpDetail = computed(() => {
    if (running.value) return `${status.value?.sessionCount ?? 0} 个会话 · ${status.value?.workspaceCount ?? 0} 个工作区`;
    if (status.value?.state === "stopped") return "本地服务当前未运行";
    return "本地服务状态未知";
  });
  const tunnelState = computed(() => (config.enabled ? (gatewayStatus.value?.state ?? "unknown") : "stopped"));
  const tunnelLabel = computed(() => (!config.enabled ? "未启用" : undefined));
  const tunnelDetail = computed(() => {
    if (!config.enabled) return "公网入口未启用，本地 MCP 仍可独立运行";
    return gatewayStatus.value?.detail || "公网隧道状态未知";
  });
  const localEndpoint = computed(
    () => status.value?.localEndpoint || gatewayStatus.value?.localUrl || `http://127.0.0.1:${config.localPort}/mcp`,
  );
  const publicBaseUrl = computed(() => (gatewayStatus.value?.publicUrl || config.publicUrl).trim());
  const publicEndpoint = computed(() => {
    if (!config.enabled) return "未启用公网连接";
    if (!publicBaseUrl.value) return "尚未配置";
    return `${publicBaseUrl.value.replace(/\/$/, "")}/mcp`;
  });
  const publicEndpointAvailable = computed(() => config.enabled && !!publicBaseUrl.value);
  const publicEndpointStale = computed(() => publicEndpointAvailable.value && gatewayStatus.value?.state !== "running");
  const geminiPublicHealth = computed<"passed" | "failed" | "unchecked">(() => {
    if (healthCheckFailed.value) return "failed";
    const entry = health.value.find((item) => item.label === "Global MCP 公网入口");
    return entry ? (entry.ok ? "passed" : "failed") : "unchecked";
  });
  const healthFailedCount = computed(() => health.value.filter(isActionableHealthFailure).length);
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
    return `最近检查 ${formatTimeOnly(lastCheckedAt.value)}`;
  });
  const frpOptions = computed(() => [
    { value: "", label: "手动填写" },
    ...frpProfiles.value.map((item) => ({
      value: item.id,
      label: `${item.name} · ${item.server}:${item.serverPort}`,
    })),
  ]);

  function applyRuntimeStatus(nextStatus: GlobalMcpOverviewDto) {
    status.value = nextStatus;
    globalMcpRuntimeState.value = nextStatus.state;
  }

  function patchConfig(patch: Partial<GlobalGatewayConfigDto>) {
    Object.assign(config, patch);
  }

  async function ensureFrpProfilesLoaded() {
    if (frpProfilesLoaded.value) return;
    try {
      frpProfiles.value = await listFrpProfiles();
      frpProfilesLoaded.value = true;
    } catch (e) {
      console.warn("[useGatewaySettings] failed to load FRP profiles:", e);
    }
  }

  async function refresh(includeHealth = false) {
    loading.value = true;
    loadError.value = "";
    const shouldLoadFrp = options.panel() !== "gemini";
    try {
      const [nextConfig, nextStatus, nextGateway, nextRuntime, nextFrp] = await Promise.all([
        getGlobalGatewayConfig(),
        getGlobalMcpOverview(),
        getGlobalGatewayStatus(),
        options.loadRuntimeSettings(),
        shouldLoadFrp ? listFrpProfiles() : Promise.resolve(null),
      ]);
      if (!dirty.value) {
        Object.assign(config, DEFAULT_GLOBAL_GATEWAY, nextConfig);
        baseline.value = { ...config };
      }
      applyRuntimeStatus(nextStatus);
      gatewayStatus.value = nextGateway;
      authType.value = nextRuntime.authType;
      mcpServerName.value = nextRuntime.mcpServerName ?? "";
      if (nextFrp !== null) {
        frpProfiles.value = nextFrp;
        frpProfilesLoaded.value = true;
      }
      if (includeHealth) {
        await checkHealth(false);
      }
    } catch (e) {
      loadError.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  function normalizePort(value: number): number {
    const port = Number(value);
    if (!isValidPort(port)) throw new Error(`端口无效：${value}`);
    return port;
  }

  async function persistConfig() {
    if (savePromise) {
      await savePromise;
      if (!dirty.value) return;
    }
    if (!dirty.value) return;
    saving.value = true;
    const run = (async () => {
      while (dirty.value) {
        const snapshot: GlobalGatewayConfigDto = {
          ...config,
          localPort: normalizePort(config.localPort),
          frpServerPort: normalizePort(config.frpServerPort),
          publicUrl: config.publicUrl.trim(),
          frpServer: config.frpServer.trim(),
          frpSubdomain: config.frpSubdomain.trim(),
        };
        await setGlobalGatewayConfig(snapshot);
        const unchangedDuringSave =
          config.enabled === snapshot.enabled &&
          config.localPort === snapshot.localPort &&
          config.tunnelType === snapshot.tunnelType &&
          config.publicUrl.trim() === snapshot.publicUrl &&
          config.cloudflareMode === snapshot.cloudflareMode &&
          config.cloudflareHttp2 === snapshot.cloudflareHttp2 &&
          config.frpProfileId === snapshot.frpProfileId &&
          config.frpServer.trim() === snapshot.frpServer &&
          config.frpServerPort === snapshot.frpServerPort &&
          config.frpSubdomain.trim() === snapshot.frpSubdomain &&
          config.frpTls === snapshot.frpTls &&
          config.useProxy === snapshot.useProxy;
        baseline.value = { ...snapshot };
        if (unchangedDuringSave) {
          Object.assign(config, snapshot);
          break;
        }
      }
    })();
    savePromise = run;
    try {
      await run;
    } finally {
      if (savePromise === run) {
        savePromise = null;
      }
      saving.value = false;
    }
  }

  function changedConfigParts() {
    const previous = baseline.value;
    if (!previous) return { localPort: false, tunnel: false };
    const localPort = previous.localPort !== config.localPort;
    const tunnel = TUNNEL_CONFIG_KEYS.some((key) => previous[key] !== config[key]);
    return { localPort, tunnel };
  }

  async function saveConfig(): Promise<boolean> {
    if (!dirty.value && !savePromise) return true;
    const changed = changedConfigParts();
    try {
      error.value = "";
      await persistConfig();
      if (changed.localPort || changed.tunnel) {
        health.value = [];
        healthCheckFailed.value = false;
        lastCheckedAt.value = null;
        options.onRecordHealth([]);
      }
      if (running.value) {
        const [nextStatus, nextGateway] = await Promise.all([getGlobalMcpOverview(), getGlobalGatewayStatus()]);
        applyRuntimeStatus(nextStatus);
        gatewayStatus.value = nextGateway;
      }
      if (changed.localPort && changed.tunnel) {
        showToast("连接配置已保存，运行中的 MCP 端口与公网隧道已自动重载生效。", { kind: "success", duration: 6000 });
      } else if (changed.localPort) {
        showToast("MCP 端口已保存，运行中的本地监听已自动平滑重载生效。", { kind: "success", duration: 5000 });
      } else if (changed.tunnel) {
        showToast("公网隧道配置已保存并自动应用。", { kind: "success", duration: 5000 });
      } else {
        showToast("连接配置已保存并热更新生效。", { kind: "success" });
      }
      return true;
    } catch (e) {
      error.value = formatErrorMessage(e);
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
      await checkHealth(false);
      showToast("Global MCP 已启动", { kind: "success" });
    } catch (err) {
      showToast(String(err), { title: "启动失败", kind: "error" });
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
      options.onRecordHealth([]);
      showToast("Global MCP 已停止", { kind: "info" });
    } catch (err) {
      showToast(String(err), { title: "停止失败", kind: "error" });
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
      baseline.value = { ...config };
      const nextStatus = await getGlobalMcpOverview();
      applyRuntimeStatus(nextStatus);
      await checkHealth(false);
      showToast("Cloudflare Tunnel 已重连，新公网地址已更新。", { kind: "success" });
    } catch (err) {
      tunnelActionError.value = String(err);
      showToast(String(err), { title: "Cloudflare Tunnel 重启失败", kind: "error" });
      try {
        gatewayStatus.value = await getGlobalGatewayStatus();
      } catch (statusErr) {
        console.warn("[useGatewaySettings] follow-up gateway status failed:", statusErr);
      }
      await checkHealth(false);
    } finally {
      busy.value = false;
    }
  }

  async function checkHealth(refreshOverview: boolean) {
    if (checking.value) return;
    checking.value = true;
    healthError.value = "";
    try {
      health.value = await checkGlobalGatewayHealth();
      options.onRecordHealth(health.value.map((item) => ({ ...item, hint: "" })));
      healthCheckFailed.value = false;
      lastCheckedAt.value = Date.now();
      if (refreshOverview) {
        const [nextStatus, nextGateway] = await Promise.all([getGlobalMcpOverview(), getGlobalGatewayStatus()]);
        applyRuntimeStatus(nextStatus);
        gatewayStatus.value = nextGateway;
      }
    } catch (e) {
      healthCheckFailed.value = true;
      healthError.value = String(e);
      options.onRecordHealth(health.value.map((item) => ({ ...item, hint: "" })), healthError.value);
      lastCheckedAt.value = Date.now();
    } finally {
      checking.value = false;
    }
  }

  function runHealth() {
    return checkHealth(true);
  }

  async function copyEndpoint(kind: "local" | "public") {
    const value = kind === "local" ? localEndpoint.value : publicEndpoint.value;
    if ((kind === "public" && !publicEndpointAvailable.value) || !value || value === "尚未配置") return;
    const label = kind === "local" ? "本地端点" : "公网端点";
    await copyText(value, { key: kind, label, durationMs: 1800 });
  }

  watch(options.panel, (nextPanel) => {
    if (nextPanel !== "gemini" && !frpProfilesLoaded.value) {
      void ensureFrpProfilesLoaded();
    }
  });

  useUnsavedChanges("MCP 端点与公网连接", dirty, saveConfig);

  onMounted(() => {
    void refresh(true);
  });

  return {
    config,
    error,
    loadError,
    healthError,
    tunnelActionError,
    health,
    authType,
    mcpServerName,
    loading,
    saving,
    busy,
    checking,
    healthCheckFailed,
    copiedEndpoint,
    running,
    canForceRestartCloudflare,
    dirty,
    mcpState,
    mcpDetail,
    tunnelState,
    tunnelLabel,
    tunnelDetail,
    localEndpoint,
    publicEndpoint,
    publicEndpointAvailable,
    publicEndpointStale,
    geminiPublicHealth,
    healthSummary,
    healthSummaryStatus,
    lastCheckedLabel,
    tunnelOptions: TUNNEL_OPTIONS,
    frpOptions,
    patchConfig,
    refresh,
    saveConfig,
    start,
    stop,
    restartCloudflareTunnel,
    runHealth,
    copyEndpoint,
  };
}
