<script lang="ts">
  import {
    Bot,
    Crosshair,
    ListChecks,
    RefreshCw,
    RotateCcw,
    ShieldCheck,
  } from "@lucide/svelte";
  import { onDestroy } from "svelte";
  import {
    acceptGoalReview,
    acceptPlanReview,
    getPlanningState,
    rejectGoalReview,
    rejectPlanReview,
    resetPlanningState,
    setPlanningMode,
    type GoalDto,
    type PlanDto,
    type PlanningMode,
    type PlanningStateDto,
  } from "$lib/api/planning";
  import { showToast } from "$lib/stores/toast";
  import ConfirmDialog from "$lib/components/ui/ConfirmDialog.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import PlanningReviewQueue from "$lib/components/planning/PlanningReviewQueue.svelte";

  interface Props {
    workspaceId: string;
  }

  let { workspaceId }: Props = $props();
  let planning = $state<PlanningStateDto | null>(null);
  let loading = $state(false);
  let busy = $state(false);
  let error = $state("");
  let reviewFeedback = $state<Record<string, string>>({});
  let refreshTimer: ReturnType<typeof setInterval> | undefined;
  let loadSeq = 0;

  const focusedGoal = $derived(
    planning?.goals.find((goal) => goal.id === planning?.focus_goal_id) ?? null,
  );
  const focusedPlan = $derived(
    planning?.plans.find((plan) => plan.id === planning?.focus_plan_id) ?? null,
  );
  const pendingGoals = $derived(
    planning?.goals.filter((goal) => goal.status === "awaiting_acceptance") ?? [],
  );
  const pendingPlans = $derived(
    planning?.plans.filter((plan) => plan.status === "awaiting_acceptance") ?? [],
  );
  const activeGoalCount = $derived(
    planning?.goals.filter((goal) => ["active", "paused"].includes(goal.status)).length ?? 0,
  );
  const activePlanCount = $derived(
    planning?.plans.filter((plan) => ["draft", "active", "paused"].includes(plan.status)).length ?? 0,
  );
  const archivedCount = $derived(
    (planning?.goals.filter((goal) => goal.status === "archived").length ?? 0)
      + (planning?.plans.filter((plan) => plan.status === "archived").length ?? 0),
  );

  function modeDescription(mode: PlanningMode): string {
    if (mode === "plan") {
      return "只允许 AI 读取、分析和维护 Goal / Plan；项目写入与命令执行由 MCP 服务端阻断。";
    }
    if (mode === "goal") {
      return "允许项目修改，但 AI 的写操作必须绑定当前激活 Goal，并遵守当前 Plan。";
    }
    return "正常开发模式；AI 仍然可以按任务需要主动创建 Goal / Plan 进行持久化跟踪。";
  }

  function statusLabel(status: string): string {
    switch (status) {
      case "active":
        return "进行中";
      case "paused":
        return "已暂停";
      case "draft":
        return "草稿";
      case "awaiting_acceptance":
        return "待人工验收";
      case "archived":
        return "已归档";
      case "cancelled":
        return "已取消";
      case "completed":
        return "已完成（旧状态）";
      case "pending":
        return "待执行";
      case "in_progress":
        return "执行中";
      case "blocked":
        return "受阻";
      case "skipped":
        return "已跳过";
      default:
        return status;
    }
  }

  let resetConfirmOpen = $state(false);

  function requestResetPlanning() {
    resetConfirmOpen = true;
  }

  async function runBusyAction(
    action: () => Promise<void>,
    successMessage: string,
    errorTitle: string,
    successTitle = "AI Planning",
  ) {
    if (busy) return;
    busy = true;
    try {
      await action();
      showToast(successMessage, { title: successTitle, kind: "success" });
    } catch (err) {
      showToast(String(err), { title: errorTitle, kind: "error" });
    } finally {
      busy = false;
    }
  }

  async function handleConfirmReset() {
    await runBusyAction(async () => {
      planning = await resetPlanningState(workspaceId);
      error = "";
      reviewFeedback = {};
      resetConfirmOpen = false;
    }, "规划状态已重置。", "重置规划失败");
  }

  function feedbackFor(id: string): string {
    return reviewFeedback[id] ?? "";
  }

  function setFeedback(id: string, value: string) {
    reviewFeedback = { ...reviewFeedback, [id]: value };
  }

  async function load(silent = false) {
    if (!workspaceId) return;
    const seq = ++loadSeq;
    if (!silent) loading = true;
    try {
      const next = await getPlanningState(workspaceId);
      if (seq === loadSeq) {
        planning = next;
        error = "";
      }
    } catch (err) {
      if (seq === loadSeq && !silent) error = String(err);
    } finally {
      if (seq === loadSeq && !silent) loading = false;
    }
  }

  async function switchMode(mode: PlanningMode) {
    if (!planning || busy || planning.mode === mode) return;
    await runBusyAction(async () => {
      planning = await setPlanningMode(workspaceId, mode);
    }, "执行约束模式已更新。", "模式切换失败");
  }

  async function acceptGoal(goal: GoalDto) {
    await runBusyAction(async () => {
      await acceptGoalReview(workspaceId, goal.id);
      await load(true);
    }, "Goal 已验收并归档，关联 Plan 也已归档。", "Goal 验收失败", "人工验收完成");
  }

  async function rejectGoal(goal: GoalDto) {
    await runBusyAction(async () => {
      await rejectGoalReview(workspaceId, goal.id, feedbackFor(goal.id));
      setFeedback(goal.id, "");
      await load(true);
    }, "Goal 已打回为进行中，AI 可继续处理。", "Goal 打回失败", "已打回");
  }

  async function acceptPlan(plan: PlanDto) {
    await runBusyAction(async () => {
      await acceptPlanReview(workspaceId, plan.id);
      await load(true);
    }, "Plan 已验收并归档。", "Plan 验收失败", "人工验收完成");
  }

  async function rejectPlan(plan: PlanDto) {
    await runBusyAction(async () => {
      await rejectPlanReview(workspaceId, plan.id, feedbackFor(plan.id));
      setFeedback(plan.id, "");
      await load(true);
    }, "Plan 已打回为进行中，AI 可继续执行。", "Plan 打回失败", "已打回");
  }

  $effect(() => {
    workspaceId;
    void load();
    if (refreshTimer) clearInterval(refreshTimer);
    refreshTimer = setInterval(() => void load(true), 2000);
  });

  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
  });
