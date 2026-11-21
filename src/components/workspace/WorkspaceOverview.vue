<script setup lang="ts">
import { useRouter } from "vue-router";
import { computed, onMounted, ref } from "vue";
import { Code2, GitBranch, RefreshCw, Sparkles } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import StatusPill from "../ui/StatusPill.vue";
import EmptyState from "../ui/EmptyState.vue";
import {
  getCachedDetectedIdes,
  getCachedWorkspaceActivityMetrics,
  globalResourceState,
  peekCachedGlobalMcpOverview,
  peekCachedWorkspaceActivityMetrics,
  useWorkspaceStatus,
} from "$lib/workspace-status-cache";
import { openWorkspaceInIde, type DetectedIdeDto } from "$lib/api/workspaces";
import { showToast } from "$lib/stores/toast";
import { statusPresentation, taskFocus } from "$lib/ui-model";
import type { WorkspaceProfile } from "$lib/types";
import type { WorkspaceTab } from "$lib/workspace-page";

const router = useRouter();
const props = defineProps<{
  workspaceId: string;
  profile: WorkspaceProfile;
}>();
const emit = defineEmits<{
  navigate: [tab: WorkspaceTab];
}>();

const { planning, git, query, gitQuery, refresh } = useWorkspaceStatus(() => props.workspaceId, { git: true, global: true });
const focus = computed(() => taskFocus(planning.value));
const ides = ref<DetectedIdeDto[]>([]);
const globalQuery = computed(globalResourceState);
const activity = computed(() => peekCachedWorkspaceActivityMetrics(props.workspaceId));
const activityError = ref("");

async function loadActivity() {
  try {
    await getCachedWorkspaceActivityMetrics(props.workspaceId, true);
    activityError.value = "";
  } catch (e) {
    activityError.value = String(e);
  }
}

const mcp = computed(() => peekCachedGlobalMcpOverview());
const changes = computed(() =>
  git.value ? (git.value.available ? git.value.changedFiles : 0) + git.value.subRepositories.reduce((n, r) => n + r.changedFiles, 0) : null,
);
const pending = computed(() => [...(planning.value?.plans ?? []), ...(planning.value?.goals ?? [])].filter((i) => i.status === "awaiting_acceptance"));

async function openIde(id: string) {
  try {
    await openWorkspaceInIde(props.workspaceId, id);
  } catch (e) {
    showToast(String(e), { kind: "error" });
  }
}

onMounted(() => {
  void getCachedDetectedIdes()
    .then((v) => (ides.value = v))
    .catch(() => {});
});
</script>

