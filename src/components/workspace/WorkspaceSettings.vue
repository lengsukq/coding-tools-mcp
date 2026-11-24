<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderInput, ShieldAlert, Trash2 } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import ConfirmDialog from "../ui/ConfirmDialog.vue";
import GlassCard from "../ui/GlassCard.vue";
import TextField from "../ui/TextField.vue";
import ToggleSwitch from "../ui/ToggleSwitch.vue";
import ChatGptSessionPrompt from "./ChatGptSessionPrompt.vue";
import SaveBar from "../ui/SaveBar.vue";
import { useUnsavedChanges } from "$lib/unsaved-changes";
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
const saveError = ref("");

watch(
  () => [props.profile.name, props.profile.path, props.profile.runtime.allow_high_risk_writes] as const,
  ([name, path, allowHighRiskWrites]) => {
    if (dirty.value && !saving.value) return;
    draft.name = name;
    draft.path = path;
    draft.allowHighRiskWrites = allowHighRiskWrites ?? false;
  },
);

const infoDirty = computed(() => draft.name.trim() !== props.profile.name || draft.path !== props.profile.path);
const securityDirty = computed(() => draft.allowHighRiskWrites !== (props.profile.runtime.allow_high_risk_writes ?? false));
const dirty = computed(() => infoDirty.value || securityDirty.value);
const enablingHighRiskWrites = computed(() => draft.allowHighRiskWrites && !(props.profile.runtime.allow_high_risk_writes ?? false));

async function choosePath() {
  try {
    const result = await open({ directory: true, multiple: false, defaultPath: draft.path });
    if (result && !Array.isArray(result)) draft.path = result;
  } catch (error) {
    showToast(String(error), { title: "选择目录失败", kind: "error" });
  }
}

async function save() {
  if (saving.value || !dirty.value) return false;
  const name = draft.name.trim();
  if (!name) {
    saveError.value = "工作区名称不能为空";
    return false;
  }
  if (!draft.path.trim()) {
    saveError.value = "请选择项目目录";
    return false;
  }
  saveError.value = "";
  saving.value = true;
  try {
    await props.onSave({ name, path: draft.path, allowHighRiskWrites: draft.allowHighRiskWrites });
    draft.name = name;
    highRiskConfirmOpen.value = false;
    showToast("工作区设置已保存", { kind: "success" });
    return true;
  } catch (error) {
    saveError.value = String(error);
    highRiskConfirmOpen.value = false;
    return false;
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

useUnsavedChanges("工作区信息与仓库保护", dirty, async () => {
  if (enablingHighRiskWrites.value) {
    throw new Error("请取消离开，在工作区设置中确认开启高风险文件写入后再保存。");
  }
  return save();
});
</script>

<template>
  <div class="ui-stack animate-fade-in-up">
    <GlassCard>
      <ChatGptSessionPrompt />
    </GlassCard>

    <GlassCard>
      <div class="ui-section-heading">
        <div>
          <h2>基本信息与仓库保护</h2>
          <p>工作区显示名称、本地路径及受保护配置文件的写入门禁。</p>
        </div>
      </div>

      <div class="space-y-4">
        <div class="ui-two-col items-end">
          <TextField
            v-model="draft.name"
            label="工作区显示名称"
            :disabled="saving"
            :error="saveError && !draft.name.trim() ? saveError : undefined"
          />
          <div class="flex items-end gap-2">
            <div class="min-w-0 flex-1">
              <TextField v-model="draft.path" label="本地项目根目录" :disabled="saving" />
            </div>
            <BaseButton class="mb-0.5" :disabled="saving" variant="secondary" @click="choosePath">
              <FolderInput :size="14" /> 选择目录
            </BaseButton>
          </div>
        </div>

        <div class="ui-inset p-4">
          <div class="mb-2 flex items-center gap-1.5 text-xs font-semibold text-[var(--ui-warning)]">
            <ShieldAlert :size="14" /> 关键配置文件保护策略
          </div>
          <ToggleSwitch
            v-model="draft.allowHighRiskWrites"
            :disabled="saving"
            label="允许 AI 修改高风险仓库文件"
            description="开启后允许 Patch 修改 .github/**、依赖清单/锁文件、构建配置、README/LICENSE 等受保护文件；删除高风险文件仍需确认，.git/** 永远不可写。"
          />
        </div>

        <SaveBar :dirty="dirty" :busy="saving" :error="saveError" @save="requestSave" />
      </div>
    </GlassCard>

    <GlassCard class="border-[color-mix(in_srgb,var(--ui-danger)_24%,var(--ui-line))]">
      <div class="flex flex-wrap items-center justify-between gap-4">
        <div class="flex min-w-0 items-center gap-3.5">
          <div class="grid h-10 w-10 shrink-0 place-items-center rounded-full bg-[var(--ui-danger-soft)] text-[var(--ui-danger)]">
            <Trash2 :size="17" />
          </div>
          <div class="min-w-0">
            <h2 class="text-base font-semibold">移除工作区</h2>
            <p class="mt-0.5 text-xs leading-5 text-[var(--ui-text-secondary)]">仅从 Coding Tools MCP 注册表中移除此工作区，磁盘上的源代码与 Git 仓库完好保留。</p>
          </div>
        </div>
        <BaseButton variant="danger" size="sm" @click="$emit('delete')">
          <Trash2 :size="14" /> 移除工作区
        </BaseButton>
      </div>
    </GlassCard>

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
