<script setup lang="ts">
import { onMounted, ref } from "vue";
import { History, Save } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import ToggleSwitch from "../ui/ToggleSwitch.vue";
import { listHistorySessions, type HistorySessionSummary } from "$lib/api/history";
import { showToast } from "$lib/stores/toast";
import type { WorkspaceProfile } from "$lib/types";

const props = defineProps<{ workspaceId: string; profile: WorkspaceProfile }>();
const emit = defineEmits<{ save: [recording: boolean, sessions: number[]] }>();
const loading = ref(false);
const saving = ref(false);
const sessions = ref<HistorySessionSummary[]>([]);
const recording = ref(props.profile.runtime.history_recording ?? true);
const selected = ref<number[]>([...(props.profile.runtime.history_context_sessions ?? [])]);

async function load() {
  loading.value = true;
  try { sessions.value = (await listHistorySessions(props.workspaceId)).sessions; }
  catch (error) { showToast(String(error), { title: "加载历史会话失败", kind: "error" }); }
  finally { loading.value = false; }
}

function toggle(number: number, checked: boolean) {
  selected.value = checked ? [...new Set([...selected.value, number])] : selected.value.filter((item) => item !== number);
}

async function save() {
  saving.value = true;
  try { emit("save", recording.value, selected.value); showToast("历史上下文配置已保存", { kind: "success" }); }
  finally { saving.value = false; }
}

onMounted(() => { void load(); });
</script>

<template>
  <div class="grid gap-4">
    <ToggleSwitch v-model="recording" label="记录 History Session" description="把开发过程写入项目内 docs/history-session，便于跨会话恢复。" />
    <div>
      <div class="mb-2 flex items-center justify-between"><div><p class="text-xs font-semibold">注入到当前上下文的历史会话</p><p class="mt-0.5 text-[11px] text-[var(--text-muted)]">只选择确实需要长期带入的会话，避免上下文膨胀。</p></div><History :size="17" class="text-[#5e5ce6]" /></div>
      <div class="max-h-[320px] space-y-2 overflow-y-auto rounded-2xl bg-black/[.025] p-2 dark:bg-white/[.04]">
        <label v-for="session in sessions" :key="session.number" class="flex cursor-pointer gap-3 rounded-xl p-2.5 transition hover:bg-white/55 dark:hover:bg-white/5"><input type="checkbox" class="mt-1 accent-[#0a84ff]" :checked="selected.includes(session.number)" @change="toggle(session.number, ($event.target as HTMLInputElement).checked)" /><span class="min-w-0"><span class="block truncate text-xs font-semibold">#{{ session.number }} {{ session.title || '会话' }}</span><span class="mt-0.5 block line-clamp-2 text-[10px] leading-4 text-[var(--text-muted)]">{{ session.latest_focus || session.path }}</span></span></label>
        <p v-if="!loading && sessions.length === 0" class="py-5 text-center text-xs text-[var(--text-muted)]">暂无历史会话</p>
      </div>
    </div>
    <div class="flex justify-end"><BaseButton :busy="saving" @click="save"><Save :size="14" />保存 History</BaseButton></div>
  </div>
</template>
