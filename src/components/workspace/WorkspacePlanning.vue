<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import StatusPill from "../ui/StatusPill.vue";
import EmptyState from "../ui/EmptyState.vue";
import SelectField from "../ui/SelectField.vue";
import TextAreaField from "../ui/TextAreaField.vue";
import SegmentedControl from "../ui/SegmentedControl.vue";
import ConfirmDialog from "../ui/ConfirmDialog.vue";
import { acceptGoalReview, acceptPlanReview, rejectGoalReview, rejectPlanReview, resetPlanningState, deletePlan, setPlanningMode, type PlanningStateDto, type PlanDto, type GoalDto, type PlanningMode } from "$lib/api/planning";
import { useWorkspaceStatus, setCachedPlanningState, getCachedPlanningState } from "$lib/workspace-status-cache";
import { planProgress, taskFocus, statusPresentation } from "$lib/ui-model";
const props = defineProps<{
    workspaceId: string;
}>();
const emit = defineEmits<{
    navigate: [
        tab: 'changes' | 'diagnostics'
    ];
}>();
const route = useRoute();
const router = useRouter();
const { planning: state, query, refresh } = useWorkspaceStatus(() => props.workspaceId);
const busy = ref(false);
const error = ref('');
const filter = ref('unfinished');
const selectedId = ref('');
const feedback = ref<Record<string, string>>({});
const resetOpen = ref(false);
const deleting = ref('');
const modes = [{ value: 'direct', label: '直接执行' }, { value: 'plan', label: '按计划执行' }, { value: 'goal', label: '按目标执行' }];
const entries = computed(() => [...(state.value?.plans ?? []).map(item => ({ kind: 'plan' as const, item })), ...(state.value?.goals ?? []).map(item => ({ kind: 'goal' as const, item }))]);
const visible = computed(() => entries.value.filter(({ item }) => filter.value === 'unfinished' ? ['draft', 'active', 'paused', 'awaiting_acceptance'].includes(item.status) || (item.status==='completed' && (item.id === state.value?.focus_plan_id || item.id === state.value?.focus_goal_id)) : filter.value === 'ended' ? ['archived', 'cancelled', 'completed'].includes(item.status) : item.status === 'awaiting_acceptance'));
const selected = computed(() => entries.value.find(e => e.item.id === selectedId.value) ?? null);
const selectedPlan = computed(() => selected.value?.kind === 'plan' ? state.value?.plans.find(p => p.id === selected.value!.item.id) ?? null : null);
const parentGoal = computed(() => state.value?.goals.find(g => g.id === selectedPlan.value?.goal_id) ?? null);
const selectedGoal = computed(() => selected.value?.kind === 'goal' ? state.value?.goals.find(g => g.id === selected.value!.item.id) ?? null : null);
const executionMatches = computed(() => selected.value?.kind === 'plan' ? state.value?.execution.plan_id === selected.value.item.id : state.value?.execution.goal_id === selected.value?.item.id);
const progress = computed(() => planProgress(selectedPlan.value));
const pendingCount = computed(() => entries.value.filter(e => e.item.status === 'awaiting_acceptance').length);
watch([() => route.query.item, entries], () => { const desired = String(route.query.item ?? ''); if (desired && entries.value.some(e => e.item.id === desired)) {
    selectedId.value = desired;
    return;
} if (!entries.value.some(e => e.item.id === selectedId.value))
    selectedId.value = taskFocus(state.value).plan?.id ?? taskFocus(state.value).goal?.id ?? visible.value[0]?.item.id ?? ''; }, { immediate: true });
