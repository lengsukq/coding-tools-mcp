<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ArrowUpDown, FileText, RefreshCw, Search } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import StatusPill from "../ui/StatusPill.vue";
import EmptyState from "../ui/EmptyState.vue";
import SelectField from "../ui/SelectField.vue";
import TextAreaField from "../ui/TextAreaField.vue";
import SegmentedControl from "../ui/SegmentedControl.vue";
import ConfirmDialog from "../ui/ConfirmDialog.vue";
import {
  acceptGoalReview,
  acceptPlanReview,
  deletePlan,
  rejectGoalReview,
  rejectPlanReview,
  resetPlanningState,
  setPlanningMode,
  type GoalDto,
  type PlanDto,
  type PlanningMode,
  type PlanningStateDto,
} from "$lib/api/planning";
import { getCachedPlanningState, setCachedPlanningState, useWorkspaceStatus } from "$lib/workspace-status-cache";
import { planProgress, statusPresentation, taskFocus } from "$lib/ui-model";

const props = defineProps<{
  workspaceId: string;
}>();
const emit = defineEmits<{
  navigate: [tab: "changes" | "diagnostics"];
}>();

const route = useRoute();
const router = useRouter();
const { planning: state, query, refresh } = useWorkspaceStatus(() => props.workspaceId);
const busy = ref(false);
const error = ref("");
const filter = ref("unfinished");
const taskSearch = ref("");
const selectedId = ref("");
const feedback = ref<Record<string, string>>({});
const resetOpen = ref(false);
const deleting = ref("");
const reverseVerification = ref(true);

const modes = [
  { value: "direct", label: "直接执行" },
  { value: "plan", label: "按计划执行" },
  { value: "goal", label: "按目标执行" },
];

const entries = computed(() => [
  ...(state.value?.plans ?? []).map((item) => ({ kind: "plan" as const, item })),
  ...(state.value?.goals ?? []).map((item) => ({ kind: "goal" as const, item })),
]);

const visible = computed(() =>
  entries.value.filter(({ item }) =>
    filter.value === "unfinished"
      ? ["draft", "active", "paused", "awaiting_acceptance"].includes(item.status) ||
        (item.status === "completed" && (item.id === state.value?.focus_plan_id || item.id === state.value?.focus_goal_id))
      : filter.value === "ended"
        ? ["archived", "cancelled", "completed"].includes(item.status)
        : item.status === "awaiting_acceptance",
  ),
);

const filteredVisible = computed(() =>
  visible.value.filter(({ item }) => {
    if (!taskSearch.value.trim()) return true;
    const q = taskSearch.value.trim().toLowerCase();
    return (item.title || "").toLowerCase().includes(q) || (item.objective || "").toLowerCase().includes(q);
  }),
);

const selected = computed(() => entries.value.find((e) => e.item.id === selectedId.value) ?? null);
const selectedPlan = computed(() => (selected.value?.kind === "plan" ? (state.value?.plans.find((p) => p.id === selected.value!.item.id) ?? null) : null));
const parentGoal = computed(() => state.value?.goals.find((g) => g.id === selectedPlan.value?.goal_id) ?? null);
const selectedGoal = computed(() => (selected.value?.kind === "goal" ? (state.value?.goals.find((g) => g.id === selected.value!.item.id) ?? null) : null));
const executionMatches = computed(() =>
  selected.value?.kind === "plan" ? state.value?.execution.plan_id === selected.value.item.id : state.value?.execution.goal_id === selected.value?.item.id,
);
const progress = computed(() => planProgress(selectedPlan.value));
const pendingCount = computed(() => entries.value.filter((e) => e.item.status === "awaiting_acceptance").length);

const displayedVerification = computed(() => {
  const records = state.value?.execution.verification ?? [];
  const mapped = records.map((text, idx) => ({
    originalIndex: idx,
    text,
    isLatest: idx === records.length - 1,
  }));
  return reverseVerification.value ? [...mapped].reverse() : mapped;
});

