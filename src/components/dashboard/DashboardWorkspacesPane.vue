<script setup lang="ts">
import { GitBranch, LayoutGrid, List, Pin, Plus, Search, X } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import EmptyState from "$src/components/ui/EmptyState.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import WorkspaceRowMenu from "./WorkspaceRowMenu.vue";
import type { DashboardWorkspaceRow } from "$lib/dashboard-types";

const workspaceLayout = defineModel<"cards" | "table">("layout", { required: true });
const workspaceSearch = defineModel<string>("search", { required: true });
const workspaceScope = defineModel<"all" | "pinned" | "changed">("scope", { required: true });

defineProps<{
  totalCount: number;
  filteredWorkspaces: DashboardWorkspaceRow[];
  orderedIndexById: Map<string, number>;
  pinnedIds: Set<string>;
  changedCount: (row: DashboardWorkspaceRow) => string | number;
}>();

const emit = defineEmits<{
  "open-path": [path: string];
  "add-workspace": [];
  "toggle-pin": [workspaceId: string];
  move: [workspaceId: string, direction: -1 | 1];
  reveal: [path: string];
  copy: [path: string];
}>();
</script>

<template>
  <GlassCard>
    <div class="ui-section-heading flex-wrap">
      <div>
        <h2>全部工作区矩阵</h2>
        <p>支持按名称/路径搜索、置顶与变更状态筛选，独立隔离各项目的执行策略。</p>
      </div>
      <div class="ui-actions">
        <div class="ui-options" role="group" aria-label="工作区范围筛选">
          <button type="button" class="ui-options__item" :class="{ active: workspaceScope === 'all' }" @click="workspaceScope = 'all'">
            全部 ({{ totalCount }})
          </button>
          <button type="button" class="ui-options__item" :class="{ active: workspaceScope === 'pinned' }" @click="workspaceScope = 'pinned'">
            已置顶
          </button>
          <button type="button" class="ui-options__item" :class="{ active: workspaceScope === 'changed' }" @click="workspaceScope = 'changed'">
            有变更
          </button>
        </div>
        <div v-if="totalCount" class="ui-options" role="group" aria-label="工作区视图切换">
          <button
            type="button"
            class="ui-options__item inline-flex items-center gap-1.5"
            :class="{ active: workspaceLayout === 'cards' }"
            @click="workspaceLayout = 'cards'"
          >
            <LayoutGrid :size="13" /> 卡片
          </button>
          <button
            type="button"
            class="ui-options__item inline-flex items-center gap-1.5"
            :class="{ active: workspaceLayout === 'table' }"
            @click="workspaceLayout = 'table'"
          >
            <List :size="13" /> 列表
          </button>
        </div>
        <BaseButton size="sm" @click="emit('add-workspace')">
          <Plus :size="14" /> 添加工作区
        </BaseButton>
      </div>
    </div>

    <div v-if="totalCount" class="relative mb-4">
      <Search :size="15" class="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
      <input
        v-model="workspaceSearch"
        type="text"
        aria-label="搜索工作区"
        placeholder="输入工作区名称、本地路径或当前任务关键词…"
        class="h-10 w-full rounded-full border border-[var(--ui-line-strong)] bg-[var(--ui-surface-subtle)] pl-10 pr-8 text-sm text-[var(--ui-text)] outline-none transition focus:border-[var(--ui-accent)] focus:bg-[var(--ui-surface-raised)] focus:shadow-[var(--focus-ring)]"
      />
      <button
        v-if="workspaceSearch"
        type="button"
        class="absolute right-3 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)] hover:text-[var(--ui-text)]"
        aria-label="清空搜索"
        @click="workspaceSearch = ''"
      >
        <X :size="14" />
      </button>
    </div>

    <EmptyState
      v-if="!filteredWorkspaces.length"
      :title="totalCount ? '没有匹配筛选条件的工作区' : '还没有注册任何工作区'"
      :description="totalCount ? '请清空搜索关键词或切换上方范围筛选。' : '选择一个本地代码仓库目录，即可通过 MCP 跟踪 AI 修改与任务进度。'"
    >
      <BaseButton v-if="!totalCount" @click="emit('add-workspace')">添加第一个工作区</BaseButton>
      <BaseButton v-else variant="secondary" @click="workspaceSearch = ''; workspaceScope = 'all'">重置筛选</BaseButton>
    </EmptyState>

    <div v-else-if="workspaceLayout === 'cards'" class="grid gap-3.5 md:grid-cols-2">
      <div
        v-for="row in filteredWorkspaces"
        :key="row.workspace.id"
        class="ui-inset group flex flex-col justify-between p-4 transition duration-150 hover:border-[var(--ui-line-strong)] hover:bg-[var(--ui-surface-hover)]"
      >
        <div>
          <div class="flex items-start justify-between gap-2">
            <button
              type="button"
              class="min-w-0 flex-1 text-left"
              @click="emit('open-path', `/workspace/${row.workspace.id}`)"
            >
              <span class="inline-flex max-w-full items-center gap-1.5">
                <Pin v-if="pinnedIds.has(row.workspace.id)" :size="12" class="shrink-0 text-[var(--ui-accent)]" />
                <strong class="truncate text-[15px] font-semibold text-[var(--ui-text)] group-hover:text-[var(--ui-accent)]">
                  {{ row.workspace.name }}
                </strong>
              </span>
              <small class="mt-0.5 block truncate font-mono text-[11px] text-[var(--ui-text-muted)]">
                {{ row.workspace.path }}
              </small>
            </button>
            <StatusPill
              :status="row.query.error ? 'error' : row.query.stale ? 'unknown' : row.state?.execution.state"
              :label="row.query.error ? '更新失败' : row.query.stale ? '数据过期' : undefined"
            />
          </div>

          <p class="mt-3 line-clamp-2 min-h-[2.5rem] text-xs leading-5 text-[var(--ui-text-secondary)]">
            {{ row.focus.title }}
          </p>
        </div>

        <div class="mt-3 flex items-center justify-between gap-2 border-t border-[var(--ui-line)] pt-3 text-xs">
          <button
            type="button"
            class="inline-flex min-w-0 max-w-full items-center gap-1.5 rounded-full bg-[var(--ui-surface)] px-2.5 py-1 text-xs font-medium text-[var(--ui-text-secondary)] hover:text-[var(--ui-accent)]"
            @click="emit('open-path', `/workspace/${row.workspace.id}/changes`)"
          >
            <GitBranch :size="12" class="shrink-0" />
            <span class="truncate">{{ row.git?.branch || "工作树" }}</span>
            <span class="shrink-0 whitespace-nowrap font-mono text-[var(--ui-accent)]">· {{ changedCount(row) }} 变更</span>
          </button>

          <WorkspaceRowMenu
            :workspace-id="row.workspace.id"
            :workspace-path="row.workspace.path"
            :pinned="pinnedIds.has(row.workspace.id)"
            :ordered-index="orderedIndexById.get(row.workspace.id) ?? -1"
            :ordered-length="totalCount"
            summary-label="操作"
            @toggle-pin="emit('toggle-pin', $event)"
            @move="(id, dir) => emit('move', id, dir)"
            @reveal="emit('reveal', $event)"
            @copy="emit('copy', $event)"
          />
        </div>
      </div>
    </div>

    <div v-else class="ui-workspace-table">
      <div class="ui-workspace-row ui-table-heading">
        <span>工作区名称</span>
        <span>当前任务</span>
        <span>执行状态</span>
        <span>变更数</span>
        <span>操作</span>
      </div>
      <div v-for="row in filteredWorkspaces" :key="row.workspace.id" class="ui-workspace-row">
        <button class="ui-text-link inline-flex min-w-0 items-center gap-1.5" @click="emit('open-path', `/workspace/${row.workspace.id}`)">
          <Pin v-if="pinnedIds.has(row.workspace.id)" :size="12" class="shrink-0" />
          <span class="truncate">{{ row.workspace.name }}</span>
        </button>
        <span class="ui-truncate text-xs text-[var(--ui-text-secondary)]" :title="row.focus.title">{{ row.focus.title }}</span>
        <StatusPill
          :status="row.query.error ? 'error' : row.query.stale ? 'unknown' : row.state?.execution.state"
          :label="row.query.error ? '更新失败' : row.query.stale ? '数据过期' : undefined"
        />
        <button class="ui-text-link font-mono text-xs" @click="emit('open-path', `/workspace/${row.workspace.id}/changes`)">
          {{ changedCount(row) }}
        </button>
        <WorkspaceRowMenu
          :workspace-id="row.workspace.id"
          :workspace-path="row.workspace.path"
          :pinned="pinnedIds.has(row.workspace.id)"
          :ordered-index="orderedIndexById.get(row.workspace.id) ?? -1"
          :ordered-length="totalCount"
          summary-label="更多"
          show-path-code
          @toggle-pin="emit('toggle-pin', $event)"
          @move="(id, dir) => emit('move', id, dir)"
          @reveal="emit('reveal', $event)"
          @copy="emit('copy', $event)"
        />
      </div>
    </div>
  </GlassCard>
</template>