</script>

<section class="tx-card p-4 sm:p-5" aria-labelledby="planning-control-title">
  <div class="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
    <div class="flex min-w-0 items-start gap-3">
      <span class="flex size-9 shrink-0 items-center justify-center rounded-[10px] bg-[var(--primary-soft)] text-[var(--primary)]">
        <Bot size={17} />
      </span>
      <div class="min-w-0">
        <div class="flex flex-wrap items-center gap-2">
          <h3 id="planning-control-title" class="text-sm font-semibold text-[var(--color-text)]">AI Planning</h3>
          {#if planning}
            <span class="tx-mono text-[10px] text-[var(--color-text-muted)]">rev {planning.revision}</span>
          {/if}
        </div>
        <p class="mt-1 max-w-3xl text-xs leading-5 text-[var(--color-text-muted)]">
          Goal / Plan 由 AI 在对话中按需要直接创建和维护。桌面端不再负责人工编写，只负责运行约束、查看进度和最终验收归档。
        </p>
      </div>
    </div>

    <div class="tx-planning-toolbar">
      {#if planning}
        <div class="tx-planning-mode-switch" aria-label="AI 执行约束模式">
          {#each ["direct", "plan", "goal"] as mode}
            <button
              type="button"
              class:active={planning.mode === mode}
              disabled={busy}
              onclick={() => void switchMode(mode as PlanningMode)}
            >
              {mode === "direct" ? "Direct" : mode === "plan" ? "Plan" : "Goal"}
            </button>
          {/each}
        </div>
      {/if}
      <Button
        variant="ghost"
        size="md"
        disabled={loading}
        busy={loading}
        onclick={() => void load()}
      >
        <RefreshCw size={13} class={loading ? "animate-spin" : ""} />
        <span>刷新</span>
      </Button>
      <Button
        variant="danger"
        size="md"
        disabled={busy}
        onclick={requestResetPlanning}
      >
        <RotateCcw size={13} />
        <span>重置规划</span>
      </Button>
    </div>
  </div>

  {#if error}
    <p class="mt-3 text-xs text-[var(--danger)]">{error}</p>
  {/if}

  {#if planning}
    <div class="mt-5 grid gap-4 xl:grid-cols-2">
      <div class="rounded-[12px] border border-[var(--color-border)] p-4">
        <p class="tx-section-label">AI 当前 Goal</p>
        {#if focusedGoal}
          <div class="mt-3">
            <div class="flex items-center justify-between gap-3">
              <strong class="text-sm">{focusedGoal.title}</strong>
              <span class="text-xs text-[var(--color-text-muted)]">{statusLabel(focusedGoal.status)}</span>
            </div>
            <p class="mt-2 text-xs leading-5 text-[var(--color-text-secondary)]">{focusedGoal.objective}</p>
            {#if focusedGoal.review_feedback}
              <p class="mt-3 rounded-[9px] bg-[var(--surface-hover)] px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                上次人工反馈：{focusedGoal.review_feedback}
              </p>
            {/if}
            <div class="mt-3 grid gap-2">
              {#each focusedGoal.success_criteria as criterion}
                <div class="flex items-start gap-2 text-xs text-[var(--color-text-secondary)]">
                  <span>{criterion.completed ? "✓" : "○"}</span>
                  <span>{criterion.text}</span>
                </div>
              {/each}
            </div>
          </div>
        {:else}
          <p class="mt-3 text-xs leading-5 text-[var(--color-text-muted)]">
            当前没有聚焦 Goal。AI 会在对话判断任务需要长期目标跟踪时主动调用 create_goal。
          </p>
        {/if}
      </div>

      <div class="rounded-[12px] border border-[var(--color-border)] p-4">
        <p class="tx-section-label">AI 当前 Plan</p>
        {#if focusedPlan}
          <div class="mt-3">
            <div class="flex items-center justify-between gap-3">
              <strong class="text-sm">{focusedPlan.title}</strong>
              <span class="text-xs text-[var(--color-text-muted)]">{statusLabel(focusedPlan.status)}</span>
            </div>
            <p class="mt-2 text-xs leading-5 text-[var(--color-text-secondary)]">{focusedPlan.objective}</p>
            {#if focusedPlan.review_feedback}
              <p class="mt-3 rounded-[9px] bg-[var(--surface-hover)] px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                上次人工反馈：{focusedPlan.review_feedback}
              </p>
            {/if}
            <div class="mt-3 grid gap-2">
              {#each focusedPlan.steps as step}
                <div class="flex items-start justify-between gap-3 text-xs text-[var(--color-text-secondary)]">
                  <span>{step.title}</span>
                  <span class="shrink-0 text-[var(--color-text-muted)]">{statusLabel(step.status)}</span>
                </div>
              {/each}
            </div>
          </div>
        {:else}
          <p class="mt-3 text-xs leading-5 text-[var(--color-text-muted)]">
            当前没有聚焦 Plan。AI 可以根据任务复杂度独立创建 Plan，或将 Plan 关联到当前 Goal。
          </p>
        {/if}
      </div>
    </div>

    <div class="tx-planning-mode-hint">
      {#if planning.mode === "plan"}
        <ShieldCheck size={14} />
      {:else if planning.mode === "goal"}
        <Crosshair size={14} />
      {:else}
        <ListChecks size={14} />
      {/if}
      <span>{modeDescription(planning.mode)}</span>
    </div>

    <div class="tx-planning-overview">
      <div class="tx-planning-focus-summary">
        <span>当前 Goal</span>
        <strong>{focusedGoal?.title ?? "暂无 Goal"}</strong>
        <small>{focusedGoal ? statusLabel(focusedGoal.status) : "AI 会在需要长期跟踪时创建"}</small>
      </div>
      <div class="tx-planning-focus-summary">
        <span>当前 Plan</span>
        <strong>{focusedPlan?.title ?? "暂无 Plan"}</strong>
        <small>{focusedPlan ? statusLabel(focusedPlan.status) : "复杂任务会拆成可追踪步骤"}</small>
      </div>
      <div class="tx-planning-stat-summary">
        <span><strong>{pendingGoals.length + pendingPlans.length}</strong> 待验收</span>
        <span><strong>{activeGoalCount + activePlanCount}</strong> 活跃</span>
        <span><strong>{archivedCount}</strong> 已归档</span>
      </div>
    </div>

    <PlanningReviewQueue
      goals={pendingGoals}
      plans={pendingPlans}
      {busy}
      {feedbackFor}
      onFeedback={setFeedback}
      onAcceptGoal={acceptGoal}
      onRejectGoal={rejectGoal}
      onAcceptPlan={acceptPlan}
      onRejectPlan={rejectPlan}
    />

  {/if}

  <ConfirmDialog
    open={resetConfirmOpen}
    title="重置 AI Planning 状态"
    message="确认重置当前工作区的 AI Planning？这将清空当前所有活跃 Goal、Plan、执行步骤记录与执行模式，此操作不可撤销，但绝不会修改项目源码。"
    confirmText="确认重置"
    cancelText="取消"
    severity="danger"
    busy={busy}
    onConfirm={handleConfirmReset}
    onCancel={() => {
      resetConfirmOpen = false;
    }}
  />
</section>
