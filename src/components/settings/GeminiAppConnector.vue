<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { Check, Copy, ExternalLink } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import { openUrl } from "$lib/api/app-info";
import {
  getGeminiLocalBridgeStatus,
  prepareGeminiLocalBridge,
  type GeminiLocalBridgeStatusDto,
} from "$lib/api/global-gateway";
import {
  assessGeminiConnection,
  assessGeminiLocalConnection,
  GEMINI_APP_URL,
  GEMINI_DEVELOPER_MODE_URL,
  GEMINI_LOCAL_AUTH_FILE,
  GEMINI_LOCAL_DEFAULT_BIN,
  GEMINI_LOCAL_SERVER_NAME,
  type GeminiConnectionInput,
} from "$lib/gemini-connector";
import { showToast } from "$lib/stores/toast";

const props = defineProps<GeminiConnectionInput & { localEndpoint: string; serverName?: string }>();
const effectiveServerName = computed(() => props.serverName?.trim() || GEMINI_LOCAL_SERVER_NAME);
const mode = ref<"local" | "cloud">("local");
const bridgeStatus = ref<GeminiLocalBridgeStatusDto | null>(null);
const preparingBridge = ref(false);
const preflight = computed(() => assessGeminiConnection({
  endpoint: props.endpoint,
  publicEnabled: props.publicEnabled,
  mcpRunning: props.mcpRunning,
  authType: props.authType,
  publicHealth: props.publicHealth,
  configDirty: props.configDirty,
}));
const localPreflight = computed(() => assessGeminiLocalConnection({
  localEndpoint: props.localEndpoint,
  mcpRunning: props.mcpRunning,
  authType: props.authType,
  bridgeCommand: bridgeStatus.value?.command,
  binaryReady: bridgeStatus.value?.binaryReady,
  credentialsReady: bridgeStatus.value?.credentialsReady,
  bridgeIssue: bridgeStatus.value?.issue,
}));
const copied = ref(false);
const localCopied = ref(false);
const prompt = computed(() => `请使用 ${effectiveServerName.value} 调用 workspace_list，列出可用工作区；然后先询问我需要进入哪个工作区，不要猜测路径。`);

const BRIDGE_REFRESH_DEBOUNCE_MS = 60;
let bridgeRefreshTimer: ReturnType<typeof setTimeout> | undefined;
let bridgeRefreshSeq = 0;

async function refreshBridgeStatus() {
  const seq = ++bridgeRefreshSeq;
  try {
    const next = await getGeminiLocalBridgeStatus();
    if (seq === bridgeRefreshSeq) bridgeStatus.value = next;
  } catch (error) {
    if (seq === bridgeRefreshSeq) {
      bridgeStatus.value = null;
      console.warn("[GeminiAppConnector] failed to load local bridge status:", error);
    }
  }
}

watch(
  () => [props.localEndpoint, props.authType],
  () => {
    if (bridgeRefreshTimer !== undefined) clearTimeout(bridgeRefreshTimer);
    bridgeRefreshTimer = setTimeout(() => {
      bridgeRefreshTimer = undefined;
      void refreshBridgeStatus();
    }, BRIDGE_REFRESH_DEBOUNCE_MS);
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  bridgeRefreshSeq += 1;
  if (bridgeRefreshTimer !== undefined) clearTimeout(bridgeRefreshTimer);
});

async function copyEndpoint() {
  if (!preflight.value.canCopy) return;
  try {
    await navigator.clipboard.writeText(props.endpoint);
    copied.value = true;
    showToast("Gemini MCP 地址已复制", { kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "复制失败", kind: "error" });
  }
}

async function copyLocalCommand() {
  if (!localPreflight.value.canCopy || preparingBridge.value) return;
  preparingBridge.value = true;
  try {
    const status = await prepareGeminiLocalBridge();
    bridgeStatus.value = status;
    await navigator.clipboard.writeText(status.command);
    localCopied.value = true;
    showToast("已生成本地桥接凭据并复制命令", { kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "准备或复制失败", kind: "error" });
  } finally {
    preparingBridge.value = false;
  }
}

async function copyPrompt() {
  try {
    await navigator.clipboard.writeText(prompt.value);
    showToast("验证提示词已复制", { kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "复制失败", kind: "error" });
  }
}

async function openGemini(url: string) {
  try {
    await openUrl(url);
  } catch (error) {
    showToast(String(error), { title: "打开 Gemini 失败", kind: "error" });
  }
}
</script>