watch(
  [() => route.query.item, entries],
  () => {
    const desired = String(route.query.item ?? "");
    if (desired && entries.value.some((e) => e.item.id === desired)) {
      if (selectedId.value !== desired) {
        const target = entries.value.find((e) => e.item.id === desired);
        filter.value = target && ["archived", "cancelled", "completed"].includes(target.item.status) ? "ended" : "unfinished";
      }
      selectedId.value = desired;
      return;
    }
    if (!entries.value.some((e) => e.item.id === selectedId.value))
      selectedId.value = taskFocus(state.value).plan?.id ?? taskFocus(state.value).goal?.id ?? visible.value[0]?.item.id ?? "";
  },
  { immediate: true },
);

function select(id: string) {
  selectedId.value = id;
  void router.replace({ query: { ...route.query, item: id } });
}

async function action(run: () => Promise<PlanningStateDto | PlanDto | GoalDto>) {
  if (busy.value) return false;
  const id = props.workspaceId;
  busy.value = true;
  error.value = "";
  let saved = false;
  try {
    const result = await run();
    saved = true;
    if ("schema_version" in result) {
      setCachedPlanningState(id, result);
    } else if (state.value) {
      const current = state.value;
      setCachedPlanningState(
        id,
        "steps" in result
          ? { ...current, plans: current.plans.map((item) => (item.id === result.id ? result : item)) }
          : { ...current, goals: current.goals.map((item) => (item.id === result.id ? result : item)) },
      );
      await getCachedPlanningState(id, true);
    } else {
      await getCachedPlanningState(id, true);
    }
    return true;
  } catch (e) {
    error.value = saved ? `操作已保存，状态刷新失败。请刷新查看最新状态：${String(e)}` : String(e);
    return false;
  } finally {
    busy.value = false;
  }
}

async function changeMode(value: string) {
  await action(() => setPlanningMode(props.workspaceId, value as PlanningMode));
}

async function review(accept: boolean) {
  const target = selected.value;
  if (!target) return;
  const text = feedback.value[target.item.id]?.trim();
  if (!accept && !text) {
    error.value = "请填写具体修改意见。";
    return;
  }
  const ok = await action(() =>
    target.kind === "plan"
      ? accept
        ? acceptPlanReview(props.workspaceId, target.item.id)
        : rejectPlanReview(props.workspaceId, target.item.id, text)
      : accept
        ? acceptGoalReview(props.workspaceId, target.item.id)
        : rejectGoalReview(props.workspaceId, target.item.id, text),
  );
  if (ok) {
    delete feedback.value[target.item.id];
  }
}

async function reset() {
  await action(() => resetPlanningState(props.workspaceId));
  if (!error.value) resetOpen.value = false;
}

async function remove() {
  await action(() => deletePlan(props.workspaceId, deleting.value));
  if (!error.value) deleting.value = "";
}
</script>

