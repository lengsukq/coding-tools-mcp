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
const error=ref("");

async function loadLogs() {
  if(logsBusy.value)return;
  error.value="";logsBusy.value = true;
  try { logs.value = await readWorkspaceLogs(props.workspaceId, "mcp"); }
  catch (e) { error.value=String(e); }
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
      <div class="mb-4 flex flex-wrap items-center justify-between gap-2"><div class="flex items-center gap-2"><SquareTerminal :size="17" class="text-[var(--ui-accent)]" /><div><h2 class="text-base font-semibold">工作区调用日志</h2><p class="text-xs text-[var(--ui-text-muted)]">仅当前项目的 MCP 调用与工具日志</p></div></div><div class="flex items-center gap-1"><BaseButton variant="ghost" size="sm" @click="openAudit"><ClipboardList :size="13" />审计记录</BaseButton><BaseButton variant="ghost" size="sm" :busy="logsBusy" @click="loadLogs"><RefreshCw :size="13" />刷新</BaseButton></div></div>
      <p v-if="error" class="ui-danger mb-4" role="alert">读取失败：{{error}}。点击刷新重试。</p><div class="max-h-[560px] space-y-3 overflow-auto rounded-lg bg-[var(--ui-log-bg)] p-3 font-mono text-xs leading-5 text-[var(--ui-log-text)]"><div v-for="chunk in logs" :key="chunk.name"><div class="mb-1 text-xs font-semibold text-[var(--ui-log-text)]">{{ chunk.name }}</div><pre class="whitespace-pre-wrap break-words">{{ chunk.content }}</pre></div><p v-if="!logsBusy && !error && logs.length === 0" class="py-6 text-center text-[var(--ui-log-text)]">暂无日志</p></div>
    </GlassCard>
  </div>
</template>
