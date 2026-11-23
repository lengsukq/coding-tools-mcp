<script setup lang="ts">
import { computed, ref, watch } from "vue";
import GlassCard from "../ui/GlassCard.vue";
import ToggleSwitch from "../ui/ToggleSwitch.vue";
import SaveBar from "../ui/SaveBar.vue";
import WorkspaceSettings from "./WorkspaceSettings.vue";
import RuntimePolicyPanel from "./RuntimePolicyPanel.vue";
import { useUnsavedChanges } from "$lib/unsaved-changes";
import type { WorkspaceProfile } from "$lib/types";
import type { RuntimePolicyDraft } from "$lib/workspace-page";

const props = withDefaults(
  defineProps<{
    workspaceId: string;
    profile: WorkspaceProfile;
    panel?: "project" | "policy" | "context";
    onSave: (draft: { name: string; path: string; allowHighRiskWrites: boolean }) => Promise<void>;
    onSavePolicy: (draft: RuntimePolicyDraft) => Promise<void>;
    onSaveHistory: (recording: boolean, sessions: number[]) => Promise<void>;
  }>(),
  { panel: "project" },
);

defineEmits<{
  delete: [];
}>();

const recording = ref(props.profile.runtime.history_recording !== false);
const busy = ref(false);
const error = ref("");
const dirty = computed(() => recording.value !== (props.profile.runtime.history_recording !== false));

watch(
  () => props.profile.runtime.history_recording,
  (value) => {
    if (!dirty.value) recording.value = value !== false;
  },
);

const policy = computed<RuntimePolicyDraft>(() => ({
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
  disabledInstructionIds: props.profile.runtime.disabled_instruction_ids ?? [],
  disabledSkillIds: props.profile.runtime.disabled_skill_ids ?? [],
  workspaceLocalEntries: props.profile.runtime.workspace_local_entries ?? true,
  workspaceScriptExtensions: props.profile.runtime.workspace_script_extensions ?? ".exe,.bat,.cmd,.ps1",
}));

async function save() {
  if (busy.value) return false;
  busy.value = true;
  error.value = "";
  try {
    await props.onSaveHistory(recording.value, [...(props.profile.runtime.history_context_sessions ?? [])]);
    return true;
  } catch (e) {
    error.value = String(e);
    return false;
  } finally {
    busy.value = false;
  }
}

useUnsavedChanges("工作区历史记录", dirty, save);
</script>

<template>
  <div class="ui-stack animate-fade-in-up">
    <!-- 子页 1：项目信息、仓库保护与启动提示词 -->
    <WorkspaceSettings
      v-if="panel === 'project'"
      :profile="profile"
      :on-save="onSave"
      @delete="$emit('delete')"
    />

    <!-- 子页 2：执行权限与命令白名单 -->
    <GlassCard v-else-if="panel === 'policy'">
      <div class="ui-section-heading">
        <div>
          <h2>执行权限与命令策略</h2>
          <p>仅作用于当前工作区；开启「继承全局执行权限」时自动与全局白名单保持同步。</p>
        </div>
      </div>
      <RuntimePolicyPanel
        :workspace-id="workspaceId"
        :model="policy"
        section="policy"
        :on-save="onSavePolicy"
      />
    </GlassCard>

    <!-- 子页 3：AI 指令、技能源与开发历史记录 -->
    <template v-else>
      <GlassCard>
        <div class="ui-section-heading">
          <div>
            <h2>AI 指令与技能上下文</h2>
            <p>管理当前仓库专属规则提示词，并扫描项目内与全局生效的技能目录。</p>
          </div>
        </div>
        <RuntimePolicyPanel
          :workspace-id="workspaceId"
          :model="policy"
          section="context"
          :on-save="onSavePolicy"
        />
      </GlassCard>

      <GlassCard>
        <div class="ui-section-heading">
          <div>
            <h2>开发历史归档</h2>
            <p>开启后自动记录会话过程与变更摘要，可在「历史会话上下文」页勾选注入给后续 AI 会话。</p>
          </div>
        </div>
        <ToggleSwitch
          v-model="recording"
          :disabled="busy"
          label="记录开发历史"
          description="将开发过程持久化保存在项目配置内，便于跨会话回溯。"
        />
        <SaveBar :dirty="dirty" :busy="busy" :error="error" @save="save" />
      </GlassCard>
    </template>
  </div>
</template>