<template>
  <div class="ui-stack">
    <GlassCard>
      <div class="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h2>执行模式与任务门禁</h2>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            {{
              state?.mode === "direct"
                ? "直接执行允许 AI 按当前权限直接修改代码与运行命令。"
                : state?.mode === "plan"
                  ? "按计划执行要求代码写操作必须关联激活计划与步骤。"
                  : "按目标执行要求写操作关联当前激活目标及其下属计划。"
            }}
          </p>
        </div>
        <div class="ui-actions">
          <SegmentedControl :items="modes" :model-value="state?.mode ?? 'direct'" aria-label="执行模式" :disabled="busy || !state" @update:model-value="changeMode" />
          <BaseButton variant="ghost" size="sm" @click="refresh">
            <RefreshCw :size="13" /> 刷新
          </BaseButton>
        </div>
      </div>
    </GlassCard>

    <div v-if="query.error || query.stale" class="ui-banner ui-banner--warning" role="status">
      <span>{{ query.error ? "任务更新失败：" + query.error : "任务数据已过期，正在重新检查。" }}</span>
      <BaseButton variant="secondary" size="sm" @click="refresh">重试</BaseButton>
    </div>
    <p v-if="error && selected?.item.status !== 'awaiting_acceptance'" class="ui-danger text-xs" role="alert">{{ error }}</p>

    <div class="ui-inspector-grid planning-inspector">
      <GlassCard :padded="false" class="ui-task-list relative flex min-w-0 flex-col overflow-hidden">
        <div class="sticky top-0 z-10 space-y-2.5 border-b border-[var(--ui-line)] bg-[var(--ui-surface-raised)] p-4">
          <SelectField
            v-model="filter"
            :options="[
              { value: 'unfinished', label: '当前与未结束' },
              { value: 'review', label: '待验收（' + pendingCount + '）' },
              { value: 'ended', label: '已完成、归档与取消' },
            ]"
          />
          <div class="relative">
            <Search :size="14" class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
            <input
              v-model="taskSearch"
              type="text"
              placeholder="搜索任务或目标…"
              class="h-9 w-full rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] pl-9 pr-3 text-xs text-[var(--ui-text)] outline-none transition focus:border-[var(--ui-accent)] focus:bg-[var(--ui-surface-raised)]"
            />
          </div>
        </div>

        <div class="max-h-[min(64dvh,640px)] min-h-0 flex-1 space-y-1.5 overflow-y-auto p-3">
          <button
            v-for="entry in filteredVisible"
            :key="entry.kind + entry.item.id"
            class="ui-task-item min-w-0 text-left transition-colors"
            :class="{ active: selectedId === entry.item.id }"
            @click="select(entry.item.id)"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="rounded-full bg-[var(--ui-surface)] px-2 py-0.5 text-[10.5px] font-medium text-[var(--ui-text-muted)]">
                {{ entry.kind === "plan" ? "计划" : "目标" }}
              </span>
              <StatusPill :status="entry.item.status" />
            </div>
            <strong class="block min-w-0 text-xs leading-5 text-[var(--ui-text)]">{{ entry.item.title }}</strong>
          </button>

          <EmptyState
            v-if="!filteredVisible.length"
            :title="taskSearch ? '没有匹配的任务' : '暂无任务'"
            :description="taskSearch ? '尝试修改搜索关键词或切换筛选范围。' : '通过 AI 会话创建目标或计划后会自动显示在此处。'"
          />
        </div>
      </GlassCard>

      <GlassCard v-if="selected" class="min-w-0 overflow-hidden">
        <div class="ui-section-heading flex-wrap">
          <div class="min-w-0 flex-1">
            <span class="inline-block rounded-full bg-[var(--ui-accent-soft)] px-2.5 py-0.5 text-[11px] font-semibold text-[var(--ui-accent)]">
              {{ selected.kind === "plan" ? "执行计划 · Plan" : "项目目标 · Goal" }}
            </span>
            <h2 class="mt-2 break-words text-lg font-semibold">{{ selected.item.title }}</h2>
          </div>
          <StatusPill :status="selected.item.status" />
        </div>

        <p v-if="parentGoal" class="ui-secondary mb-3 text-xs">关联目标：{{ parentGoal.title }}</p>

        <details class="ui-disclosure">
          <summary class="cursor-pointer text-xs">任务目标说明</summary>
          <p class="ui-disclosure-body whitespace-pre-wrap text-xs leading-relaxed text-[var(--ui-text-secondary)]">{{ selected.item.objective }}</p>
        </details>

        <template v-if="selectedPlan">
          <div class="mt-5 flex items-center justify-between text-xs text-[var(--ui-text-muted)]">
            <span>{{ progress.label }}</span>
            <span class="font-mono font-medium text-[var(--ui-text-secondary)]">{{ progress.percent }}%</span>
          </div>
          <div class="ui-progress">
            <span :style="{ width: progress.percent + '%' }" />
          </div>

          <ol class="mt-4 list-none space-y-2.5">
            <li v-for="(step, index) in selectedPlan.steps" :key="step.id" class="ui-inset min-w-0 p-3.5">
              <div class="flex items-start justify-between gap-3">
                <span class="min-w-0 flex-1 break-words text-xs font-medium leading-5 text-[var(--ui-text)]">
                  <span class="mr-1 font-mono font-semibold text-[var(--ui-accent)]">{{ index + 1 }}.</span>
                  {{ step.title }}
                </span>
                <span class="shrink-0"><StatusPill :status="step.status" /></span>
              </div>

              <details v-if="step.notes" class="mt-2.5 min-w-0 rounded-[14px] border border-[var(--ui-line)] bg-[var(--ui-surface-raised)]">
                <summary class="flex cursor-pointer items-center gap-1.5 px-3 py-2 text-[11px] text-[var(--ui-text-muted)]">
                  <FileText :size="12" class="shrink-0 text-[var(--ui-accent)]" />步骤备注 · 点击展开
                </summary>
                <p class="whitespace-pre-wrap break-words border-t border-[var(--ui-line)] px-3 py-2.5 text-[11.5px] leading-5 text-[var(--ui-text-secondary)]">
                  {{ step.notes }}
                </p>
              </details>
            </li>
          </ol>
        </template>

        <template v-if="selectedGoal">
          <h3 class="mt-5 text-sm font-semibold">目标验收条件</h3>
          <div class="mt-2">
            <div v-for="criterion in selectedGoal.success_criteria" :key="criterion.id" class="ui-data-row text-xs">
              <span class="min-w-0 flex-1 break-words">{{ criterion.text }}</span>
              <span class="shrink-0"><StatusPill :status="criterion.completed ? 'completed' : 'pending'" /></span>
            </div>
          </div>
          <details v-if="selectedGoal.constraints.length" class="ui-disclosure mt-3">
            <summary class="cursor-pointer text-xs">约束条件</summary>
            <ul class="ui-disclosure-body list-disc pl-5 text-xs">
              <li v-for="constraint in selectedGoal.constraints" :key="constraint">{{ constraint }}</li>
            </ul>
          </details>
        </template>

        <div class="ui-actions mt-5">
          <BaseButton variant="secondary" size="sm" @click="emit('navigate', 'changes')">查看变更快照</BaseButton>
          <BaseButton variant="ghost" size="sm" @click="emit('navigate', 'diagnostics')">查看执行日志</BaseButton>
        </div>

        <div v-if="executionMatches" class="ui-inset mt-5 p-4">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <h3>执行与验证记录</h3>
            <button
              type="button"
              class="inline-flex items-center gap-1 text-xs font-medium text-[var(--ui-accent)] hover:underline"
              @click="reverseVerification = !reverseVerification"
            >
              <ArrowUpDown :size="12" />{{ reverseVerification ? "最新在前" : "最早在前" }}
            </button>
          </div>
          <p class="ui-secondary mt-1.5 text-xs">当前执行状态：{{ statusPresentation(state?.execution.state).label }}</p>
          <p v-if="state?.execution.last_error" class="ui-danger mt-1 break-words text-xs" role="alert">{{ state.execution.last_error }}</p>
          <p class="ui-muted mt-1.5 text-xs">共 {{ state?.execution.verification.length ?? 0 }} 条验证证据。</p>
          <ul class="mt-3 space-y-2">
            <li
              v-for="record in displayedVerification"
              :key="record.originalIndex"
              class="whitespace-pre-wrap break-words rounded-[14px] border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] p-3 text-xs"
            >
              <div class="mb-1 flex items-center gap-2 text-[10.5px] text-[var(--ui-text-muted)]">
                <span class="font-mono font-semibold text-[var(--ui-accent)]">#{{ record.originalIndex + 1 }}</span>
                <span v-if="record.isLatest" class="rounded-full bg-[var(--ui-success-soft)] px-2 py-0.5 font-semibold text-[var(--ui-success)]">最新验证</span>
              </div>
              <div class="text-[11.5px] leading-relaxed text-[var(--ui-text-secondary)]">{{ record.text }}</div>
            </li>
          </ul>
        </div>
        <p v-else class="ui-muted mt-4">当前任务暂无直接关联的执行记录；可前往历史或变更页追溯。</p>

        <div v-if="selected.item.status === 'awaiting_acceptance'" class="ui-review-box mt-5">
          <h3>人工验收确认</h3>
          <p v-if="error" class="ui-danger mt-2 text-xs" role="alert">{{ error }}</p>
          <p class="ui-secondary mt-1.5 text-xs leading-5">{{ selected.item.review_summary || "请复核任务说明、代码变更与验证记录后给出结论。" }}</p>
          <TextAreaField
            v-model="feedback[selected.item.id]"
            label="修改意见（打回重做时必填）"
            hint="提交修改意见后任务将重新激活，供 AI 继续改进。"
            :disabled="busy"
          />
          <div class="ui-actions mt-4">
            <BaseButton :busy="busy" @click="review(true)">验收通过</BaseButton>
            <BaseButton variant="secondary" :disabled="busy" @click="review(false)">打回并提交意见</BaseButton>
          </div>
        </div>

        <details class="ui-disclosure mt-5">
          <summary class="cursor-pointer text-xs">任务记录管理</summary>
          <div class="ui-disclosure-body">
            <p class="ui-muted">删除计划或重置任务状态仅清理 Planning 记录，不会影响源码文件或历史会话。</p>
            <div class="ui-actions">
              <BaseButton v-if="selected.kind === 'plan'" variant="danger" size="sm" :disabled="busy" @click="deleting = selected.item.id">
                删除此计划
              </BaseButton>
              <BaseButton variant="danger" size="sm" :disabled="busy" @click="resetOpen = true">重置全部任务状态</BaseButton>
            </div>
          </div>
        </details>
      </GlassCard>

      <GlassCard v-else class="min-w-0">
        <EmptyState
          :title="query.loading ? '正在加载任务' : '选择左侧任务查看详情'"
          description="目标和计划由 AI 会话发起创建，你可以在此处审查步骤进度、验证证据并完成人工验收。"
        />
        <details class="ui-disclosure mt-4">
          <summary class="cursor-pointer text-xs">任务记录管理</summary>
          <div class="ui-disclosure-body">
            <BaseButton variant="danger" size="sm" @click="resetOpen = true">重置全部任务状态</BaseButton>
          </div>
        </details>
      </GlassCard>
    </div>

    <ConfirmDialog
      :open="resetOpen"
      title="重置任务状态"
      message="清空当前工作区的目标、计划与执行状态。源码和历史会话保持不变。"
      severity="danger"
      :busy="busy"
      @confirm="reset"
      @cancel="resetOpen = false"
    />
    <ConfirmDialog
      :open="!!deleting"
      title="删除计划"
      message="删除此计划及其步骤记录，不修改源码和历史会话。"
      severity="danger"
      :busy="busy"
      @confirm="remove"
      @cancel="deleting = ''"
    />
  </div>
</template>

<style scoped>
.planning-inspector {
  grid-template-columns: 224px minmax(0, 1fr);
  gap: 16px;
}
@container subnav-stage (max-width: 640px) {
  .planning-inspector {
    grid-template-columns: minmax(0, 1fr);
  }
  .ui-task-list {
    max-height: 320px;
  }
}
@container workspace-page (max-width: 800px) {
  .planning-inspector {
    grid-template-columns: minmax(0, 1fr);
  }
  .ui-task-list {
    max-height: 320px;
  }
}
</style>
