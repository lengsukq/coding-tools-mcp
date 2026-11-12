<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, Copy, ExternalLink } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import { openUrl } from "$lib/api/app-info";
import {
  assessGeminiConnection,
  GEMINI_APP_URL,
  GEMINI_DEVELOPER_MODE_URL,
  type GeminiConnectionInput,
} from "$lib/gemini-connector";
import { showToast } from "$lib/stores/toast";

const props = defineProps<GeminiConnectionInput>();
const preflight = computed(() => assessGeminiConnection({
  endpoint: props.endpoint,
  publicEnabled: props.publicEnabled,
  mcpRunning: props.mcpRunning,
  authType: props.authType,
  publicHealth: props.publicHealth,
  configDirty: props.configDirty,
}));
const copied = ref(false);
const prompt = "请使用 Coding Tools MCP 调用 workspace_list，列出可用工作区；然后先询问我需要进入哪个工作区，不要猜测路径。";

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

async function copyPrompt() {
  try {
    await navigator.clipboard.writeText(prompt);
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
        <p>将当前 Global MCP 作为 Gemini 自定义应用接入。Gemini 网页端创建连接，移动 App 使用同一账号的已连接应用。</p>
      </div>
      <span class="text-xs font-medium" :class="preflight.ready ? 'text-[var(--ui-success)]' : 'text-[var(--ui-text-muted)]'">
        {{ preflight.ready ? "本地前置检查通过" : "尚未满足全部连接条件" }}
      </span>
    </div>

    <div class="ui-inset mt-4 flex min-w-0 flex-wrap items-center gap-3 p-3">
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

    <div v-if="preflight.issues.length" class="ui-banner ui-banner--warning mt-3" role="status">
      <div>
        <strong class="text-xs">连接前检查</strong>
        <ul class="mt-2 list-disc space-y-1 pl-5 text-xs leading-5">
          <li v-for="issue in preflight.issues" :key="issue">{{ issue }}</li>
        </ul>
      </div>
    </div>
    <p v-else class="mt-3 text-xs text-[var(--ui-text-muted)]">
      本地服务、OAuth 与公网可达性检查通过。此结果不代表 Gemini 账号已完成授权。
    </p>

    <div class="mt-4 space-y-2 text-xs leading-6 text-[var(--ui-text-secondary)]">
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
