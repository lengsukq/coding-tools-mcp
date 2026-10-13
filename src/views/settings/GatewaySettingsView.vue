<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from "vue";
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
import SaveBar from "$src/components/ui/SaveBar.vue";
import { useUnsavedChanges } from "$lib/unsaved-changes";
import { connectionHealthPresentation } from "$lib/ui-model";
import { showToast } from "$lib/stores/toast";
import {
  approveComputerScreenRequest,
  decideComputerSensitiveAction,
  getComputerScreenStatus,
  listComputerAvailableApps,
  pauseComputerAccess,
  requestComputerAccessibilityPermission,
  requestComputerScreenPermission,
  resumeComputerAccess,
  revokeComputerScreenAccess,
  type ComputerScreenStatus,
  type ComputerApplication,
} from "$lib/api/computer";

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
const computer = ref<ComputerScreenStatus | null>(null);
const computerBusy = ref(false);
const computerApps = ref<ComputerApplication[]>([]);
const selectedComputerApp = ref("");
const computerAppOptions = computed(() => computerApps.value.map((app) => ({ value: app.bundle_id, label: app.name })));
const screenPermissionTroubleshooting = computed(() => {
  const diagnostic = computer.value?.screenPermissionDiagnostic;
  if (!diagnostic?.ok) return "当前进程的权限诊断暂不可用；请先更新并重新打开桌面应用。";
  if (!diagnostic.is_app_bundle) return "检测到当前程序不是从 macOS .app 应用包运行。开发版与系统设置里的正式应用可能不是同一权限主体；请完全退出开发进程后，从「应用程序」启动已授权的正式 App。";
  if (diagnostic.bundle_id !== diagnostic.expected_bundle_id) return "当前运行进程的 Bundle ID 与项目正式应用不一致。请退出此版本，改从「应用程序」启动 Coding Tools MCP，并确认系统设置中授权的确实是同一份应用。";
  return "macOS 当前进程仍未通过屏幕录制权限预检。即使系统设置的开关已开启，授权变更后也可能需要完全退出应用（含菜单栏常驻进程）并重新启动，然后点击「重新检测」。如仍失败，请检查当前应用路径是否就是已授权的版本。";
});
let computerPoll: ReturnType<typeof setInterval> | undefined;
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

async function decideSensitiveAction(approve: boolean) {
  const confirmationId = computer.value?.pendingSensitiveActionId;
  if (!confirmationId || computerBusy.value) return;
  computerBusy.value = true;
  try {
    computer.value = await decideComputerSensitiveAction(confirmationId, approve);
    showToast(approve
      ? "已批准一次敏感动作；AI 须在有效期内携带确认 ID 原样重试，批准本身不会执行动作。"
      : "已拒绝本次敏感操作。",
    { kind: "info" });
  } catch (error) {
    showToast(String(error), { title: "敏感操作审批失败", kind: "error" });
  } finally {
    computerBusy.value = false;
  }
}

async function toggleComputerPause() {
  if (computerBusy.value || !computer.value?.active) return;
  computerBusy.value = true;
  try {
    computer.value = computer.value.paused
      ? await resumeComputerAccess()
      : await pauseComputerAccess();
    showToast(computer.value.paused ? "Computer Use 已暂停，截图和输入均已阻止。" : "Computer Use 已恢复，请重新截图确认当前界面。", { kind: "info" });
  } catch (error) {
    showToast(String(error), { title: "切换 Computer Use 状态失败", kind: "error" });
  } finally {
    computerBusy.value = false;
  }
}

async function requestAccessibilityPermission() {
  try {
    computer.value = await requestComputerAccessibilityPermission();
    if (!computer.value.accessibilityGranted) {
      showToast("请在 macOS 系统设置 → 隐私与安全性 → 辅助功能中允许 Coding Tools MCP。", { kind: "info", duration: 7000 });
    }
  } catch (error) {
    showToast(String(error), { title: "无法请求辅助功能权限", kind: "error" });
  }
}

