<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderInput, Save, Trash2 } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import TextField from "../ui/TextField.vue";
import ChatGptSessionPrompt from "./ChatGptSessionPrompt.vue";
import { showToast } from "$lib/stores/toast";
import type { WorkspaceProfile } from "$lib/types";

const props = defineProps<{
  profile: WorkspaceProfile;
  onSave: (draft: { name: string; path: string }) => Promise<void>;
}>();
defineEmits<{ delete: [] }>();
const draft = reactive({ name: props.profile.name, path: props.profile.path });
const saving = ref(false);
watch(() => props.profile, (value) => { draft.name = value.name; draft.path = value.path; }, { deep: true });
const dirty = computed(() => draft.name.trim() !== props.profile.name || draft.path !== props.profile.path);

async function choosePath() {
  const result = await open({ directory: true, multiple: false, defaultPath: draft.path });
  if (result && !Array.isArray(result)) draft.path = result;
}
async function save() {
  if (saving.value || !dirty.value) return;
  const name = draft.name.trim();
  if (!name) {
    showToast("工作区名称不能为空", { kind: "warning" });
    return;
  }
  saving.value = true;
  try {
    await props.onSave({ name, path: draft.path });
    draft.name = name;
    showToast("工作区信息已保存", { kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "保存工作区失败", kind: "error" });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="grid gap-4">
    <ChatGptSessionPrompt />
    <GlassCard><div class="mb-4"><h2 class="text-sm font-semibold">工作区信息</h2><p class="mt-1 text-[11px] text-[var(--text-muted)]">名称只影响桌面端显示；目录变化后运行中的 MCP 需要重启。</p></div><div class="grid gap-4"><TextField v-model="draft.name" label="工作区名称" /><div><TextField v-model="draft.path" label="项目目录" /><BaseButton class="mt-2" variant="secondary" size="sm" @click="choosePath"><FolderInput :size="13" />重新选择目录</BaseButton></div><div class="flex justify-end"><BaseButton :busy="saving" :disabled="!dirty" @click="save"><Save :size="14" />保存修改</BaseButton></div></div></GlassCard>
    <GlassCard class="border-[#ff375f]/16 bg-[#ff375f]/[.045]"><div class="flex items-center gap-4"><div class="flex h-10 w-10 items-center justify-center rounded-2xl bg-[#ff375f]/12 text-[#ff375f]"><Trash2 :size="17" /></div><div class="min-w-0 flex-1"><h2 class="text-sm font-semibold">删除工作区</h2><p class="mt-1 text-[11px] leading-5 text-[var(--text-secondary)]">仅移除 Coding Tools MCP 中的配置，不会删除本地源码文件。</p></div><BaseButton variant="danger" @click="$emit('delete')"><Trash2 :size="14" />删除工作区</BaseButton></div></GlassCard>
  </div>
</template>
