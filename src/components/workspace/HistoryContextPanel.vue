<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { History, RefreshCw, Search } from "@lucide/vue";
import SaveBar from "../ui/SaveBar.vue";
import {useUnsavedChanges} from "$lib/unsaved-changes";
import BaseButton from "../ui/BaseButton.vue";
import ToggleSwitch from "../ui/ToggleSwitch.vue";
import { listHistorySessions, type HistorySessionSummary } from "$lib/api/history";
import { createSessionReviewUrl, issueWorkspaceReviewUrl, listWorkspaceReviews, type WorkspaceReviewSummaryDto } from "$lib/api/workspaces";
import { showToast } from "$lib/stores/toast";
import { openUrl } from "$lib/api/app-info";
import { getPlanningState, type PlanningStateDto } from "$lib/api/planning";
import type { WorkspaceProfile } from "$lib/types";

const props = defineProps<{
  showRecording?: boolean;
  workspaceId: string;
  profile: WorkspaceProfile;
  onSave: (recording: boolean, sessions: number[]) => Promise<void>;
}>();
const router = useRouter();
const loading = ref(false);
const saving = ref(false);
const sessions = ref<HistorySessionSummary[]>([]);
const reviews = ref<WorkspaceReviewSummaryDto[]>([]);
const planning = ref<PlanningStateDto | null>(null);
const reviewError = ref("");
const recording = ref(props.profile.runtime.history_recording ?? true);
const selected = ref<number[]>([...(props.profile.runtime.history_context_sessions ?? [])]);
const baseline = ref(serializedSelection());
const saveError = ref("");
const loadError = ref("");
const sessionSearch = ref("");
let loadGeneration = 0;

const sortedSessions = computed(() => [...sessions.value].sort((a, b) => b.number - a.number));
const recentTasks = computed(() => [
  ...(planning.value?.plans ?? []).map(item => ({ ...item, kind: "计划" })),
  ...(planning.value?.goals ?? []).map(item => ({ ...item, kind: "目标" })),
].sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 20));

const filteredSessions = computed(() => sortedSessions.value.filter(s => {
  if (!sessionSearch.value.trim()) return true;
  const q = sessionSearch.value.trim().toLowerCase();
  return String(s.number).includes(q) ||
         (s.title || '').toLowerCase().includes(q) ||
         (s.latest_focus || '').toLowerCase().includes(q) ||
         (s.path || '').toLowerCase().includes(q);
}));

function serializedSelection() {
  return JSON.stringify({ recording: recording.value, sessions: [...selected.value].sort((a, b) => a - b) });
}
async function viewSessionReview(session: HistorySessionSummary) {
  if (!session.session_key) {
    showToast("这个历史会话没有可用于聚合 Review 的 session key", { kind: "error" });
    return;
  }
  try {
    const review = await createSessionReviewUrl(props.workspaceId, session.session_key);
    if (!review) {
      showToast("这个会话暂时没有可聚合的 AI 修改", { kind: "success" });
      return;
    }
    await openUrl(review.url);
    reviews.value = await listWorkspaceReviews(props.workspaceId);
  } catch (error) {
    showToast(String(error), { title: "无法生成会话 Diff", kind: "error" });
  }
}
function sessionReviews(session: HistorySessionSummary) { return reviews.value.filter(review => review.sessionId && review.sessionId === session.session_key); }
async function viewReview(id: string) {
  try {
    const url = await issueWorkspaceReviewUrl(props.workspaceId, id);
    await openUrl(url);
  } catch (error) {
    showToast(String(error), { title: "无法打开 Review", kind: "error" });
  }
}

const dirty = computed(() => serializedSelection() !== baseline.value);
watch(() => props.profile, (profile) => {
  if(dirty.value) return;
  recording.value = profile.runtime.history_recording ?? true;
  selected.value = [...(profile.runtime.history_context_sessions ?? [])];
  baseline.value = serializedSelection();
}, { immediate: true, deep: true });

