<script setup lang="ts">
import { computed, onMounted, ref, onUnmounted } from "vue";
import { ChevronDown, FileCode2, History, RefreshCw, Search, Trash2 } from "@lucide/vue";
import GlassCard from "../ui/GlassCard.vue";
import BaseButton from "../ui/BaseButton.vue";
import EmptyState from "../ui/EmptyState.vue";
import ConfirmDialog from "../ui/ConfirmDialog.vue";
import SelectField from "../ui/SelectField.vue";
import { getCachedWorkspaceReviews, peekCachedWorkspaceReviews, setCachedWorkspaceReviews } from "$lib/workspace-status-cache";
import { createWorkspaceReviewUrl, createSessionReviewUrl, issueWorkspaceReviewUrl, deleteWorkspaceReview, type WorkspaceReviewSummaryDto } from "$lib/api/workspaces";
import { listHistorySessions, type HistorySessionSummary } from "$lib/api/history";
import { openUrl } from "$lib/api/app-info";
import { showToast } from "$lib/stores/toast";
const props = defineProps<{
    workspaceId: string;
}>();
const reviews = computed(() => peekCachedWorkspaceReviews(props.workspaceId) ?? []);
const sessions = ref<HistorySessionSummary[]>([]);
const selectedSession = ref('');
const filter = ref('all');
const searchQuery = ref('');
const busy = ref(false);
const error = ref('');
const pending = ref<WorkspaceReviewSummaryDto | null>(null);
const activeMenuId = ref<string | null>(null);
let disposed = false;

// Sort newest first
const sortedReviews = computed(() => [...reviews.value].sort((a, b) => b.createdAt - a.createdAt));

const filtered = computed(() => sortedReviews.value.filter(r => {
    const matchesScope = filter.value === 'all' || r.scope === filter.value;
    const query = searchQuery.value.trim().toLowerCase();
    const matchesQuery = !query ||
        (r.summary || '').toLowerCase().includes(query) ||
        (r.sessionId || '').toLowerCase().includes(query);
    return matchesScope && matchesQuery;
}));

const sessionOptions = computed(() => [
    { value: '', label: '选择历史会话' },
    ...sessions.value.filter(s => s.session_key).map(s => ({ value: s.session_key!, label: `#${s.number} ${s.title || '开发会话'}` }))
]);

async function load() {
    error.value = '';
    try {
        const [_, history] = await Promise.all([getCachedWorkspaceReviews(props.workspaceId, true), listHistorySessions(props.workspaceId)]);
        if (!disposed)
            sessions.value = history.sessions;
    } catch (e) {
        if (!disposed)
            error.value = String(e);
    }
}

async function generate(scope: 'workspace' | 'session') {
    if (busy.value) return;
    busy.value = true;
    error.value = '';
    try {
        const review = scope === 'workspace' ? await createWorkspaceReviewUrl(props.workspaceId) : await createSessionReviewUrl(props.workspaceId, selectedSession.value);
        if (review) {
            await openUrl(review.url);
            await load();
        } else {
            showToast('当前范围没有可展示的变更', { kind: 'info' });
        }
    } catch (e) {
        error.value = String(e);
    } finally {
        busy.value = false;
    }
}

async function view(id: string) {
    try {
        await openUrl(await issueWorkspaceReviewUrl(props.workspaceId, id));
    } catch (e) {
        error.value = String(e);
    }
}

async function remove() {
    if (!pending.value || busy.value) return;
    busy.value = true;
    try {
        await deleteWorkspaceReview(props.workspaceId, pending.value.id);
        setCachedWorkspaceReviews(props.workspaceId, reviews.value.filter(r => r.id !== pending.value!.id));
        pending.value = null;
    } catch (e) {
        error.value = String(e);
    } finally {
        busy.value = false;
    }
}

function date(value: number) {
    return new Date(value * 1000).toLocaleString('zh-CN', {
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        hour12: false,
    });
}

function handleGlobalClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target.closest('.ui-row-menu-container')) {
        activeMenuId.value = null;
    }
}

onMounted(() => {
    void load();
    window.addEventListener('click', handleGlobalClick);
});

onUnmounted(() => {
    disposed = true;
    window.removeEventListener('click', handleGlobalClick);
});
</script>

