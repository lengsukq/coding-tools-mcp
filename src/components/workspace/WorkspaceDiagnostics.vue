<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { ClipboardList, RefreshCw, SquareTerminal } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import { readWorkspaceLogs, type LogChunk } from "$lib/api/logs";
import { showToast } from "$lib/stores/toast";

const props = defineProps<{ workspaceId: string }>();
const router = useRouter();
const logs = ref<LogChunk[]>([]);
const logsBusy = ref(false);

async function loadLogs() {
  logsBusy.value = true;
  try { logs.value = await readWorkspaceLogs(props.workspaceId, "mcp"); }
  catch (error) { showToast(String(error), { title: "读取日志失败", kind: "error" }); }
  finally { logsBusy.value = false; }
}

function openAudit() {
  void router.push({ path: "/audit", query: { workspaceId: props.workspaceId } });
}

onMounted(() => { void loadLogs(); });
</script>

<template>
  <div class="grid gap-4">
    <GlassCard class="min-w-0">
      <div class="mb-4 flex flex-wrap items-center justify-between gap-2"><div class="flex items-center gap-2"><SquareTerminal :size="17" class="text-[var(--primary)]" /><div><h2 class="text-sm font-semibold">Workspace 调用日志</h2><p class="text-[10px] text-[var(--text-muted)]">仅当前项目的 MCP 调用与工具日志</p></div></div><div class="flex items-center gap-1"><BaseButton variant="ghost" size="sm" @click="openAudit"><ClipboardList :size="13" />审计记录</BaseButton><BaseButton variant="ghost" size="sm" :busy="logsBusy" @click="loadLogs"><RefreshCw :size="13" />刷新</BaseButton></div></div>
      <div class="max-h-[560px] space-y-3 overflow-auto rounded-2xl bg-[#111216] p-3 font-mono text-[11px] leading-5 text-[#d8dee9]"><div v-for="chunk in logs" :key="chunk.name"><div class="mb-1 text-[10px] font-semibold uppercase tracking-[.12em] text-[#7aa2f7]">{{ chunk.name }}</div><pre class="whitespace-pre-wrap break-words">{{ chunk.content }}</pre></div><p v-if="!logsBusy && logs.length === 0" class="py-6 text-center text-[#7f8490]">暂无日志</p></div>
    </GlassCard>
  </div>
</template>