async function load() {
  const workspaceId = props.workspaceId;
  const generation = ++loadGeneration;
  loading.value = true;
  loadError.value="";
  reviewError.value = "";
  sessions.value = [];
  reviews.value = [];
  planning.value = null;
  const [history, changeReviews, taskState] = await Promise.allSettled([
    listHistorySessions(workspaceId),
    listWorkspaceReviews(workspaceId),
    getPlanningState(workspaceId),
  ]);
  if (generation !== loadGeneration || props.workspaceId !== workspaceId) return;
  if (history.status === "fulfilled") sessions.value = history.value.sessions ?? [];
  else loadError.value = String(history.reason);
  if (changeReviews.status === "fulfilled") reviews.value = changeReviews.value;
  else reviewError.value = "变更快照暂时不可用，不影响历史记录浏览。";
  if (taskState.status === "fulfilled") planning.value = taskState.value;
  loading.value = false;
}

function openTask(id: string) {
  void router.push({ path: "/workspace/" + props.workspaceId + "/planning", query: { item: id } });
}

function toggle(number: number, checked: boolean) {
  selected.value = checked ? [...new Set([...selected.value, number])] : selected.value.filter((item) => item !== number);
}

async function save() {
  if (saving.value || !dirty.value) return;
  saving.value = true;
  saveError.value="";
  try {
    const savedSelection = serializedSelection();
    await props.onSave(recording.value, [...selected.value]);
    baseline.value = savedSelection;
    showToast("历史上下文配置已保存", { kind: "success" });
    return true;
  } catch (error) {
    saveError.value=String(error);
    return false;
  }
  finally { saving.value = false; }
}

useUnsavedChanges("历史上下文",dirty,save);
onMounted(() => { void load(); });
watch(() => props.workspaceId, () => { void load(); });
</script>