<template>
  <div class="ui-stack">
    <div v-if="query.error || query.stale || gitQuery.error" class="ui-banner ui-banner--warning" role="status">
      <span>摘要{{ query.error || gitQuery.error ? "更新失败" : "已过期" }}，以下为上次数据。{{ query.error || gitQuery.error }}</span>
      <BaseButton variant="secondary" size="sm" @click="refresh">重试</BaseButton>
    </div>

    <GlassCard v-if="pending.length || planning?.execution.last_error">
      <div class="ui-section-heading">
        <div>
          <h2>需要优先处理</h2>
          <p>当前项目存在待人工验收或执行异常。</p>
        </div>
      </div>
      <div class="space-y-1">
        <button
          v-for="item in pending"
          :key="item.id"
          class="ui-list-action"
          @click="router.push({ path: `/workspace/${workspaceId}/planning`, query: { item: item.id } })"
        >
          <span>
            <strong>待验收 · {{ item.title }}</strong>
            <small>核对变更与验证证据后确认完成</small>
          </span>
          <span>前往验收 →</span>
        </button>
        <button v-if="planning?.execution.last_error" class="ui-list-action" @click="emit('navigate', 'diagnostics')">
          <span>
            <strong>最近执行异常</strong>
            <small>{{ planning.execution.last_error }}</small>
          </span>
          <span>查看诊断 →</span>
        </button>
      </div>
    </GlassCard>

    <GlassCard variant="focal">
      <div class="ui-section-heading">
        <div class="inline-flex items-center gap-2">
          <span class="grid h-7 w-7 place-items-center rounded-full bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]">
            <Sparkles :size="14" />
          </span>
          <h2>当前焦点任务</h2>
        </div>
        <StatusPill
          v-if="focus.plan || focus.goal"
          :status="query.error || query.stale ? 'unknown' : (focus.plan?.status ?? focus.goal?.status)"
        />
        <StatusPill
          v-else
          :status="query.error || query.stale ? 'unknown' : (planning?.execution.state === 'blocked' || planning?.execution.state === 'failed' ? planning.execution.state : 'idle')"
          :label="query.error || query.stale ? undefined : (planning?.execution.state === 'blocked' || planning?.execution.state === 'failed' ? undefined : '空闲')"
        />
      </div>

      <template v-if="focus.plan || focus.goal">
        <h3 class="ui-focus-title">{{ focus.title }}</h3>
        <p class="ui-secondary line-clamp-2 text-sm">{{ focus.detail }}</p>
        <div v-if="focus.step" class="ui-inset mt-4 px-3.5 py-2.5 text-xs text-[var(--ui-text-secondary)]">
          <span class="text-[var(--ui-text-muted)]">当前步骤：</span>
          <strong class="font-medium text-[var(--ui-text)]">{{ focus.step.title }}</strong>
        </div>
        <div class="mt-4 flex items-center justify-between text-xs text-[var(--ui-text-muted)]">
          <span>{{ focus.progress.label }}</span>
          <span class="font-mono font-medium text-[var(--ui-text-secondary)]">{{ focus.progress.percent }}%</span>
        </div>
        <div class="ui-progress">
          <span :style="{ width: focus.progress.percent + '%' }" />
        </div>
        <div class="ui-actions mt-5">
          <BaseButton @click="emit('navigate', 'planning')">查看任务详情 →</BaseButton>
          <BaseButton variant="secondary" @click="emit('navigate', 'changes')">查看变更与证据</BaseButton>
        </div>
      </template>
      <div v-else class="ui-inset flex items-center justify-between gap-4 px-4 py-3.5 text-xs">
        <div class="min-w-0">
          <strong class="block text-sm font-semibold text-[var(--ui-text)]">{{ query.loading ? "正在加载任务" : "暂无当前任务" }}</strong>
          <p class="mt-0.5 text-[var(--ui-text-muted)]">通过 AI 会话发起目标或计划后，可以在这里实时跟踪步骤进度与验收。</p>
        </div>
        <BaseButton variant="secondary" size="sm" @click="emit('navigate', 'planning')">查看任务列表</BaseButton>
      </div>
    </GlassCard>

    <div class="ui-main-grid">
      <GlassCard>
        <div class="ui-section-heading">
          <div>
            <h2>代码与活跃会话</h2>
            <p>Git 工作树状态与当前绑定到此工作区的 MCP 会话。</p>
          </div>
          <BaseButton variant="ghost" size="sm" @click="refresh">
            <RefreshCw :size="13" /> 刷新
          </BaseButton>
        </div>

        <div class="ui-inset mb-3.5 flex items-center justify-between gap-3 px-3.5 py-2.5">
          <div class="flex min-w-0 items-center gap-2.5">
            <span class="grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]">
              <GitBranch :size="14" />
            </span>
            <div class="min-w-0">
              <span class="block text-[11px] text-[var(--ui-text-muted)]">当前分支</span>
              <strong
                class="block truncate font-mono text-sm font-semibold text-[var(--ui-text)]"
                :title="git?.available ? git.branch || '分离 HEAD' : undefined"
              >
                {{ git?.available ? git.branch || "分离 HEAD" : git?.subRepositories.length ? `${git.subRepositories.length} 个子仓库` : "非 Git 仓库" }}
              </strong>
            </div>
          </div>
          <span
            v-if="git?.available && (git.ahead > 0 || git.behind > 0)"
            class="shrink-0 whitespace-nowrap rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface)] px-2.5 py-0.5 font-mono text-[11px] text-[var(--ui-text-secondary)]"
          >
            ↑{{ git.ahead }} ↓{{ git.behind }}
          </span>
        </div>

        <div class="grid grid-cols-3 gap-3">
          <button class="ui-metric-action" @click="emit('navigate', 'changes')">
            <span class="block whitespace-nowrap text-xs text-[var(--ui-text-muted)]">变更文件</span>
            <strong class="mt-1.5 block text-2xl font-semibold tabular-nums leading-tight">{{ changes ?? "—" }}</strong>
          </button>
          <div class="ui-inset px-4 py-3.5">
            <span class="block whitespace-nowrap text-xs text-[var(--ui-text-muted)]">活跃会话</span>
            <strong class="mt-1.5 block text-2xl font-semibold tabular-nums leading-tight">{{ mcp?.sessions.filter((s) => s.workspaceId === workspaceId).length ?? "—" }}</strong>
          </div>
          <button class="ui-metric-action" @click="emit('navigate', 'planning')">
            <span class="block whitespace-nowrap text-xs text-[var(--ui-text-muted)]">验证记录</span>
            <strong class="mt-1.5 block text-2xl font-semibold tabular-nums leading-tight">{{ planning?.execution.verification.length ?? "—" }}</strong>
          </button>
        </div>

        <p v-if="git?.lastCommit" class="ui-inset mt-3.5 truncate px-3.5 py-2.5 font-mono text-xs text-[var(--ui-text-secondary)]" :title="git.lastCommit">
          最近提交 · {{ git.lastCommit }}
        </p>

        <div v-if="ides.length" class="ui-actions mt-4">
          <BaseButton v-for="ide in ides" :key="ide.id" variant="secondary" size="sm" @click="openIde(ide.id)">
            <Code2 :size="13" /> 使用 {{ ide.name }} 打开
          </BaseButton>
        </div>

        <details v-if="git?.subRepositories.length" class="ui-disclosure mt-4">
          <summary>子仓库状态 · {{ git.subRepositories.length }}</summary>
          <div class="ui-disclosure-body">
            <div v-for="repo in git.subRepositories" :key="repo.path" class="ui-data-row text-xs">
              <span>{{ repo.name }} · {{ repo.branch || "分离 HEAD" }}</span>
              <span class="font-mono">{{ repo.changedFiles }} 个变更 · ↑{{ repo.ahead }} ↓{{ repo.behind }}</span>
            </div>
          </div>
        </details>
      </GlassCard>

      <GlassCard>
        <div class="ui-section-heading">
          <div>
            <h2>执行策略概览</h2>
            <p>当前生效的权限与记录模式。</p>
          </div>
          <BaseButton variant="secondary" size="sm" @click="emit('navigate', 'settings')">调整配置</BaseButton>
        </div>
        <p v-if="globalQuery.error || globalQuery.stale" class="ui-muted mb-2">全局运行摘要待更新。</p>
        <div class="policy-overview-grid">
          <div class="ui-data-row text-sm">
            <span class="text-[var(--ui-text-secondary)]">全局 MCP 服务</span>
            <StatusPill :status="globalQuery.error || globalQuery.stale ? 'unknown' : mcp?.state" />
          </div>
          <div class="ui-data-row text-sm">
            <span class="text-[var(--ui-text-secondary)]">执行模式</span>
            <strong class="font-medium">{{ statusPresentation(planning?.mode).label }}</strong>
          </div>
          <div class="ui-data-row text-sm">
            <span class="text-[var(--ui-text-secondary)]">工具档位</span>
            <strong class="font-medium">{{ statusPresentation(profile.runtime.tool_profile).label }}</strong>
          </div>
          <div class="ui-data-row text-sm">
            <span class="text-[var(--ui-text-secondary)]">权限来源</span>
            <strong class="font-medium">{{ profile.runtime.inherit_global_execution_policy ? "继承全局" : statusPresentation(profile.runtime.permission_mode).label }}</strong>
          </div>
          <div class="ui-data-row text-sm">
            <span class="text-[var(--ui-text-secondary)]">开发历史记录</span>
            <strong class="font-medium">{{ profile.runtime.history_recording === false ? "关闭" : "开启" }}</strong>
          </div>
        </div>
        <p class="ui-muted mt-3.5 leading-relaxed">验证记录是 AI 提交的执行证据，请结合代码 Diff 与验收条件完成最终判断。</p>
      </GlassCard>
    </div>

    <details class="ui-disclosure" @toggle="($event.target as HTMLDetailsElement).open && loadActivity()">
      <summary>累计变更活动统计 <span class="ui-muted">按需读取</span></summary>
      <div class="ui-disclosure-body">
        <p v-if="activityError" class="ui-danger text-xs" role="alert">更新失败：{{ activityError }}</p>
        <div v-if="activity" class="ui-metric-grid">
          <div>
            <span>记录操作数</span>
            <strong>{{ activity.totalOperations }}</strong>
          </div>
          <div>
            <span>涉及文件数</span>
            <strong>{{ activity.totalFiles }}</strong>
          </div>
          <div>
            <span>累计新增行</span>
            <strong class="text-[var(--ui-success)]">+{{ activity.totalAdditions }}</strong>
          </div>
          <div>
            <span>累计删除行</span>
            <strong class="text-[var(--ui-danger)]">−{{ activity.totalDeletions }}</strong>
          </div>
        </div>
        <EmptyState v-else title="暂无活动统计" description="启用历史记录后的 AI 修改操作会汇总在此处。" />
      </div>
    </details>
  </div>
</template>

<style scoped>
.policy-overview-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 0 24px;
}

@container subnav-stage (min-width: 560px) and (max-width: 820px) {
  .policy-overview-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
