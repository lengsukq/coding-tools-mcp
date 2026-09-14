<script lang="ts">
  import { Archive, CheckCircle2, Crosshair, ListChecks, RotateCcw } from "@lucide/svelte";
  import type { GoalDto, PlanDto } from "$lib/api/planning";
  import Button from "$lib/components/ui/Button.svelte";

  interface Props {
    goals: GoalDto[];
    plans: PlanDto[];
    busy: boolean;
    feedbackFor: (id: string) => string;
    onFeedback: (id: string, value: string) => void;
    onAcceptGoal: (goal: GoalDto) => void | Promise<void>;
    onRejectGoal: (goal: GoalDto) => void | Promise<void>;
    onAcceptPlan: (plan: PlanDto) => void | Promise<void>;
    onRejectPlan: (plan: PlanDto) => void | Promise<void>;
  }

  let {
    goals,
    plans,
    busy,
    feedbackFor,
    onFeedback,
    onAcceptGoal,
    onRejectGoal,
    onAcceptPlan,
    onRejectPlan,
  }: Props = $props();
</script>

{#if goals.length > 0 || plans.length > 0}
  <div class="mt-5 rounded-[12px] border border-[var(--color-border)] p-4">
    <div class="flex items-start gap-3">
      <span class="flex size-8 shrink-0 items-center justify-center rounded-[9px] bg-[var(--primary-soft)] text-[var(--primary)]">
        <CheckCircle2 size={15} />
      </span>
      <div>
        <p class="tx-section-label">人工验收</p>
        <p class="mt-1 text-xs leading-5 text-[var(--color-text-muted)]">
          AI 完成工作后只能提交到这里等待验收。点击「验收并归档」才会真正关闭；打回后会重新激活，让 AI 在后续对话继续处理。
        </p>
      </div>
    </div>

    <div class="mt-4 grid gap-3">
      {#each goals as goal}
        <article class="rounded-[12px] border border-[var(--color-border)] p-4">
          <div class="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <Crosshair size={14} class="text-[var(--primary)]" />
                <strong class="text-sm">Goal · {goal.title}</strong>
              </div>
              <p class="mt-2 text-xs leading-5 text-[var(--color-text-secondary)]">{goal.objective}</p>
              {#if goal.review_summary}
                <div class="mt-3 rounded-[9px] bg-[var(--surface-hover)] px-3 py-2 text-xs leading-5 text-[var(--color-text-secondary)]">
                  <span class="font-medium text-[var(--color-text)]">AI 验收摘要：</span>{goal.review_summary}
                </div>
              {/if}
              {#if goal.success_criteria.length > 0}
                <div class="mt-3 grid gap-1.5 text-xs text-[var(--color-text-secondary)]">
                  {#each goal.success_criteria as criterion}
                    <div class="flex items-start gap-2">
                      <span>{criterion.completed ? "✓" : "○"}</span>
                      <span>{criterion.text}</span>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
            <Button variant="primary" size="sm" disabled={busy} onclick={() => void onAcceptGoal(goal)}>
              <Archive size={13} /> 验收并归档
            </Button>
          </div>
          <div class="mt-3 flex flex-col gap-2 border-t border-[var(--color-border)] pt-3 sm:flex-row">
            <input
              class="tx-input min-h-9 flex-1 text-xs"
              placeholder="可选：填写打回原因，AI 下次继续时可以参考"
              value={feedbackFor(goal.id)}
              oninput={(event) => onFeedback(goal.id, event.currentTarget.value)}
            />
            <Button variant="ghost" size="sm" disabled={busy} onclick={() => void onRejectGoal(goal)}>
              <RotateCcw size={13} /> 打回继续
            </Button>
          </div>
        </article>
      {/each}

      {#each plans as plan}
        <article class="rounded-[12px] border border-[var(--color-border)] p-4">
          <div class="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <ListChecks size={14} class="text-[var(--primary)]" />
                <strong class="text-sm">Plan · {plan.title}</strong>
              </div>
              <p class="mt-2 text-xs leading-5 text-[var(--color-text-secondary)]">{plan.objective}</p>
              {#if plan.review_summary}
                <div class="mt-3 rounded-[9px] bg-[var(--surface-hover)] px-3 py-2 text-xs leading-5 text-[var(--color-text-secondary)]">
                  <span class="font-medium text-[var(--color-text)]">AI 验收摘要：</span>{plan.review_summary}
                </div>
              {/if}
              {#if plan.steps.length > 0}
                <div class="mt-3 grid gap-1.5 text-xs text-[var(--color-text-secondary)]">
                  {#each plan.steps as step}
                    <div class="flex items-start gap-2">
                      <span>{step.status === "completed" ? "✓" : step.status === "skipped" ? "–" : "○"}</span>
                      <span>{step.title}</span>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
            <Button variant="primary" size="sm" disabled={busy} onclick={() => void onAcceptPlan(plan)}>
              <Archive size={13} /> 验收并归档
            </Button>
          </div>
          <div class="mt-3 flex flex-col gap-2 border-t border-[var(--color-border)] pt-3 sm:flex-row">
            <input
              class="tx-input min-h-9 flex-1 text-xs"
              placeholder="可选：填写打回原因"
              value={feedbackFor(plan.id)}
              oninput={(event) => onFeedback(plan.id, event.currentTarget.value)}
            />
            <Button variant="ghost" size="sm" disabled={busy} onclick={() => void onRejectPlan(plan)}>
              <RotateCcw size={13} /> 打回继续
            </Button>
          </div>
        </article>
      {/each}
    </div>
  </div>
{/if}
