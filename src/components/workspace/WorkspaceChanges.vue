<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { ChevronDown, ExternalLink, RefreshCw, Search, Trash2 } from "@lucide/vue";
import GlassCard from "../ui/GlassCard.vue";
import BaseButton from "../ui/BaseButton.vue";
import EmptyState from "../ui/EmptyState.vue";
import ConfirmDialog from "../ui/ConfirmDialog.vue";
import SelectField from "../ui/SelectField.vue";
import { getCachedWorkspaceReviews, peekCachedWorkspaceReviews, setCachedWorkspaceReviews } from "$lib/workspace-status-cache";
import {
  createSessionReviewUrl,
  createWorkspaceReviewUrl,
  deleteWorkspaceReview,
  issueWorkspaceReviewUrl,
  type WorkspaceReviewSummaryDto,
} from "$lib/api/workspaces";
import { listHistorySessions, type HistorySessionSummary } from "$lib/api/history";
import { openUrl } from "$lib/api/app-info";
import { formatShortDateTime } from "$lib/formatters";
import { showToast } from "$lib/stores/toast";

const props = defineProps<{
  workspaceId: string;
}>();

const reviews = computed(() => peekCachedWorkspaceReviews(props.workspaceId) ?? []);
const sessions = ref<HistorySessionSummary[]>([]);
const selectedSession = ref("");
const filter = ref("all");
const searchQuery = ref("");
const busy = ref(false);
const error = ref("");
const pending = ref<WorkspaceReviewSummaryDto | null>(null);
const activeMenuId = ref<string | null>(null);
let disposed = false;

const sortedReviews = computed(() => [...reviews.value].sort((a, b) => b.createdAt - a.createdAt));

const filtered = computed(() =>
  sortedReviews.value.filter((r) => {
    const matchesScope = filter.value === "all" || r.scope === filter.value;
    const q = searchQuery.value.trim().toLowerCase();
    const matchesQuery = !q || (r.summary || "").toLowerCase().includes(q) || (r.sessionId || "").toLowerCase().includes(q);
    return matchesScope && matchesQuery;
  }),
);

const sessionOptions = computed(() => [
  { value: "", label: "选择需要聚合的历史会话…" },
  ...sessions.value.filter((s) => s.session_key).map((s) => ({ value: s.session_key!, label: `#${s.number} ${s.title || "开发会话"}` })),
]);

async function load() {
  error.value = "";
  try {
    const [_, history] = await Promise.all([getCachedWorkspaceReviews(props.workspaceId, true), listHistorySessions(props.workspaceId)]);
    if (!disposed) sessions.value = history.sessions;
  } catch (e) {
    if (!disposed) error.value = String(e);
  }
}