<template>
<div class="ui-stack">
  <GlassCard>
    <div class="ui-section-heading">
      <div>
        <h2>查看代码变更</h2>
        <p>生成工作区或会话 Diff，保留当前时刻的冻结快照。</p>
      </div>
      <BaseButton :busy="busy" @click="generate('workspace')">查看工作区 Diff</BaseButton>
    </div>
    <div class="ui-form-row">
      <SelectField v-model="selectedSession" label="会话范围" :options="sessionOptions"/>
      <BaseButton variant="secondary" :disabled="!selectedSession" :busy="busy" @click="generate('session')">查看会话 Diff</BaseButton>
    </div>
    <p class="ui-muted mt-4">工作区 Diff 对比当前 HEAD；会话 Diff 聚合该会话记录的 AI 修改；单次操作快照只包含对应操作。</p>
  </GlassCard>

  <GlassCard class="relative">
    <div class="ui-section-heading">
      <div class="flex items-center gap-2">
        <h2>冻结快照</h2>
        <span class="rounded-full bg-[var(--ui-surface-subtle)] px-2 py-0.5 text-xs text-[var(--ui-text-muted)]">{{ sortedReviews.length }}</span>
      </div>
      <BaseButton variant="ghost" size="sm" @click="load"><RefreshCw :size="13" />刷新</BaseButton>
    </div>

    <!-- Sticky filter and search bar -->
    <div class="sticky top-0 z-10 -mx-5 -mt-2 bg-[var(--ui-surface)]/95 px-5 pb-3 pt-2 backdrop-blur-md border-b border-[var(--ui-line)]">
      <div class="flex flex-wrap items-center gap-3">
        <div class="w-36 shrink-0">
          <SelectField v-model="filter" :options="[{value:'all',label:'全部范围'},{value:'workspace',label:'工作区'},{value:'session',label:'会话'},{value:'operation',label:'单次操作'}]"/>
        </div>
        <div class="relative min-w-[200px] flex-1">
          <Search :size="13" class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
          <input
            v-model="searchQuery"
            type="text"
            placeholder="搜索快照说明、会话 ID…"
            class="h-9 w-full rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] pl-8 pr-3 text-xs text-[var(--ui-text)] outline-none transition-[background,border-color,box-shadow] focus:border-[var(--ui-accent)] focus:bg-[var(--ui-surface-raised)] focus:shadow-[var(--focus-ring)]"
          />
        </div>
      </div>
    </div>

    <p v-if="error" class="ui-danger mt-4" role="alert">{{error}}</p>

    <!-- Scrollable Snapshot List -->
    <div class="max-h-[560px] overflow-y-auto divide-y divide-[var(--ui-line)]">
      <div v-for="review in filtered" :key="review.id" class="flex flex-wrap items-center justify-between gap-3 py-3 transition hover:bg-[var(--ui-surface-hover)] rounded-xl px-2">
        <div class="min-w-0 flex-1 space-y-1">
          <div class="flex flex-wrap items-center gap-2">
            <span
              :class="review.scope === 'operation' ? 'bg-purple-500/10 text-purple-600 dark:text-purple-400' : review.scope === 'session' ? 'bg-blue-500/10 text-blue-600 dark:text-blue-400' : 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'"
              class="rounded-md px-1.5 py-0.5 text-[10px] font-medium"
            >
              {{ review.scope === 'operation' ? '单次操作' : review.scope === 'session' ? '会话' : '工作区' }}
            </span>
            <strong class="truncate text-xs font-semibold text-[var(--ui-text)]">{{ review.summary || '变更快照' }}</strong>
          </div>
          <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-[var(--ui-text-muted)]">
            <span>{{ review.files }} 个文件</span>
            <span>·</span>
            <span class="font-mono text-[var(--ui-success)] font-medium">+{{ review.additions }}</span>
            <span class="font-mono text-[var(--ui-danger)] font-medium">−{{ review.deletions }}</span>
            <span>·</span>
            <span>{{ date(review.createdAt) }}</span>
            <template v-if="review.sessionId">
              <span>·</span>
              <span class="truncate max-w-[200px] font-mono text-[10px]" :title="review.sessionId">会话: {{ review.sessionId }}</span>
            </template>
          </div>
        </div>

        <div class="flex items-center gap-2 shrink-0">
          <BaseButton variant="secondary" size="sm" @click="view(review.id)">查看 Diff</BaseButton>
          <div class="ui-row-menu-container relative">
            <BaseButton
              variant="ghost"
              size="sm"
              class="gap-1 text-[var(--ui-text-secondary)]"
              @click.stop="activeMenuId = activeMenuId === review.id ? null : review.id"
            >
              管理
              <ChevronDown :size="12" class="transition-transform duration-150" :class="{ 'rotate-180': activeMenuId === review.id }" />
            </BaseButton>
            <div v-if="activeMenuId === review.id" class="ui-menu right-0 top-[calc(100%+4px)] min-w-[120px]">
              <BaseButton variant="danger" size="sm" class="w-full justify-start text-xs" @click="pending = review; activeMenuId = null">
                <Trash2 :size="13" />删除快照
              </BaseButton>
            </div>
          </div>
        </div>
      </div>
    </div>

    <EmptyState v-if="!filtered.length && !error" :title="searchQuery ? '没有匹配的快照' : '暂无变更快照'" :description="searchQuery ? '尝试更改关键词或清除筛选。' : '查看工作区 Diff，或选择有 AI 修改的历史会话生成快照。'"/>
  </GlassCard>

  <ConfirmDialog
    :open="!!pending"
    title="删除冻结快照"
    message="仅删除本地 Diff 快照，不修改源码或 Git 历史。删除后无法恢复。"
    severity="danger"
    :busy="busy"
    confirm-text="删除快照"
    @confirm="remove"
    @cancel="pending=null"
  />
</div>
</template>
