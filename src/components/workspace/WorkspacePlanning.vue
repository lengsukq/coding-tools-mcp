<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Archive, CheckCircle2, RefreshCw, RotateCcw, Target, Trash2 } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import ConfirmDialog from "../ui/ConfirmDialog.vue";
import GlassCard from "../ui/GlassCard.vue";
import SegmentedControl from "../ui/SegmentedControl.vue";
import StatusPill from "../ui/StatusPill.vue";
import {
  acceptGoalReview,
  acceptPlanReview,
  deletePlan,
  getPlanningState,
  rejectGoalReview,
  rejectPlanReview,
  resetPlanningState,
  setPlanningMode,
  type PlanDto,
  type PlanningMode,
  type PlanningStateDto,
} from "$lib/api/planning";
import { showToast } from "$lib/stores/toast";

const props = defineProps<{ workspaceId: string }>();
const state = ref<PlanningStateDto | null>(null);
const busy = ref(false);
const deleteBusy = ref(false);
const pendingDeletePlan = ref<PlanDto | null>(null);
let refreshTimer = 0;
let refreshInFlight = false;
const modeItems = [{ value: "direct", label: "Direct" }, { value: "plan", label: "Plan" }, { value: "goal", label: "Goal" }];
const focusedGoal = computed(() => state.value?.goals.find((item) => item.id === state.value?.focus_goal_id) ?? null);
const focusedPlan = computed(() => state.value?.plans.find((item) => item.id === state.value?.focus_plan_id) ?? null);
const reviewGoals = computed(() => state.value?.goals.filter((item) => item.status === "awaiting_acceptance") ?? []);
const reviewPlans = computed(() => state.value?.plans.filter((item) => item.status === "awaiting_acceptance") ?? []);

async function load() {
  busy.value = true;
  try { state.value = await getPlanningState(props.workspaceId); }
  catch (error) { showToast(String(error), { title: "加载 Planning 失败", kind: "error" }); }
  finally { busy.value = false; }
}
async function changeMode(value: string) { busy.value = true; try { state.value = await setPlanningMode(props.workspaceId, value as PlanningMode); } finally { busy.value = false; } }
async function reset() { busy.value = true; try { state.value = await resetPlanningState(props.workspaceId); showToast("Planning 已重置", { kind: "success" }); } finally { busy.value = false; } }
async function refreshSilently() {
  if (refreshInFlight) return;
  refreshInFlight = true;
  try {
    const next = await getPlanningState(props.workspaceId);
    if (!state.value || next.revision !== state.value.revision) state.value = next;
  } catch {
    // Manual refresh still surfaces errors. Background refresh stays quiet.
  } finally {
    refreshInFlight = false;
  }
}
async function confirmDeletePlan() {
  const plan = pendingDeletePlan.value;
  if (!plan || deleteBusy.value) return;
  deleteBusy.value = true;
  try {
    state.value = await deletePlan(props.workspaceId, plan.id);
    pendingDeletePlan.value = null;
    showToast(`Plan「${plan.title}」已删除`, { kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "删除 Plan 失败", kind: "error" });
  } finally {
    deleteBusy.value = false;
  }
}
async function review(kind: "goal" | "plan", id: string, accept: boolean) {
  busy.value = true;
  try {
    if (kind === "goal") {
      if (accept) await acceptGoalReview(props.workspaceId, id);
      else await rejectGoalReview(props.workspaceId, id, "请根据桌面端反馈继续调整");
    } else if (accept) {
      await acceptPlanReview(props.workspaceId, id);
    } else {
      await rejectPlanReview(props.workspaceId, id, "请根据桌面端反馈继续调整");
    }
    await load();
  } finally { busy.value = false; }
}
onMounted(() => { void load(); });
onMounted(() => {
  refreshTimer = window.setInterval(() => {
    if (document.visibilityState === "visible" && !busy.value && !deleteBusy.value) {
      void refreshSilently();
    }
  }, 2000);
});
onUnmounted(() => window.clearInterval(refreshTimer));
</script>

