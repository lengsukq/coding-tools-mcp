<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { ClipboardList, RefreshCw, SquareTerminal } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import { readWorkspaceLogs, type LogChunk } from "$lib/api/logs";
import { useWorkspaceStatus } from "$lib/workspace-status-cache";
import StatusPill from "../ui/StatusPill.vue";

const props = defineProps<{ workspaceId: string }>();
const router = useRouter();
const { planning, query, refresh } = useWorkspaceStatus(() => props.workspaceId);
const logs = ref<LogChunk[]>([]);
const logsBusy = ref(false);
const error = ref("");

async function loadLogs() {
  if (logsBusy.value) return;
  error.value = "";
  logsBusy.value = true;
  try {
    logs.value = await readWorkspaceLogs(props.workspaceId, "mcp");
  } catch (e) {
    error.value = String(e);
  } finally {
    logsBusy.value = false;
  }
}

function openAudit() {
  void router.push({ path: "/audit", query: { workspaceId: props.workspaceId } });
}

onMounted(() => {
  void loadLogs();
});
</script>

<template>
  <div class="ui-stack">
    <GlassCard v-if="planning?.execution.last_error || ['blocked', 'failed', 'error'].includes(planning?.execution.state ?? '')">
      <div class="ui-section-heading">
        <div>
          <h2>当前执行阻塞异常</h2>
          <p>结合下方日志与全局工具审计定位根因。</p>
        </div>
        <StatusPill :status="query.stale || query.error ? 'stale' : planning?.execution.state" />
      </div>
      <div class="ui-banner ui-banner--danger text-xs">
        <p class="whitespace-pre-wrap break-words" role="alert">{{ planning?.execution.last_error || "执行已阻塞，请查看下方日志与工具审计了解原因。" }}</p>
      </div>
      <p v-if="query.error || query.stale" class="ui-muted mt-3">{{ query.error || "以上为上次缓存数据，正在更新。" }}</p>
      <div class="ui-actions mt-4">
        <BaseButton variant="secondary" size="sm" @click="refresh">刷新执行状态</BaseButton>
        <BaseButton
          v-if="planning?.execution.plan_id || planning?.execution.goal_id"
          variant="ghost"
          size="sm"
          @click="router.push({ path: `/workspace/${workspaceId}/planning`, query: { item: planning?.execution.plan_id || planning?.execution.goal_id } })"
        >
          查看关联任务
        </BaseButton>
      </div>
    </GlassCard>

    <GlassCard class="min-w-0">
      <div class="ui-section-heading flex-wrap">
        <div class="flex items-center gap-3">
          <div class="grid h-9 w-9 shrink-0 place-items-center rounded-[12px] bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]">
            <SquareTerminal :size="17" />
          </div>
          <div>
            <h2>工作区运行与调用日志</h2>
            <p>仅展示当前项目作用域下的 MCP 请求与命令输出。</p>
          </div>
        </div>
        <div class="ui-actions">
          <BaseButton variant="secondary" size="sm" @click="openAudit">
            <ClipboardList :size="13" /> 查看工具审计
          </BaseButton>
          <BaseButton variant="ghost" size="sm" :busy="logsBusy" @click="loadLogs">
            <RefreshCw v-if="!logsBusy" :size="13" /> 刷新日志
          </BaseButton>
        </div>
      </div>

      <p v-if="error" class="ui-danger mb-4 text-xs" role="alert">读取日志失败：{{ error }}。请点击刷新重试。</p>

      <div class="overflow-hidden rounded-[20px] border border-white/10 bg-[var(--ui-log-bg)] shadow-inner">
        <div class="flex items-center justify-between border-b border-white/10 px-4 py-2.5 text-[11px] text-slate-400">
          <div class="flex items-center gap-1.5">
            <span class="h-2.5 w-2.5 rounded-full bg-rose-500/70" />
            <span class="h-2.5 w-2.5 rounded-full bg-amber-400/70" />
            <span class="h-2.5 w-2.5 rounded-full bg-emerald-500/70" />
            <span class="ml-2 font-mono">mcp-workspace-logs</span>
          </div>
          <span class="font-mono">{{ logs.length }} 个日志文件</span>
        </div>

        <div class="min-h-[340px] max-h-[58dvh] space-y-4 overflow-auto p-4 font-mono text-xs leading-5 text-[var(--ui-log-text)]">
          <div v-for="chunk in logs" :key="chunk.name" class="space-y-1.5">
            <div class="inline-block rounded-full bg-white/10 px-2.5 py-0.5 text-[11px] font-semibold text-sky-300">{{ chunk.name }}</div>
            <pre class="whitespace-pre-wrap break-words pl-1 text-[11.5px] leading-relaxed opacity-90">{{ chunk.content }}</pre>
          </div>
          <p v-if="!logsBusy && !error && logs.length === 0" class="py-10 text-center text-slate-400">当前工作区暂无日志记录</p>
        </div>
      </div>
    </GlassCard>
  </div>
</template>