function select(id: string) { selectedId.value = id; void router.replace({ query: { ...route.query, item: id } }); }
async function action(run: () => Promise<PlanningStateDto | PlanDto | GoalDto>) {
    if (busy.value) return;
    const id = props.workspaceId;
    busy.value = true;
    error.value = '';
    let saved = false;
    try {
        const result = await run();
        saved = true;
        // Apply the authoritative mutation response before refreshing, so a failed
        // follow-up read cannot offer the same acceptance action a second time.
        if ('schema_version' in result) {
            setCachedPlanningState(id, result);
        } else if (state.value) {
            const current = state.value;
            setCachedPlanningState(id, 'steps' in result
                ? { ...current, plans: current.plans.map(item => item.id === result.id ? result : item) }
                : { ...current, goals: current.goals.map(item => item.id === result.id ? result : item) });
        }
        await getCachedPlanningState(id, true);
    } catch (e) {
        error.value = saved ? `操作已保存，状态刷新失败。请刷新查看最新状态：${String(e)}` : String(e);
    } finally {
        busy.value = false;
    }
}
async function changeMode(value: string) { await action(() => setPlanningMode(props.workspaceId, value as PlanningMode)); }
async function review(accept: boolean) { const target = selected.value; if (!target)
    return; const text = feedback.value[target.item.id]?.trim(); if (!accept && !text) {
    error.value = '请填写具体修改意见。';
    return;
} await action(() => target.kind === 'plan' ? (accept ? acceptPlanReview(props.workspaceId, target.item.id) : rejectPlanReview(props.workspaceId, target.item.id, text)) : (accept ? acceptGoalReview(props.workspaceId, target.item.id) : rejectGoalReview(props.workspaceId, target.item.id, text))); }
async function reset() { await action(() => resetPlanningState(props.workspaceId)); if (!error.value)
    resetOpen.value = false; }
async function remove() { await action(() => deletePlan(props.workspaceId, deleting.value)); if (!error.value)
    deleting.value = ''; }
</script>
<template><div class="ui-stack">
 <GlassCard>
<div class="ui-section-heading">
<div>
<h2>任务与验收</h2>
<p>跟踪 AI 会话发起的目标和计划，查看证据并提交验收反馈。</p>
</div>
<BaseButton variant="ghost" size="sm" @click="refresh">刷新</BaseButton>
</div>
<div class="ui-actions">
<SegmentedControl :items="modes" :model-value="state?.mode??'direct'" aria-label="执行模式" :disabled="busy||!state" @update:model-value="changeMode"/>
</div>
<p class="ui-muted mt-3">{{state?.mode==='direct'?'直接执行允许 AI 按当前权限开展工作。':state?.mode==='plan'?'按计划执行要求写操作关联计划和步骤。':'按目标执行要求写操作关联目标及其计划。'}}模式变更从后续工具调用起生效。</p>
</GlassCard>
 <div v-if="query.error||query.stale" class="ui-banner ui-banner--warning" role="status">
<span>{{query.error?'任务更新失败：'+query.error:'任务数据已过期，正在重新检查。'}}</span>
<BaseButton variant="secondary" size="sm" @click="refresh">重试</BaseButton>
</div>
<p v-if="error && selected?.item.status!=='awaiting_acceptance'" class="ui-danger" role="alert">{{error}}</p>
 <div class="ui-inspector-grid">
<GlassCard class="ui-task-list">
<SelectField v-model="filter" label="任务筛选" :options="[{value:'unfinished',label:'当前与未结束'},{value:'review',label:'待验收（'+pendingCount+'）'},{value:'ended',label:'已完成、归档与取消'}]"/>
<div class="mt-4">
<button v-for="entry in visible" :key="entry.kind+entry.item.id" class="ui-task-item" :class="{active:selectedId===entry.item.id}" @click="select(entry.item.id)">
<span class="ui-muted">{{entry.kind==='plan'?'计划':'目标'}}</span>
<strong>{{entry.item.title}}</strong>
<StatusPill :status="entry.item.status"/>
</button>
</div>
<EmptyState v-if="!visible.length" title="没有匹配的任务" description="可以调整筛选，或通过 AI 会话发起新的目标和计划。"/>
</GlassCard>
 <GlassCard v-if="selected">
<div class="ui-section-heading">
<div>
<p class="ui-muted">{{selected.kind==='plan'?'计划':'目标'}}</p>
<h2 class="break-words">{{selected.item.title}}</h2>
</div>
<StatusPill :status="selected.item.status"/>
</div>
<p v-if="parentGoal" class="ui-secondary">所属目标：{{parentGoal.title}}</p>
<details class="ui-disclosure">
<summary>任务说明</summary>
<p class="ui-disclosure-body whitespace-pre-wrap">{{selected.item.objective}}</p>
</details>
 <template v-if="selectedPlan">
