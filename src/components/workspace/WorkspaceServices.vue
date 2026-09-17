<script setup lang="ts">
import { computed, ref } from "vue";
import { ShieldCheck } from "@lucide/vue";
import GlassCard from "../ui/GlassCard.vue";
import SegmentedControl from "../ui/SegmentedControl.vue";
import RuntimePolicyPanel from "./RuntimePolicyPanel.vue";
import HistoryContextPanel from "./HistoryContextPanel.vue";
import type { WorkspaceProfile } from "$lib/types";
import type { RuntimePolicyDraft } from "$lib/workspace-page";

const props = defineProps<{
  workspaceId: string;
  profile: WorkspaceProfile;
  onSavePolicy: (draft: RuntimePolicyDraft) => Promise<void>;
  onSaveHistory: (recording: boolean, sessions: number[]) => Promise<void>;
}>();
const section = ref("policy");
const items = [
  { value: "policy", label: "执行权限" },
  { value: "history", label: "历史上下文" },
];
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
</script>

<template>
  <div class="grid gap-4">
    <GlassCard>
      <div class="mb-5 flex flex-wrap items-center justify-between gap-4 border-b border-black/[.055] pb-4 dark:border-white/[.07]">
        <div class="flex items-center gap-3">
          <div class="grid h-10 w-10 place-items-center rounded-2xl bg-[#0a84ff]/10 text-[#0a84ff]"><ShieldCheck :size="18" /></div>
          <div><h2 class="text-sm font-semibold">Workspace Context</h2><p class="mt-0.5 text-[11px] text-[var(--text-muted)]">只保存项目级策略。MCP 连接、认证和公网入口统一在全局设置管理。</p></div>
        </div>
        <SegmentedControl :items="items" :model-value="section" @update:model-value="section = $event" />
      </div>
      <RuntimePolicyPanel v-if="section === 'policy'" :workspace-id="workspaceId" :model="policyModel" :on-save="onSavePolicy" />
      <HistoryContextPanel v-else :workspace-id="workspaceId" :profile="profile" :on-save="onSaveHistory" />
    </GlassCard>
  </div>
</template>