<template>
  <GlassCard id="settings-gemini-connector">
    <div class="ui-section-heading">
      <div>
        <h2>Gemini App · MCP 连接</h2>
        <p>连接到当前 Global MCP。Local MCP 使用本机命令，无需公网隧道；Cloud MCP 继续使用 HTTPS 地址。</p>
      </div>
      <span class="text-xs font-medium" :class="(mode === 'local' ? localPreflight.ready : preflight.ready) ? 'text-[var(--ui-success)]' : 'text-[var(--ui-text-muted)]'">
        {{ (mode === 'local' ? localPreflight.ready : preflight.ready) ? "连接前置检查通过" : "尚未满足全部连接条件" }}
      </span>
    </div>

    <div class="mt-4 flex flex-wrap gap-2" role="group" aria-label="Gemini MCP 连接方式">
      <BaseButton size="sm" :variant="mode === 'local' ? 'primary' : 'secondary'" :aria-pressed="mode === 'local'" @click="mode = 'local'">Local MCP · 本地</BaseButton>
      <BaseButton size="sm" :variant="mode === 'cloud' ? 'primary' : 'secondary'" :aria-pressed="mode === 'cloud'" @click="mode = 'cloud'">Cloud MCP · 公网</BaseButton>
    </div>

    <template v-if="mode === 'local'">
      <div class="ui-inset mt-4 space-y-3 p-3">
        <div>
          <span class="text-xs text-[var(--ui-text-muted)]">服务器名称</span>
          <strong class="mt-1 block text-sm">{{ effectiveServerName }}</strong>
        </div>
        <div>
          <span class="text-xs text-[var(--ui-text-muted)]">说明</span>
          <p class="mt-1 text-xs">Coding Tools MCP 本地工作区管理与开发工具。复用当前正在运行的 Global MCP。</p>
        </div>
        <div class="flex min-w-0 flex-wrap items-center gap-3">
          <div class="min-w-0 flex-1">
            <span class="text-xs text-[var(--ui-text-muted)]">命令（粘贴到 Gemini 的 Local MCP → 命令）</span>
            <code class="mt-1 block break-all text-xs">{{ localPreflight.command || "当前本地端点无效，无法生成命令" }}</code>
          </div>
          <BaseButton variant="secondary" size="sm" :disabled="!localPreflight.canCopy || preparingBridge" @click="copyLocalCommand">
            <Check v-if="localCopied && !localPreflight.needsPrepare" :size="13" />
            <Copy v-else :size="13" />
            {{ preparingBridge ? "准备中..." : localPreflight.needsPrepare ? "生成凭据并复制" : localCopied ? "已复制" : "复制命令" }}
          </BaseButton>
        </div>
        <div class="grid gap-1 text-xs text-[var(--ui-text-muted)]">
          <span>桥接入口：<code>{{ bridgeStatus?.binaryPath || GEMINI_LOCAL_DEFAULT_BIN }}</code>（{{ bridgeStatus?.binaryReady ? "已就绪" : "点击右侧按钮自动生成" }}）</span>
          <span v-if="props.authType === 'oauth'">凭据文件（仅本机当前用户 0600 权限）：<code>{{ bridgeStatus?.authFilePath || GEMINI_LOCAL_AUTH_FILE }}</code></span>
        </div>
      </div>
      <div v-if="localPreflight.issues.length" class="ui-banner ui-banner--warning mt-3" role="status">
        <ul class="list-disc space-y-1 pl-5 text-xs leading-5">
          <li v-for="issue in localPreflight.issues" :key="issue">{{ issue }}</li>
        </ul>
      </div>
      <p v-else class="mt-3 text-xs text-[var(--ui-text-muted)]">本机原生 stdio 桥接与凭据已就绪（毫秒级启动，无需 Node.js / npx）；这不代表 Gemini 已实际保存或连接。</p>
      <div class="mt-4 space-y-2 text-xs leading-6 text-[var(--ui-text-secondary)]">
        <h3 class="text-sm font-semibold text-[var(--ui-text)]">本地连接步骤</h3>
        <ol class="list-decimal space-y-1 pl-5">
          <li>确认本机 Global MCP 正在运行，点击上方「{{ localPreflight.needsPrepare ? "生成凭据并复制" : "复制命令" }}」自动生成桥接入口与受保护的 OAuth 凭据并复制命令。</li>
          <li>在 Gemini 的「设置自定义的关联应用」中选择 Local MCP，服务器名称填写「{{ effectiveServerName }}」。</li>
          <li>将上方整行原生命令粘贴到「命令」字段；「说明」可填写 Coding Tools MCP 本地工具。</li>
          <li>若 Gemini 默认阻止本地桥接访问网络，在 Gemini 的「沙盒设置」中允许网络访问 <code>127.0.0.1</code>；内置默认沙盒已可读取 <code>~/.local</code> 下的桥接入口与凭据文件，无需关闭整个沙盒。</li>
          <li>保存后通过 workspace_list 验证连接。若出现 MCP.MCPError code=4，点击「显示日志」检查 Global MCP 是否已启动或沙盒是否拦截回环网络。</li>
        </ol>
        <p>内置原生 Rust <code>mcp-stdio</code> 桥接直接转发 Gemini 的 stdio MCP 至本机 HTTP /mcp，冷启动小于 10ms，不会触发 Gemini 导入约 5 秒超时，也不依赖 Node.js / npx 或浏览器交互式授权。</p>
        <p>本地命令仅对可执行该命令的 Gemini 客户端有效。如果使用的 Gemini 网页环境不具备本地进程执行能力，仍需选择 Cloud MCP。</p>
      </div>
    </template>

    <div v-if="mode === 'cloud'" class="ui-inset mt-4 space-y-3 p-3">
      <div>
        <span class="text-xs text-[var(--ui-text-muted)]">MCP 对外展示名称（可在「认证与访问范围」中自定义）</span>
        <strong class="mt-1 block text-sm">{{ effectiveServerName }}</strong>
      </div>
      <div class="flex min-w-0 flex-wrap items-center gap-3">
        <div class="min-w-0 flex-1">
          <span class="text-xs text-[var(--ui-text-muted)]">Gemini 自定义 MCP Server URL</span>
          <code class="mt-1 block break-all text-xs">{{ preflight.validEndpoint ? endpoint : "尚无可用的公网 HTTPS /mcp 地址" }}</code>
        </div>
        <BaseButton variant="secondary" size="sm" :disabled="!preflight.canCopy" @click="copyEndpoint">
          <Check v-if="copied" :size="13" />
          <Copy v-else :size="13" />
          {{ copied ? "已复制" : "复制地址" }}
        </BaseButton>
      </div>
    </div>

    <div v-if="mode === 'cloud' && preflight.issues.length" class="ui-banner ui-banner--warning mt-3" role="status">
      <div>
        <strong class="text-xs">连接前检查</strong>
        <ul class="mt-2 list-disc space-y-1 pl-5 text-xs leading-5">
          <li v-for="issue in preflight.issues" :key="issue">{{ issue }}</li>
        </ul>
      </div>
    </div>
    <p v-else-if="mode === 'cloud'" class="mt-3 text-xs text-[var(--ui-text-muted)]">
      本地服务、OAuth 与公网可达性检查通过。此结果不代表 Gemini 账号已完成授权。
    </p>

    <div v-if="mode === 'cloud'" class="mt-4 space-y-2 text-xs leading-6 text-[var(--ui-text-secondary)]">
      <h3 class="text-sm font-semibold text-[var(--ui-text)]">连接步骤</h3>
      <ol class="list-decimal space-y-1 pl-5">
        <li>确保 MCP 服务正在运行，公网健康检查通过，认证方式为 OAuth。</li>
        <li>在 Gemini 网页版打开 Settings → Connected Apps → Custom apps → Add custom app。</li>
        <li>粘贴上方 HTTPS /mcp 地址，按照 Gemini 引导进行 OAuth 授权；优先使用动态客户端注册（DCR）。</li>
        <li>在 Gemini 对话中启用该应用并验证 workspace_list，移动端使用同一 Google 账号访问。</li>
      </ol>
      <p>适用条件：美国地区、18 岁以上、个人 Google 账号、英语界面且开启 Keep Activity。能否创建连接由 Gemini 账号和地区资格决定。</p>
      <p>建议仅给需要的工作区授权，保留 MCP 执行策略与敏感操作确认。不要把 Bearer Token、Client Secret 或授权口令添加到 URL。</p>
    </div>

    <div class="mt-4 flex flex-wrap gap-2">
      <BaseButton size="sm" variant="secondary" @click="openGemini(GEMINI_APP_URL)">
        <ExternalLink :size="13" /> 打开 Gemini
      </BaseButton>
      <BaseButton size="sm" variant="ghost" @click="openGemini(GEMINI_DEVELOPER_MODE_URL)">
        <ExternalLink :size="13" /> Developer Mode
      </BaseButton>
      <BaseButton size="sm" variant="ghost" @click="copyPrompt">
        <Copy :size="13" /> 复制验证提示词
      </BaseButton>
    </div>
  </GlassCard>
</template>