async function generate(scope: "workspace" | "session") {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const review = scope === "workspace" ? await createWorkspaceReviewUrl(props.workspaceId) : await createSessionReviewUrl(props.workspaceId, selectedSession.value);
    if (review) {
      await openUrl(review.url);
      await load();
    } else {
      showToast("当前范围没有可展示的变更", { kind: "info" });
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
    setCachedWorkspaceReviews(
      props.workspaceId,
      reviews.value.filter((r) => r.id !== pending.value!.id),
    );
    pending.value = null;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

function date(value: number) {
  return formatShortDateTime(value * 1000);
}

function handleGlobalClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (!target.closest(".ui-row-menu-container")) {
    activeMenuId.value = null;
  }
}

onMounted(() => {
  void load();
  window.addEventListener("click", handleGlobalClick);
});

onUnmounted(() => {
  disposed = true;
  window.removeEventListener("click", handleGlobalClick);
});
</script>

<template>
  <div class="ui-stack">
    <GlassCard variant="focal">
      <div class="ui-section-heading flex-wrap">
        <div>
          <h2>生成代码变更快照（Diff Review）</h2>
          <p>一键冻结当前工作区或指定会话的全部修改，在独立双栏 Diff 视图中逐行审阅。</p>
        </div>
        <BaseButton :busy="busy" @click="generate('workspace')">
          <ExternalLink v-if="!busy" :size="14" /> 查看当前工作区 Diff
        </BaseButton>
      </div>

      <div class="ui-inset p-4">
        <div class="ui-form-row">
          <SelectField v-model="selectedSession" label="按会话聚合 AI 修改" :options="sessionOptions" />
          <BaseButton variant="secondary" :disabled="!selectedSession" :busy="busy" @click="generate('session')">生成会话 Diff</BaseButton>
        </div>
        <p class="ui-muted mt-2.5">工作区 Diff 对比当前 Git HEAD；会话 Diff 聚合该会话内记录的所有补丁操作。</p>
      </div>
    </GlassCard>

    <GlassCard class="relative">
      <div class="ui-section-heading">
        <div class="flex items-center gap-2.5">
          <h2>历史冻结快照</h2>
          <span class="rounded-full bg-[var(--ui-surface-subtle)] px-2.5 py-0.5 font-mono text-xs text-[var(--ui-text-muted)]">
            {{ sortedReviews.length }}
          </span>
        </div>
        <BaseButton variant="ghost" size="sm" @click="load">
          <RefreshCw :size="13" /> 刷新
        </BaseButton>
      </div>

      <div class="ui-inset mb-4 flex flex-wrap items-center gap-3 p-3">
        <div class="w-40 shrink-0">
          <SelectField
            v-model="filter"
            :options="[
              { value: 'all', label: '全部范围' },
              { value: 'workspace', label: '工作区级' },
              { value: 'session', label: '会话聚合' },
              { value: 'operation', label: '单次操作' },
            ]"
          />
        </div>
        <div class="relative min-w-[200px] flex-1">
          <Search :size="14" class="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
          <input
            v-model="searchQuery"
            type="text"
            placeholder="搜索快照摘要或会话 ID…"
            class="h-10 w-full rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] pl-9 pr-4 text-xs text-[var(--ui-text)] outline-none transition focus:border-[var(--ui-accent)] focus:shadow-[var(--focus-ring)]"
          />
        </div>
      </div>

      <p v-if="error" class="ui-danger mb-4 text-xs" role="alert">{{ error }}</p>

      <div class="max-h-[560px] space-y-2 overflow-y-auto">
        <div
          v-for="review in filtered"
          :key="review.id"
          class="ui-inset flex flex-wrap items-center justify-between gap-3 px-4 py-3.5 transition hover:border-[var(--ui-line-strong)] hover:bg-[var(--ui-surface-hover)]"
        >
          <div class="min-w-0 flex-1 space-y-1.5">
            <div class="flex flex-wrap items-center gap-2">
              <span
                :class="
                  review.scope === 'operation'
                    ? 'bg-purple-500/12 text-purple-600 dark:text-purple-400'
                    : review.scope === 'session'
                      ? 'bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]'
                      : 'bg-[var(--ui-success-soft)] text-[var(--ui-success)]'
                "
                class="rounded-full px-2.5 py-0.5 text-[10.5px] font-semibold"
              >
                {{ review.scope === "operation" ? "单次操作" : review.scope === "session" ? "会话聚合" : "工作区全局" }}
              </span>
              <strong class="truncate text-sm font-semibold text-[var(--ui-text)]">{{ review.summary || "代码变更快照" }}</strong>
            </div>

            <div class="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-xs text-[var(--ui-text-muted)]">
              <span class="tabular-nums">{{ review.files }} 个文件</span>
              <span>·</span>
              <span class="rounded-full bg-[var(--ui-success-soft)] px-2 py-0.5 font-mono font-medium text-[var(--ui-success)]">+{{ review.additions }}</span>
              <span class="rounded-full bg-[var(--ui-danger-soft)] px-2 py-0.5 font-mono font-medium text-[var(--ui-danger)]">−{{ review.deletions }}</span>
              <span>·</span>
              <span class="font-mono text-[11px]">{{ date(review.createdAt) }}</span>
              <template v-if="review.sessionId">
                <span>·</span>
                <span class="max-w-[200px] truncate font-mono text-[11px]" :title="review.sessionId">会话 {{ review.sessionId }}</span>
              </template>
            </div>
          </div>

          <div class="flex shrink-0 items-center gap-2">
            <BaseButton variant="secondary" size="sm" @click="view(review.id)">打开 Diff</BaseButton>
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
              <div v-if="activeMenuId === review.id" class="ui-menu right-0 top-[calc(100%+6px)] min-w-[132px]">
                <BaseButton variant="danger" size="sm" class="w-full justify-start text-xs" @click="pending = review; activeMenuId = null">
                  <Trash2 :size="13" /> 删除快照
                </BaseButton>
              </div>
            </div>
          </div>
        </div>
      </div>

      <EmptyState
        v-if="!filtered.length && !error"
        :title="searchQuery ? '没有匹配的变更快照' : '暂无变更快照'"
        :description="searchQuery ? '尝试调整搜索关键词或切换筛选范围。' : '点击上方「查看当前工作区 Diff」即可随时生成第一份只读代码快照。'"
      />
    </GlassCard>

    <ConfirmDialog
      :open="!!pending"
      title="删除冻结快照"
      message="仅删除本地 Diff 快照记录，不会修改源码文件或 Git 提交历史。"
      severity="danger"
      :busy="busy"
      confirm-text="确认删除"
      @confirm="remove"
      @cancel="pending = null"
    />
  </div>
</template>
