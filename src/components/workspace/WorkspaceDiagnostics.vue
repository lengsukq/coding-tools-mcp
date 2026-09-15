<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Activity, RefreshCw, SquareTerminal } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import StatusPill from "../ui/StatusPill.vue";
import { runHealthChecks, type HealthItem } from "$lib/api/health";
import { readWorkspaceLogs, type LogChunk } from "$lib/api/logs";
import { showToast } from "$lib/stores/toast";

const props = defineProps<{ workspaceId: string }>();
const health = ref<HealthItem[]>([]);
const logs = ref<LogChunk[]>([]);
const healthBusy = ref(false);
const logsBusy = ref(false);

async function loadHealth() {
  healthBusy.value = true;
  try { health.value = await runHealthChecks(props.workspaceId); }
  catch (error) { showToast(String(error), { title: "健康检查失败", kind: "error" }); }
  finally { healthBusy.value = false; }
}

async function loadLogs() {
  logsBusy.value = true;
  try { logs.value = await readWorkspaceLogs(props.workspaceId, "mcp"); }
  catch (error) { showToast(String(error), { title: "读取日志失败", kind: "error" }); }
  finally { logsBusy.value = false; }
}

onMounted(() => { void loadHealth(); void loadLogs(); });
</script>

<template>
  <div class="grid gap-4 lg:grid-cols-[380px_minmax(0,1fr)]">
    <GlassCard>
      <div class="mb-4 flex items-center justify-between"><div class="flex items-center gap-2"><Activity :size="17" class="text-[#30d158]" /><div><h2 class="text-sm font-semibold">健康检查</h2><p class="text-[10px] text-[var(--text-muted)]">Workspace Context / Global MCP</p></div></div><BaseButton variant="ghost" size="sm" :busy="healthBusy" @click="loadHealth"><RefreshCw :size="13" />刷新</BaseButton></div>
      <div class="space-y-2"><div v-for="item in health" :key="item.label" class="rounded-2xl bg-black/[.025] p-3 dark:bg-white/[.04]"><div class="flex items-center justify-between gap-2"><span class="text-xs font-semibold">{{ item.label }}</span><StatusPill :status="item.ok ? 'success' : 'error'" :label="item.ok ? '正常' : '异常'" /></div><p class="mt-1.5 text-[11px] leading-5 text-[var(--text-secondary)]">{{ item.detail }}</p><p v-if="item.hint" class="mt-1 text-[10px] leading-4 text-[var(--text-muted)]">{{ item.hint }}</p></div><p v-if="!healthBusy && health.length === 0" class="py-5 text-center text-xs text-[var(--text-muted)]">暂无检查项</p></div>
    </GlassCard>
    <GlassCard class="min-w-0">
      <div class="mb-4 flex items-center justify-between"><div class="flex items-center gap-2"><SquareTerminal :size="17" class="text-[#5e5ce6]" /><div><h2 class="text-sm font-semibold">Workspace 调用日志</h2><p class="text-[10px] text-[var(--text-muted)]">仅当前项目的 MCP 调用与工具日志</p></div></div><BaseButton variant="ghost" size="sm" :busy="logsBusy" @click="loadLogs"><RefreshCw :size="13" />刷新</BaseButton></div>
      <div class="max-h-[560px] space-y-3 overflow-auto rounded-2xl bg-[#111216] p-3 font-mono text-[11px] leading-5 text-[#d8dee9]"><div v-for="chunk in logs" :key="chunk.name"><div class="mb-1 text-[10px] font-semibold uppercase tracking-[.12em] text-[#7aa2f7]">{{ chunk.name }}</div><pre class="whitespace-pre-wrap break-words">{{ chunk.content }}</pre></div><p v-if="!logsBusy && logs.length === 0" class="py-6 text-center text-[#7f8490]">暂无日志</p></div>
    </GlassCard>
  </div>
</template>
