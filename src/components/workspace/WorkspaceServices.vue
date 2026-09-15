<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, Copy, Radio, RotateCw } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import SegmentedControl from "../ui/SegmentedControl.vue";
import StatusPill from "../ui/StatusPill.vue";
import TunnelConfigPanel from "./TunnelConfigPanel.vue";
import AuthConfigPanel from "./AuthConfigPanel.vue";
import RuntimePolicyPanel from "./RuntimePolicyPanel.vue";
import HistoryContextPanel from "./HistoryContextPanel.vue";
import GptQuickCopy from "./GptQuickCopy.vue";
import ChatGptSessionPrompt from "./ChatGptSessionPrompt.vue";
import type { AuthConfig, RuntimeState, WorkspaceProfile } from "$lib/types";
import type { RuntimePolicyDraft, SaveTunnelOptions, TunnelFormConfig } from "$lib/workspace-page";

const props = defineProps<{
  workspaceId: string;
  profile: WorkspaceProfile;
  state: RuntimeState;
  busy: boolean;
  localEndpoint: string;
  publicEndpoint: string;
  defaultLocalEndpoint: string;
}>();
const emit = defineEmits<{
  toggle: [];
  restart: [];
  saveTunnel: [config: TunnelFormConfig, options?: SaveTunnelOptions];
  saveAuth: [auth: AuthConfig];
  savePolicy: [draft: RuntimePolicyDraft];
  saveHistory: [recording: boolean, sessions: number[]];
}>();
const section = ref("connection");
const copied = ref<string | null>(null);
const items = [
  { value: "connection", label: "连接与隧道" },
  { value: "auth", label: "访问认证" },
  { value: "policy", label: "执行权限" },
  { value: "history", label: "历史上下文" },
];
const tunnelConfig = computed<TunnelFormConfig>(() => ({
  type: props.profile.tunnel.type ?? "none",
  public_url: props.profile.tunnel.public_url ?? "",
  frp_server: props.profile.tunnel.frp_server ?? "",
  frp_subdomain: props.profile.tunnel.frp_subdomain ?? "",
  frp_profile_id: props.profile.tunnel.frp_profile_id ?? "",
  frp_server_port: props.profile.tunnel.frp_server_port ?? 7000,
  cloudflare_mode: props.profile.tunnel.cloudflare_mode ?? "quick",
  use_proxy: props.profile.tunnel.use_proxy ?? true,
  use_global_gateway: props.profile.tunnel.use_global_gateway ?? false,
}));
const policyModel = computed<RuntimePolicyDraft>(() => ({
  toolProfile: props.profile.runtime.tool_profile,
  permissionMode: props.profile.runtime.permission_mode,
  inheritGlobalExecutionPolicy: props.profile.runtime.inherit_global_execution_policy ?? false,
  allowedCommands: props.profile.runtime.allowed_commands ?? "",
  executablePaths: props.profile.runtime.executable_paths ?? "",
  aiInstructions: props.profile.runtime.ai_instructions ?? "",
  instructionSources: props.profile.runtime.instruction_sources ?? [],
  skillSources: props.profile.runtime.skill_sources ?? [],
  customInstructionPaths: props.profile.runtime.custom_instruction_paths ?? "",
  customSkillPaths: props.profile.runtime.custom_skill_paths ?? "",
  workspaceLocalEntries: props.profile.runtime.workspace_local_entries ?? true,
  workspaceScriptExtensions: props.profile.runtime.workspace_script_extensions ?? ".exe,.bat,.cmd,.ps1",
}));

async function copy(value: string, key: string) {
  if (!value) return;
  await navigator.clipboard.writeText(value);
  copied.value = key;
  setTimeout(() => { copied.value = null; }, 1400);
}
</script>

<template>
  <div class="grid gap-4">
    <GlassCard class="overflow-hidden">
      <div class="mb-4 flex items-center justify-between gap-4">
        <div class="flex items-center gap-3"><div class="flex h-10 w-10 items-center justify-center rounded-2xl bg-[#30d158]/12 text-[#21a642]"><Radio :size="18" /></div><div><div class="flex items-center gap-2"><h2 class="text-sm font-semibold">MCP Runtime</h2><StatusPill :status="state" /></div><p class="mt-0.5 text-[11px] text-[var(--text-muted)]">Streamable HTTP · Tauri 内嵌 Runtime</p></div></div>
        <div class="flex gap-2"><BaseButton variant="secondary" :busy="busy" @click="$emit('restart')"><RotateCw :size="14" />重启</BaseButton><BaseButton :busy="busy" @click="$emit('toggle')">{{ state === 'running' ? '停止服务' : '启动服务' }}</BaseButton></div>
      </div>
      <div class="grid grid-cols-2 gap-3">
        <button type="button" class="rounded-2xl bg-black/[.025] p-3.5 text-left transition hover:bg-black/[.045] dark:bg-white/[.04] dark:hover:bg-white/[.065]" @click="copy(localEndpoint || defaultLocalEndpoint, 'local')"><span class="text-[10px] font-semibold uppercase tracking-[.12em] text-[var(--text-muted)]">Local endpoint</span><span class="mt-1.5 flex items-center gap-2"><code class="min-w-0 flex-1 truncate text-xs">{{ localEndpoint || defaultLocalEndpoint }}</code><Check v-if="copied === 'local'" :size="13" class="text-[#30d158]" /><Copy v-else :size="13" class="text-[var(--text-muted)]" /></span></button>
        <button type="button" class="rounded-2xl bg-black/[.025] p-3.5 text-left transition hover:bg-black/[.045] dark:bg-white/[.04] dark:hover:bg-white/[.065]" @click="copy(publicEndpoint, 'public')"><span class="text-[10px] font-semibold uppercase tracking-[.12em] text-[var(--text-muted)]">Public endpoint</span><span class="mt-1.5 flex items-center gap-2"><code class="min-w-0 flex-1 truncate text-xs">{{ publicEndpoint || '尚未连接公网隧道' }}</code><Check v-if="copied === 'public'" :size="13" class="text-[#30d158]" /><Copy v-else :size="13" class="text-[var(--text-muted)]" /></span></button>
      </div>
      <GptQuickCopy class="mt-4" :workspace-id="workspaceId" :profile="profile" :public-mcp-endpoint="publicEndpoint" />
    </GlassCard>

    <GlassCard>
      <div class="mb-5 flex items-center justify-between gap-4 border-b border-black/[.055] pb-4 dark:border-white/[.07]"><div><h2 class="text-sm font-semibold">服务配置</h2><p class="mt-0.5 text-[11px] text-[var(--text-muted)]">配置改动仍沿用原有 Tauri IPC 与重启策略。</p></div><SegmentedControl :items="items" :model-value="section" @update:model-value="section = $event" /></div>
      <TunnelConfigPanel v-if="section === 'connection'" :workspace-id="workspaceId" :config="tunnelConfig" @save="(config, options) => $emit('saveTunnel', config, options)" />
      <AuthConfigPanel v-else-if="section === 'auth'" :workspace-id="workspaceId" :auth="profile.auth" @save="$emit('saveAuth', $event)" />
      <RuntimePolicyPanel v-else-if="section === 'policy'" :workspace-id="workspaceId" :model="policyModel" @save="$emit('savePolicy', $event)" />
      <HistoryContextPanel v-else :workspace-id="workspaceId" :profile="profile" @save="(recording, sessions) => $emit('saveHistory', recording, sessions)" />
    </GlassCard>
    <ChatGptSessionPrompt />
  </div>
</template>