<template>
  <div class="grid gap-4">
    <GlassCard>
      <div class="flex items-center justify-between gap-5"><div><div class="flex items-center gap-2"><Target :size="18" class="text-[#bf5af2]" /><h2 class="text-sm font-semibold">Planning Control</h2></div><p class="mt-1 text-[11px] text-[var(--text-muted)]">模式和焦点状态直接读取项目内 Planning State。</p></div><div class="flex items-center gap-2"><SegmentedControl :items="modeItems" :model-value="state?.mode ?? 'direct'" @update:model-value="changeMode" /><BaseButton variant="ghost" size="sm" :busy="busy" @click="load"><RefreshCw :size="13" /></BaseButton><BaseButton variant="secondary" size="sm" :busy="busy" @click="reset"><RotateCcw :size="13" />重置</BaseButton></div></div>
    </GlassCard>

    <div class="grid grid-cols-2 gap-4">
      <GlassCard><div class="mb-3 flex items-center justify-between"><h3 class="text-xs font-semibold uppercase tracking-[.12em] text-[var(--text-muted)]">Focused Goal</h3><StatusPill v-if="focusedGoal" :status="focusedGoal.status" /></div><template v-if="focusedGoal"><h2 class="text-base font-semibold">{{ focusedGoal.title }}</h2><p class="mt-2 text-xs leading-5 text-[var(--text-secondary)]">{{ focusedGoal.objective }}</p><div class="mt-3 space-y-1.5"><div v-for="item in focusedGoal.success_criteria" :key="item.id" class="flex gap-2 text-[11px]"><CheckCircle2 :size="13" :class="item.completed ? 'text-[#30d158]' : 'text-[var(--text-muted)]'" /><span :class="item.completed ? 'text-[var(--text-secondary)] line-through opacity-70' : ''">{{ item.text }}</span></div></div></template><p v-else class="py-5 text-center text-xs text-[var(--text-muted)]">当前没有 focused Goal</p></GlassCard>
      <GlassCard><div class="mb-3 flex items-center justify-between"><h3 class="text-xs font-semibold uppercase tracking-[.12em] text-[var(--text-muted)]">Focused Plan</h3><StatusPill v-if="focusedPlan" :status="focusedPlan.status" /></div><template v-if="focusedPlan"><h2 class="text-base font-semibold">{{ focusedPlan.title }}</h2><p class="mt-2 text-xs leading-5 text-[var(--text-secondary)]">{{ focusedPlan.objective }}</p><div class="mt-3 space-y-1.5"><div v-for="(step, index) in focusedPlan.steps" :key="step.id" class="flex items-start gap-2 rounded-xl bg-black/[.022] px-2.5 py-2 text-[11px] dark:bg-white/[.035]"><span class="mt-0.5 text-[10px] text-[var(--text-muted)]">{{ index + 1 }}</span><span class="min-w-0 flex-1">{{ step.title }}</span><StatusPill :status="step.status" /></div></div></template><p v-else class="py-5 text-center text-xs text-[var(--text-muted)]">当前没有 focused Plan</p></GlassCard>
    </div>

    <GlassCard v-if="state?.plans.length">
      <div class="mb-3 flex items-center justify-between gap-3">
        <div>
          <h3 class="text-xs font-semibold uppercase tracking-[.12em] text-[var(--text-muted)]">Plans</h3>
          <p class="mt-1 text-[11px] text-[var(--text-muted)]">Plan 会自动刷新；可在这里删除单个计划，不影响源码和 History。</p>
        </div>
        <span class="text-[11px] text-[var(--text-muted)]">{{ state.plans.length }} 个</span>
      </div>
      <div class="space-y-2">
        <div v-for="plan in state.plans" :key="plan.id" class="flex items-center gap-3 rounded-2xl bg-black/[.022] px-3 py-2.5 dark:bg-white/[.035]">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <p class="truncate text-xs font-semibold">{{ plan.title }}</p>
              <StatusPill :status="plan.status" />
              <span v-if="plan.id === state.focus_plan_id" class="rounded-full bg-[#5e5ce6]/10 px-2 py-0.5 text-[9px] font-semibold text-[#5e5ce6]">Focused</span>
            </div>
            <p class="mt-1 line-clamp-1 text-[11px] text-[var(--text-secondary)]">{{ plan.objective }}</p>
            <p class="mt-1 text-[10px] text-[var(--text-muted)]">{{ plan.steps.length }} Steps · revision {{ plan.revision }}</p>
          </div>
          <BaseButton variant="ghost" size="sm" title="删除 Plan" @click="pendingDeletePlan = plan"><Trash2 :size="13" />删除</BaseButton>
        </div>
      </div>
    </GlassCard>
    <GlassCard v-else-if="state" class="text-center">
      <div class="mx-auto max-w-md py-5">
        <Target :size="22" class="mx-auto text-[var(--text-muted)]" />
        <h3 class="mt-3 text-sm font-semibold">还没有 Plan</h3>
        <p class="mt-1 text-xs leading-5 text-[var(--text-muted)]">在 Chat 中进入 Plan / Goal 模式并创建计划后，这里会自动刷新并展示步骤、进度与人工验收状态。</p>
      </div>
    </GlassCard>

    <GlassCard v-if="reviewGoals.length || reviewPlans.length"><div class="mb-3 flex items-center gap-2"><Archive :size="16" class="text-[#ff9f0a]" /><h2 class="text-sm font-semibold">等待人工验收</h2></div><div class="space-y-2"><div v-for="goal in reviewGoals" :key="goal.id" class="flex items-center gap-3 rounded-2xl bg-[#ff9f0a]/7 p-3"><div class="min-w-0 flex-1"><p class="text-xs font-semibold">Goal · {{ goal.title }}</p><p class="mt-1 line-clamp-2 text-[11px] text-[var(--text-secondary)]">{{ goal.review_summary || goal.objective }}</p></div><BaseButton variant="ghost" size="sm" @click="review('goal', goal.id, false)">退回</BaseButton><BaseButton size="sm" @click="review('goal', goal.id, true)">通过</BaseButton></div><div v-for="plan in reviewPlans" :key="plan.id" class="flex items-center gap-3 rounded-2xl bg-[#5e5ce6]/7 p-3"><div class="min-w-0 flex-1"><p class="text-xs font-semibold">Plan · {{ plan.title }}</p><p class="mt-1 line-clamp-2 text-[11px] text-[var(--text-secondary)]">{{ plan.review_summary || plan.objective }}</p></div><BaseButton variant="ghost" size="sm" @click="review('plan', plan.id, false)">退回</BaseButton><BaseButton size="sm" @click="review('plan', plan.id, true)">通过</BaseButton></div></div></GlassCard>

    <ConfirmDialog
      :open="!!pendingDeletePlan"
      title="删除 Plan"
      :message="pendingDeletePlan ? `确定删除「${pendingDeletePlan.title}」？` : ''"
      detail="会同步清理 Goal 关联、Focused Plan 和 Execution 中的引用；源码文件与 History 不会被删除。"
      confirm-text="确认删除"
      severity="danger"
      :busy="deleteBusy"
      @confirm="confirmDeletePlan"
      @cancel="pendingDeletePlan = null"
    />
  </div>
</template>
