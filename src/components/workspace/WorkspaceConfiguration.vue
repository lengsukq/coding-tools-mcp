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
const props = defineProps<{
    workspaceId: string;
    profile: WorkspaceProfile;
    onSave: (draft: {
        name: string;
        path: string;
        allowHighRiskWrites: boolean;
    }) => Promise<void>;
    onSavePolicy: (draft: RuntimePolicyDraft) => Promise<void>;
    onSaveHistory: (recording: boolean, sessions: number[]) => Promise<void>;
}>();
defineEmits<{
    delete: [
    ];
}>();
const recording = ref(props.profile.runtime.history_recording !== false);
const busy = ref(false);
const error = ref('');
const dirty = computed(() => recording.value !== (props.profile.runtime.history_recording !== false));
watch(() => props.profile.runtime.history_recording, value => { if (!dirty.value)
    recording.value = value !== false; });
const policy = computed<RuntimePolicyDraft>(() => ({ toolProfile: props.profile.runtime.tool_profile, permissionMode: props.profile.runtime.permission_mode, inheritGlobalExecutionPolicy: props.profile.runtime.inherit_global_execution_policy ?? false, allowedCommands: props.profile.runtime.allowed_commands ?? '', executablePaths: props.profile.runtime.executable_paths ?? '', aiInstructions: props.profile.runtime.ai_instructions ?? '', instructionSources: props.profile.runtime.instruction_sources ?? [], skillSources: props.profile.runtime.skill_sources ?? [], customInstructionPaths: props.profile.runtime.custom_instruction_paths ?? '', customSkillPaths: props.profile.runtime.custom_skill_paths ?? '', workspaceLocalEntries: props.profile.runtime.workspace_local_entries ?? true, workspaceScriptExtensions: props.profile.runtime.workspace_script_extensions ?? '.exe,.bat,.cmd,.ps1' }));
async function save() { if (busy.value)
    return false; busy.value = true; error.value = ''; try {
    await props.onSaveHistory(recording.value, [...(props.profile.runtime.history_context_sessions ?? [])]);
    return true;
}
catch (e) {
    error.value = String(e);
    return false;
}
finally {
    busy.value = false;
} }
useUnsavedChanges('工作区历史记录', dirty, save);
</script>
<template><div class="ui-stack">
<p class="ui-banner">以下设置只影响当前工作区；继承全局时会显示具体来源。</p>
<WorkspaceSettings :profile="profile" :on-save="onSave" @delete="$emit('delete')"/>
<GlassCard>
<div class="ui-section-heading">
<div>
<h2>执行权限与 AI 上下文</h2>
<p>当前工作区覆盖值；全局默认值在应用设置 → AI 执行。</p>
</div>
</div>
<RuntimePolicyPanel :workspace-id="workspaceId" :model="policy" :on-save="onSavePolicy"/>
</GlassCard>
<GlassCard>
<ToggleSwitch v-model="recording" :disabled="busy" label="记录开发历史" description="把开发过程记录到项目内，供跨会话追溯。历史上下文选择在历史页管理。"/>
<SaveBar :dirty="dirty" :busy="busy" :error="error" @save="save"/>
</GlassCard>
</div></template>
