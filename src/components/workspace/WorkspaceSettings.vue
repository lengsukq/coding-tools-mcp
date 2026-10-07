<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderInput, Save, Trash2 } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import ConfirmDialog from "../ui/ConfirmDialog.vue";
import GlassCard from "../ui/GlassCard.vue";
import TextField from "../ui/TextField.vue";
import ToggleSwitch from "../ui/ToggleSwitch.vue";
import ChatGptSessionPrompt from "./ChatGptSessionPrompt.vue";
import SaveBar from "../ui/SaveBar.vue";
import {useUnsavedChanges} from "$lib/unsaved-changes";
import { showToast } from "$lib/stores/toast";
import type { WorkspaceProfile } from "$lib/types";

const props = defineProps<{
  profile: WorkspaceProfile;
  onSave: (draft: { name: string; path: string; allowHighRiskWrites: boolean }) => Promise<void>;
}>();
defineEmits<{ delete: [] }>();
const draft = reactive({
  name: props.profile.name,
  path: props.profile.path,
  allowHighRiskWrites: props.profile.runtime.allow_high_risk_writes ?? false,
});
const saving = ref(false);
const highRiskConfirmOpen = ref(false);
const saveError=ref("");
watch(() => props.profile, (value) => {
  if(dirty.value && !saving.value) return;
  draft.name = value.name;
  draft.path = value.path;
  draft.allowHighRiskWrites = value.runtime.allow_high_risk_writes ?? false;
}, { deep: true });
const infoDirty = computed(() => draft.name.trim() !== props.profile.name || draft.path !== props.profile.path);
const securityDirty = computed(() => draft.allowHighRiskWrites !== (props.profile.runtime.allow_high_risk_writes ?? false));
const dirty = computed(() => infoDirty.value || securityDirty.value);
const enablingHighRiskWrites = computed(() =>
  draft.allowHighRiskWrites && !(props.profile.runtime.allow_high_risk_writes ?? false),
);

async function choosePath() {
  const result = await open({ directory: true, multiple: false, defaultPath: draft.path });
  if (result && !Array.isArray(result)) draft.path = result;
}
async function save() {
  if (saving.value || !dirty.value) return false;
  const name = draft.name.trim();
  if (!name) {
    saveError.value="工作区名称不能为空";
    return false;
  }
  if(!draft.path.trim()){saveError.value="请选择项目目录";return false;}
  saveError.value="";
  saving.value = true;
  try {
    await props.onSave({ name, path: draft.path, allowHighRiskWrites: draft.allowHighRiskWrites });
    draft.name = name;
    highRiskConfirmOpen.value = false;
    showToast("工作区设置已保存", { kind: "success" });
    return true;
  } catch (error) {
    saveError.value=String(error);return false;
  } finally {
    saving.value = false;
  }
}

function requestSave() {
  if (saving.value || !dirty.value) return;
  if (enablingHighRiskWrites.value) {
    highRiskConfirmOpen.value = true;
    return;
  }
  void save();
}
useUnsavedChanges("工作区信息与仓库保护",dirty,async()=>{if(enablingHighRiskWrites.value){throw new Error("请取消离开，在工作区设置中确认开启高风险文件写入后再保存。");}return save();});
</script>

<template>
  <div class="grid gap-4 animate-fade-in-up">
    <ChatGptSessionPrompt />
    <GlassCard><div class="mb-4"><h2 class="text-base font-semibold">工作区信息</h2><p class="mt-1 text-xs text-[var(--ui-text-muted)]">名称只影响桌面端显示；目录变化后运行中的 MCP 需要重启。</p></div><div class="grid gap-4"><TextField v-model="draft.name" label="工作区名称" :disabled="saving" :error="saveError && !draft.name.trim() ? saveError : undefined" /><div><TextField v-model="draft.path" label="项目目录" :disabled="saving" /><BaseButton class="mt-2" :disabled="saving" variant="secondary" size="sm" @click="choosePath"><FolderInput :size="13" />重新选择目录</BaseButton></div></div></GlassCard>
    <GlassCard>
      <div class="mb-4">
        <h2 class="text-base font-semibold">仓库保护</h2>
        <p class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">默认保护 GitHub 配置，避免 AI 意外修改 Actions、Issue 模板和仓库配置。</p>
      </div>
      <div class="ui-inset p-3">
        <ToggleSwitch
          v-model="draft.allowHighRiskWrites" :disabled="saving"
          label="允许 AI 修改高风险文件"
          description="开启后允许 Patch 修改 .github/**、依赖清单/锁文件、构建配置、README/LICENSE 等高风险仓库文件；删除高风险文件仍需要显式确认。.git/** 永远保持不可写。"
        />
      </div>
      <SaveBar :dirty="dirty" :busy="saving" :error="saveError" @save="requestSave"/>
    </GlassCard>
    <GlassCard class="border-[color-mix(in_srgb,var(--ui-danger)_22%,var(--ui-line))] bg-[var(--ui-danger-soft)]"><div class="flex items-center gap-4"><div class="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--ui-danger-soft)] text-[var(--ui-danger)]"><Trash2 :size="17" /></div><div class="min-w-0 flex-1"><h2 class="text-base font-semibold">删除工作区</h2><p class="mt-1 text-xs leading-5 text-[var(--ui-text-secondary)]">仅移除 Coding Tools MCP 中的配置，不会删除本地源码文件。</p></div><BaseButton variant="danger" @click="$emit('delete')"><Trash2 :size="14" />删除工作区</BaseButton></div></GlassCard>

    <ConfirmDialog
      :open="highRiskConfirmOpen"
      title="开启高风险文件写入"
      message="确定允许 AI 修改当前工作区的高风险仓库文件？"
      detail="开启后可修改 .github/**、依赖清单/锁文件、构建配置、README/LICENSE 等受保护文件；删除高风险文件仍需显式确认，.git/** 仍保持不可写。"
      confirm-text="确认开启"
      severity="danger"
      :busy="saving"
      @confirm="save"
      @cancel="highRiskConfirmOpen = false"
    />
  </div>
</template>