<p class="ui-muted mt-4">{{progress.label}}</p>
<div class="ui-progress">
<span :style="{width:progress.percent+'%'}"/>
</div>
<ol class="ui-step-list">
<li v-for="(step,index) in selectedPlan.steps" :key="step.id">
<div class="ui-data-row">
<span class="min-w-0 break-words">{{index+1}}. {{step.title}}</span>
<StatusPill :status="step.status"/>
</div>
<details v-if="step.notes" class="ui-disclosure">
<summary>步骤备注</summary>
<p class="ui-disclosure-body whitespace-pre-wrap">{{step.notes}}</p>
</details>
</li>
</ol>
</template>
 <template v-if="selectedGoal">
<h3 class="mt-5">验收条件</h3>
<div v-for="criterion in selectedGoal.success_criteria" :key="criterion.id" class="ui-data-row">
<span>{{criterion.text}}</span>
<StatusPill :status="criterion.completed?'completed':'pending'"/>
</div>
<details v-if="selectedGoal.constraints.length" class="ui-disclosure">
<summary>约束条件</summary>
<ul class="ui-disclosure-body">
<li v-for="constraint in selectedGoal.constraints" :key="constraint">{{constraint}}</li>
</ul>
</details>
</template>
 <div class="ui-actions mt-5">
<BaseButton variant="secondary" @click="emit('navigate','changes')">查看变更快照</BaseButton>
<BaseButton variant="ghost" @click="emit('navigate','diagnostics')">查看诊断</BaseButton>
</div>
 <div v-if="executionMatches" class="ui-inset mt-4 p-4">
<h3>执行与验证记录</h3>
<p class="ui-secondary mt-2">执行状态：{{statusPresentation(state?.execution.state).label}}</p>
<p v-if="state?.execution.last_error" class="ui-danger break-words" role="alert">{{state.execution.last_error}}</p>
<p class="ui-muted mt-2">已有 {{state?.execution.verification.length??0}} 条记录，需结合变更与验收条件判断结果。</p>
<ul>
<li v-for="(record,index) in state?.execution.verification" :key="index" class="whitespace-pre-wrap break-words mt-2">{{record}}</li>
</ul>
</div>
<p v-else class="ui-muted mt-4">当前任务没有可关联的执行与验证记录；可到历史和变更页追溯已有信息。</p>
 <div v-if="selected.item.status==='awaiting_acceptance'" class="ui-review-box mt-5">
<h3>人工验收</h3>
<p v-if="error" class="ui-danger mt-3" role="alert">{{error}}</p>
<p class="ui-secondary mt-2">{{selected.item.review_summary||'请复核任务说明、变更和验证记录。'}}</p>
<TextAreaField v-model="feedback[selected.item.id]" label="修改意见" hint="要求继续修改时，请填写具体问题；提交失败会保留输入。" :disabled="busy"/>
<div class="ui-actions mt-4">
<BaseButton :busy="busy" @click="review(true)">验收通过</BaseButton>
<BaseButton variant="secondary" :disabled="busy" @click="review(false)">提交修改意见</BaseButton>
</div>
</div>
 <details class="ui-disclosure mt-5">
<summary>任务管理</summary>
<div class="ui-disclosure-body">
<p class="ui-muted">删除计划仅移除计划记录；重置清空当前 Planning 状态，不删除源码和历史会话。</p>
<div class="ui-actions">
<BaseButton v-if="selected.kind==='plan'" variant="danger" size="sm" :disabled="busy" @click="deleting=selected.item.id">删除此计划</BaseButton>
<BaseButton variant="danger" size="sm" :disabled="busy" @click="resetOpen=true">重置任务状态</BaseButton>
</div>
</div>
</details>
 </GlassCard>
<GlassCard v-else>
<EmptyState :title="query.loading?'正在加载任务':'选择任务查看详情'" description="任务和目标由 AI 会话创建。这里提供状态、变更入口和人工验收。"/>
<details class="ui-disclosure">
<summary>任务管理</summary>
<div class="ui-disclosure-body">
<BaseButton variant="danger" size="sm" @click="resetOpen=true">重置任务状态</BaseButton>
</div>
</details>
</GlassCard>
</div>
 <ConfirmDialog :open="resetOpen" title="重置任务状态" message="清空当前工作区的目标、计划与执行状态。源码和历史会话保持不变。" severity="danger" :busy="busy" @confirm="reset" @cancel="resetOpen=false"/>
 <ConfirmDialog :open="!!deleting" title="删除计划" message="删除此计划及其步骤记录，不修改源码和历史会话。" severity="danger" :busy="busy" @confirm="remove" @cancel="deleting=''"/>
</div></template>