<template>
  <div class="grid gap-4">
    <ToggleSwitch v-if="showRecording !== false" v-model="recording" :disabled="saving" label="记录 History Session" description="把开发过程写入项目内 docs/history-session，便于跨会话恢复。" />
    <p v-if="loadError" class="ui-danger" role="alert">加载历史失败：{{loadError}} <BaseButton variant="secondary" size="sm" @click="load">重试</BaseButton></p>
    <div>
      <div class="mb-2 flex items-center justify-between">
        <div>
          <p class="text-xs font-semibold">注入到当前上下文的历史会话</p>
          <p class="mt-0.5 text-xs text-[var(--text-muted)]">只选择确实需要长期带入的会话，避免上下文膨胀。最新会话排在顶部。</p>
        </div>
        <BaseButton variant="ghost" size="sm" :disabled="loading" @click="load">
          <RefreshCw :size="14" :class="{ 'animate-spin': loading }" /> 刷新
        </BaseButton>
      </div>

      <p v-if="reviewError" class="ui-muted mb-2" role="status">{{ reviewError }}</p>
      <div class="max-h-[380px] overflow-y-auto rounded-2xl bg-[var(--ui-surface-subtle)] p-2.5 space-y-2">
        <!-- Sticky Search Bar -->
        <div class="sticky top-0 z-10 bg-[var(--ui-surface-subtle)] pb-1.5 pt-0.5 backdrop-blur-md">
          <div class="relative">
            <Search :size="13" class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
            <input
              v-model="sessionSearch"
              type="text"
              placeholder="搜索历史会话标题、编号…"
              class="h-8 w-full rounded-xl border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] pl-7 pr-3 text-xs text-[var(--ui-text)] outline-none transition focus:border-[var(--ui-accent)]"
            />
          </div>
        </div>

        <div v-for="session in filteredSessions" :key="session.number" class="flex gap-3 rounded-xl p-2.5 transition hover:bg-[var(--ui-surface-hover)] bg-[var(--ui-surface)]/60 border border-[var(--ui-line)]/50">
          <input
            :aria-label="`注入会话 ${session.number} ${session.title}`"
            type="checkbox"
            :disabled="saving"
            class="mt-1 accent-[var(--primary)] shrink-0"
            :checked="selected.includes(session.number)"
            @change="toggle(session.number, ($event.target as HTMLInputElement).checked)"
          />
          <span class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="rounded bg-[var(--ui-accent-soft)] px-1.5 py-0.2 text-[10px] font-semibold text-[var(--ui-accent)]">#{{ session.number }}</span>
              <span class="block truncate text-xs font-semibold text-[var(--ui-text)]">{{ session.title || '开发会话' }}</span>
            </div>
            <span class="mt-1 block line-clamp-2 text-xs leading-4 text-[var(--text-muted)]">{{ session.latest_focus || session.path }}</span>
            <span class="mt-2 flex flex-wrap gap-1.5">
              <BaseButton v-if="session.session_key" variant="secondary" size="sm" @click.prevent="viewSessionReview(session)">查看会话 Diff</BaseButton>
              <BaseButton v-for="review in sessionReviews(session).filter(item => item.scope === 'operation').slice(0,3)" :key="review.id" variant="ghost" size="sm" @click.prevent="viewReview(review.id)">查看操作快照</BaseButton>
            </span>
            <details class="ui-disclosure mt-2">
              <summary class="text-xs cursor-pointer text-[var(--ui-text-muted)]">查看会话摘要</summary>
              <div class="ui-disclosure-body text-xs">
                <p class="ui-muted">{{session.entry_count}} 条记录 · {{session.created_at}}</p>
                <p v-for="snippet in session.snippets" :key="snippet.turn_id" class="whitespace-pre-wrap break-words text-[11px]">{{snippet.text}}</p>
                <p v-if="!session.snippets.length" class="text-[11px]">暂无摘要片段。</p>
              </div>
            </details>
          </span>
        </div>

        <div v-if="loading" class="px-4 py-6 text-center text-xs text-[var(--text-muted)]">正在读取历史档案…</div>
        <div v-else-if="!loadError && !filteredSessions.length" class="px-4 py-6 text-center">
          <p class="text-xs font-medium text-[var(--text-secondary)]">{{ sessionSearch ? '未找到匹配的历史会话' : '暂无历史会话' }}</p>
          <p class="mt-1 text-xs leading-5 text-[var(--text-muted)]">{{ sessionSearch ? '尝试清除或调整搜索关键词。' : '这个工作区尚无已写入的会话档案。开启记录仅允许保存，AI 仍需调用 History checkpoint；未写入的聊天正文无法事后还原。' }}</p>
        </div>
      </div>
    </div>
    <div v-if="recentTasks.length" class="min-w-0">
      <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div>
          <p class="text-xs font-semibold">任务与计划历史</p>
          <p class="mt-1 text-xs text-[var(--text-muted)]">来自独立的 Planning 记录，不是聊天会话档案；即使没有 History checkpoint 也可查看。</p>
        </div>
        <History :size="16" class="text-[var(--ui-text-muted)]" />
      </div>
      <div class="max-h-72 space-y-1 overflow-y-auto rounded-xl border border-[var(--ui-line)] p-2">
        <button v-for="task in recentTasks" :key="task.id" type="button"
          class="flex w-full min-w-0 items-center gap-2 rounded-lg px-3 py-2 text-left hover:bg-[var(--ui-surface-hover)]"
          @click="openTask(task.id)">
          <span class="shrink-0 text-[10px] text-[var(--ui-text-muted)]">{{ task.kind }}</span>
          <span class="min-w-0 flex-1 truncate text-xs font-medium">{{ task.title }}</span>
          <span class="shrink-0 text-[10px] text-[var(--ui-text-muted)]">{{ task.status }}</span>
        </button>
      </div>
    </div>
    <SaveBar :dirty="dirty" :busy="saving" :error="saveError" label="保存历史上下文" @save="save" />
  </div>
</template>
