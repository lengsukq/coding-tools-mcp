<script setup lang="ts">
import { CheckCircle2, Pin, Plus, Sparkles } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import EmptyState from "$src/components/ui/EmptyState.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import { statusPresentation } from "$lib/ui-model";
import type { DashboardAttentionItem, DashboardWorkspaceRow } from "$lib/dashboard-types";

defineProps<{
  showFocus: boolean;
  showWorkspaces: boolean;
  focused?: DashboardWorkspaceRow;
  attention: DashboardAttentionItem[];
  attentionTotal: number;
  healthState: string;
  healthLabel: string;
  healthDetail: string;
  spotlightWorkspaces: DashboardWorkspaceRow[];
  totalWorkspaces: number;
  pinnedIds: Set<string>;
  changedCount: (row: DashboardWorkspaceRow) => string | number;
}>();

const emit = defineEmits<{
  "open-path": [target: string | { path: string; query?: Record<string, string> }];
  "open-all-workspaces": [];
  "add-workspace": [];
}>();
</script>

<template>
  <div class="ui-bento-grid ui-bento-grid--compact">
    <GlassCard v-if="showFocus" variant="focal" class="flex flex-col justify-between">
      <div>
        <div class="ui-section-heading">
          <div class="inline-flex items-center gap-2">
            <span class="grid h-7 w-7 place-items-center rounded-full bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]">
              <Sparkles :size="14" />
            </span>
            <h2>当前焦点工作</h2>
          </div>
          <StatusPill
            v-if="focused"
            :status="
              focused.query.error || focused.query.stale
                ? 'unknown'
                : (focused.focus.plan?.status ?? focused.focus.goal?.status ?? focused.state?.execution.state ?? 'unknown')
            "
          />
        </div>

        <template v-if="focused">
          <div class="inline-flex items-center gap-2 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-3 py-1 text-xs text-[var(--ui-text-secondary)]">
            <strong class="font-medium text-[var(--ui-text)]">{{ focused.workspace.name }}</strong>
            <span>·</span>
            <span>{{ statusPresentation(focused.state?.mode).label }}</span>
          </div>
          <h3 class="ui-focus-title">{{ focused.focus.title }}</h3>
          <p class="ui-secondary line-clamp-2 text-sm">{{ focused.focus.detail }}</p>
          <div v-if="focused.focus.step" class="ui-inset mt-4 px-3.5 py-2.5 text-xs text-[var(--ui-text-secondary)]">
            <span class="text-[var(--ui-text-muted)]">正在推进步骤：</span>
            <strong class="font-medium text-[var(--ui-text)]">{{ focused.focus.step.title }}</strong>
          </div>
        </template>
        <EmptyState
          v-else
          title="暂无进行中的焦点任务"
          description="在任意 AI 会话中发起目标（Goal）或计划（Plan）后，这里将实时呈现执行进度与待验收节点。"
        />
      </div>

      <div v-if="focused" class="mt-5 pt-2">
        <div class="flex items-center justify-between text-xs text-[var(--ui-text-muted)]">
          <span>{{ focused.focus.progress.label }}</span>
          <span class="font-mono font-medium text-[var(--ui-text-secondary)]">{{ focused.focus.progress.percent }}%</span>
        </div>
        <div class="ui-progress" :aria-label="focused.focus.progress.label">
          <span :style="{ width: focused.focus.progress.percent + '%' }" />
        </div>
        <div class="ui-actions mt-5">
          <BaseButton @click="emit('open-path', `/workspace/${focused.workspace.id}/planning`)">查看任务详情 →</BaseButton>
          <BaseButton variant="secondary" @click="emit('open-path', `/workspace/${focused.workspace.id}/changes`)">审查代码变更</BaseButton>
        </div>
      </div>
    </GlassCard>

    <GlassCard class="flex flex-col justify-between">
      <div>
        <div class="ui-section-heading">
          <div>
            <h2>待处理与验收</h2>
            <p>阻塞项、待人工验收与连接告警始终置顶可见。</p>
          </div>
          <span
            class="shrink-0 whitespace-nowrap rounded-full px-2.5 py-0.5 text-xs font-semibold tabular-nums"
            :class="
              attentionTotal > 0
                ? 'bg-[var(--ui-warning-soft)] text-[var(--ui-warning)]'
                : 'bg-[var(--ui-success-soft)] text-[var(--ui-success)]'
            "
          >
            {{ attentionTotal }} 项
          </span>
        </div>

        <div class="space-y-1">
          <button
            v-if="healthState === 'warning' || healthState === 'error'"
            class="ui-list-action bg-[var(--ui-warning-soft)]/60"
            @click="emit('open-path', '/settings/connection?panel=health')"
          >
            <span>
              <strong>全局连接 · {{ healthLabel }}</strong>
              <small>{{ healthDetail }}</small>
            </span>
            <span>排查连接 →</span>
          </button>

          <button
            v-for="item in attention"
            :key="item.key"
            class="ui-list-action !gap-2.5"
            @click="emit('open-path', { path: item.path, query: item.query })"
          >
            <span class="min-w-0">
              <strong class="truncate text-[13px]" :title="item.title">{{ item.title }}</strong>
              <small class="truncate">{{ item.detail }}</small>
            </span>
            <span class="shrink-0">{{ item.path.endsWith("/diagnostics") ? "查看诊断" : "前往验收" }} →</span>
          </button>
        </div>

        <div
          v-if="attentionTotal === 0"
          class="ui-inset flex items-center gap-3.5 p-4 text-sm text-[var(--ui-text-secondary)]"
        >
          <div class="grid h-9 w-9 shrink-0 place-items-center rounded-full bg-[var(--ui-success-soft)] text-[var(--ui-success)]">
            <CheckCircle2 :size="18" />
          </div>
          <div>
            <strong class="block text-sm font-semibold text-[var(--ui-text)]">当前没有阻塞或待验收事项</strong>
            <span class="mt-0.5 block text-xs text-[var(--ui-text-muted)]">尚未打开的工作区按需加载，点击右上角「刷新」可同步全部状态。</span>
          </div>
        </div>
      </div>
    </GlassCard>
  </div>

  <!-- Compact Spotlight of Top 6 Workspaces -->
  <GlassCard v-if="showWorkspaces">
    <div class="ui-section-heading">
      <div>
        <h2>常用工作区速览</h2>
        <p>置顶与最近排序的前 {{ spotlightWorkspaces.length }} 个工作区；完整列表与筛选在左侧「全部工作区」。</p>
      </div>
      <BaseButton v-if="totalWorkspaces > 6" variant="secondary" size="sm" @click="emit('open-all-workspaces')">
        查看全部 {{ totalWorkspaces }} 个 →
      </BaseButton>
      <BaseButton v-else size="sm" @click="emit('add-workspace')">
        <Plus :size="14" /> 添加工作区
      </BaseButton>
    </div>

    <EmptyState v-if="!totalWorkspaces" title="还没有注册任何工作区" description="选择一个本地代码仓库目录，即可通过 MCP 跟踪 AI 修改与任务进度。">
      <BaseButton @click="emit('add-workspace')">添加第一个工作区</BaseButton>
    </EmptyState>

    <div v-else class="grid gap-3 sm:grid-cols-2">
      <button
        v-for="row in spotlightWorkspaces"
        :key="row.workspace.id"
        type="button"
        class="ui-inset group flex flex-col justify-between p-3.5 text-left transition duration-150 hover:border-[var(--ui-line-strong)] hover:bg-[var(--ui-surface-hover)]"
        @click="emit('open-path', `/workspace/${row.workspace.id}`)"
      >
        <div class="flex items-start justify-between gap-2">
          <span class="inline-flex min-w-0 items-center gap-1.5">
            <Pin v-if="pinnedIds.has(row.workspace.id)" :size="12" class="shrink-0 text-[var(--ui-accent)]" />
            <strong class="truncate text-sm font-semibold text-[var(--ui-text)] group-hover:text-[var(--ui-accent)]">
              {{ row.workspace.name }}
            </strong>
          </span>
          <span class="shrink-0 font-mono text-[11px] text-[var(--ui-accent)]">{{ changedCount(row) }} 变更</span>
        </div>
        <p class="mt-2 truncate text-xs text-[var(--ui-text-secondary)]">{{ row.focus.title }}</p>
      </button>
    </div>
  </GlassCard>
</template>