async function requestScreenPermission() {
  try {
    computer.value = await requestComputerScreenPermission();
    if (!computer.value.screenRecordingGranted) {
      showToast("请在 macOS 系统设置的屏幕录制权限中授权 Coding Tools MCP；授权后可能需要重启应用。", { kind: "info", duration: 7000 });
    }
  } catch (error) {
    showToast(String(error), { title: "无法请求屏幕录制权限", kind: "error" });
  }
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
    healthCheckFailed.value = false;
    lastCheckedAt.value = Date.now();
    const [nextStatus, nextGateway] = await Promise.all([getGlobalMcpOverview(), getGlobalGatewayStatus()]);
    applyRuntimeStatus(nextStatus);
    gatewayStatus.value = nextGateway;
  } catch (e) {
    healthCheckFailed.value = true;
    healthError.value = String(e);
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

async function refreshComputerScreen() {
  try {
    const previous = computer.value?.pendingRequestId;
    computer.value = await getComputerScreenStatus();
    if (computer.value.pendingScope === "control" && computer.value.pendingRequestId && computer.value.pendingRequestId !== previous) {
      const result = await listComputerAvailableApps();
      computerApps.value = result.applications || [];
      selectedComputerApp.value = "";
    }
  } catch {
    // MCP connection settings must remain functional if the desktop command
    // is unavailable in an older installed runtime.
  }
}

async function approveComputer() {
  const requestId = computer.value?.pendingRequestId;
  if (!requestId || computerBusy.value) return;
  computerBusy.value = true;
  try {
    const control = computer.value?.pendingScope === "control";
    computer.value = await approveComputerScreenRequest(requestId, control ? selectedComputerApp.value : undefined);
    showToast(control ? "已授权当前会话控制选定应用 5 分钟。" : "已授权当前会话查看屏幕 5 分钟。", { kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "屏幕授权失败", kind: "error" });
  } finally {
    computerBusy.value = false;
  }
}

async function revokeComputer() {
  if (computerBusy.value) return;
  computerBusy.value = true;
  try {
    computer.value = await revokeComputerScreenAccess();
    showToast("Computer Use 屏幕访问已关闭", { kind: "info" });
  } catch (error) {
    showToast(String(error), { title: "撤销失败", kind: "error" });
  } finally {
    computerBusy.value = false;
  }
}

useUnsavedChanges("MCP 端点与公网连接", dirty, saveConfig);
onMounted(() => {
  void (async () => {
    await refresh();
    await runHealth();
  })();
  void refreshComputerScreen();
  computerPoll = setInterval(() => void refreshComputerScreen(), 3000);
});
onUnmounted(() => {
  if (computerPoll) clearInterval(computerPoll);
});
</script>

<template>
  <div id="settings-gateway" class="ui-stack">
    <div v-if="loadError" class="ui-banner ui-banner--warning" role="alert" aria-live="assertive">
      <span>{{ loadError }}</span>
      <BaseButton variant="secondary" @click="refresh">重试</BaseButton>
    </div>

    <GlassCard>
      <div class="section-heading flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 class="text-base font-semibold">Computer Use · 屏幕访问</h2>
          <p class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">
            默认关闭。“查看”权限只允许读取屏幕，“控制”权限可向你明确选择的目标应用发送鼠标与键盘事件。
            仅当前 MCP 会话有效 5 分钟，随时可撤销。Mac 系统权限需由你手动授予。
          </p>
        </div>
        <StatusPill :status="computer?.active && !computer?.paused ? 'running' : 'stopped'" :label="computer?.paused ? '已暂停' : computer?.active ? '已授权' : '未授权'" />
      </div>
      <div v-if="computer?.active && computer.pendingSensitiveActionId" class="ui-inset mt-3 flex flex-wrap items-center gap-3 p-3">
        <div class="min-w-0 flex-1">
          <strong class="text-xs">高风险操作需单独确认</strong>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            目标应用：{{ computer.pendingSensitiveActionApp }} · {{ computer.pendingSensitiveActionDescription }}
          </p>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            {{ computer.pendingSensitiveActionApproved ? "已允许一次，等待原客户端重试执行；不可重复使用" : "审批本身不会执行操作" }} · 剩余 {{ computer.pendingSensitiveActionExpiresInSeconds ?? 0 }} 秒
          </p>
        </div>
        <BaseButton v-if="!computer.pendingSensitiveActionApproved" :disabled="computerBusy || computer.paused" @click="decideSensitiveAction(true)">允许本次</BaseButton>
        <BaseButton variant="secondary" :disabled="computerBusy" @click="decideSensitiveAction(false)">拒绝</BaseButton>
      </div>
      <div v-if="computer?.accessibilityGranted === false" class="ui-inset mt-3 flex flex-wrap items-center justify-between gap-3 p-3">
        <p class="text-xs text-[var(--ui-text-muted)]">控制鼠标和键盘需要 macOS 辅助功能权限。</p>
        <BaseButton variant="secondary" :disabled="computerBusy" @click="requestAccessibilityPermission">请求辅助功能权限</BaseButton>
      </div>
      <div v-if="computer && !computer.supported" class="ui-banner ui-banner--warning mt-3 text-xs">
        首版仅支持 macOS。
      </div>
      <div v-else-if="computer?.screenRecordingGranted === false" class="ui-inset mt-3 flex flex-wrap items-start justify-between gap-3 p-3">
        <div class="min-w-0 flex-1">
          <p class="text-xs font-semibold">当前运行进程未通过 macOS 屏幕录制权限检测</p>
          <p class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">{{ screenPermissionTroubleshooting }}</p>
          <details class="mt-2 text-xs text-[var(--ui-text-muted)]">
            <summary class="cursor-pointer">查看当前运行进程的权限诊断</summary>
            <div class="mt-2 grid gap-1 break-all">
              <p>实际 Bundle ID：{{ computer.screenPermissionDiagnostic?.bundle_id || "无法获取" }}</p>
              <p>预期 Bundle ID：{{ computer.screenPermissionDiagnostic?.expected_bundle_id || "com.codingtools.mcp.desktop" }}</p>
              <p>启动形式：{{ computer.screenPermissionDiagnostic?.is_app_bundle ? ".app 应用包" : "独立可执行文件 / 开发版本" }}</p>
              <p>应用路径：{{ computer.screenPermissionDiagnostic?.bundle_path || "无法获取" }}</p>
              <p>可执行程序：{{ computer.screenPermissionDiagnostic?.executable_path || "无法获取" }}</p>
              <p>当前进程检测：{{ computer.screenPermissionDiagnostic?.preflight_granted ? "通过" : "未通过" }}</p>
            </div>
          </details>
        </div>
        <div class="flex flex-wrap gap-2">
          <BaseButton variant="secondary" :disabled="computerBusy" @click="refreshComputerScreen">重新检测</BaseButton>
          <BaseButton variant="secondary" :disabled="computerBusy" @click="requestScreenPermission">请求系统权限</BaseButton>
        </div>
      </div>
      <div v-if="computer?.pendingRequestId" class="ui-inset mt-3 flex flex-wrap items-center gap-3 p-3">
        <div class="min-w-0 flex-1">
          <strong class="text-xs">有 AI 客户端申请{{ computer.pendingScope === "control" ? "控制指定应用" : "查看屏幕" }}</strong>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            请求编号：{{ computer.pendingRequestId }} · {{ computer.pendingExpiresInSeconds ?? 0 }} 秒后失效
          </p>
          <SelectField v-if="computer.pendingScope === 'control'" v-model="selectedComputerApp" class="mt-2" label="允许控制的应用（须切换该应用到前台后才能操作）" :options="computerAppOptions" />
        </div>
        <BaseButton :busy="computerBusy" :disabled="!computer.supported || !computer.screenRecordingGranted || (computer.pendingScope === 'control' && (!computer.accessibilityGranted || !selectedComputerApp))" @click="approveComputer">批准 5 分钟</BaseButton>
        <BaseButton variant="secondary" :disabled="computerBusy" @click="revokeComputer">拒绝</BaseButton>
      </div>
      <div v-else class="mt-3 flex flex-wrap items-center justify-between gap-3">
        <span class="text-xs text-[var(--ui-text-muted)]">
          {{ computer?.active ? `${computer.activeScope === "control" ? "控制" : "查看"}授权 · 剩余 ${computer.expiresInSeconds ?? 0} 秒${computer.targetAppBundle ? " · " + computer.targetAppBundle : ""}${computer.remainingControlActions !== null ? " · 剩余操作 " + computer.remainingControlActions + " 次" : ""}` : "当前没有已批准的 Computer Use 会话" }}
        </span>
        <div v-if="computer?.active" class="flex flex-wrap items-center gap-2">
          <BaseButton variant="secondary" :disabled="computerBusy" @click="toggleComputerPause">{{ computer.paused ? "恢复" : "紧急暂停" }}</BaseButton>
          <BaseButton variant="secondary" :disabled="computerBusy" @click="revokeComputer">立即撤销</BaseButton>
        </div>
        <BaseButton v-else variant="ghost" :disabled="computerBusy" @click="refreshComputerScreen">刷新</BaseButton>
      </div>
    </GlassCard>

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
