<script setup lang="ts">
import { computed, onMounted, ref, onUnmounted } from "vue";
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
const busy = ref(false);
const error = ref('');
const pending = ref<WorkspaceReviewSummaryDto | null>(null);
let disposed = false;
const filtered = computed(() => reviews.value.filter(r => filter.value === 'all' || r.scope === filter.value));
const sessionOptions = computed(() => [{ value: '', label: '选择历史会话' }, ...sessions.value.filter(s => s.session_key).map(s => ({ value: s.session_key!, label: `#${s.number} ${s.title || '开发会话'}` }))]);
async function load() { error.value = ''; try {
    const [_, history] = await Promise.all([getCachedWorkspaceReviews(props.workspaceId, true), listHistorySessions(props.workspaceId)]);
    if (!disposed)
        sessions.value = history.sessions;
}
catch (e) {
    if (!disposed)
        error.value = String(e);
} }
async function generate(scope: 'workspace' | 'session') { if (busy.value)
    return; busy.value = true; error.value = ''; try {
    const review = scope === 'workspace' ? await createWorkspaceReviewUrl(props.workspaceId) : await createSessionReviewUrl(props.workspaceId, selectedSession.value);
    if (review) {
        await openUrl(review.url);
        await load();
    }
    else
        showToast('当前范围没有可展示的变更', { kind: 'info' });
}
catch (e) {
    error.value = String(e);
}
finally {
    busy.value = false;
} }
async function view(id: string) { try {
    await openUrl(await issueWorkspaceReviewUrl(props.workspaceId, id));
}
catch (e) {
    error.value = String(e);
} }
async function remove() { if (!pending.value || busy.value)
    return; busy.value = true; try {
    await deleteWorkspaceReview(props.workspaceId, pending.value.id);
    setCachedWorkspaceReviews(props.workspaceId, reviews.value.filter(r => r.id !== pending.value!.id));
    pending.value = null;
}
catch (e) {
    error.value = String(e);
}
finally {
    busy.value = false;
} }
function date(value: number) { return new Date(value * 1000).toLocaleString('zh-CN'); }
onMounted(() => void load());
onUnmounted(() => { disposed = true; });
</script>
<template><div class="ui-stack">
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
 <GlassCard>
<div class="ui-section-heading">
<h2>冻结快照</h2>
<BaseButton variant="ghost" size="sm" @click="load">刷新</BaseButton>
</div>
<SelectField v-model="filter" label="范围筛选" :options="[{value:'all',label:'全部'},{value:'workspace',label:'工作区'},{value:'session',label:'会话'},{value:'operation',label:'单次操作'}]"/>
<p v-if="error" class="ui-danger mt-4" role="alert">{{error}}</p>
<div v-for="review in filtered" :key="review.id" class="ui-data-row items-start">
<div class="min-w-0">
<strong class="block break-words">{{review.summary||'变更快照'}}</strong>
<p class="ui-muted">{{review.scope==='operation'?'单次操作':review.scope==='session'?'会话':'工作区'}} · {{review.files}} 个文件 · +{{review.additions}} −{{review.deletions}} · {{date(review.createdAt)}}</p>
<p class="ui-muted">{{review.sessionId?'关联会话：'+review.sessionId:'关联会话不可用'}} · 冻结快照</p>
</div>
<div class="ui-actions">
<BaseButton variant="secondary" size="sm" @click="view(review.id)">查看 Diff</BaseButton>
<details class="ui-row-menu">
<summary>管理</summary>
<BaseButton variant="danger" size="sm" @click="pending=review">删除快照</BaseButton>
</details>
</div>
</div>
<EmptyState v-if="!filtered.length&&!error" title="暂无变更快照" description="查看工作区 Diff，或选择有 AI 修改的历史会话生成快照。"/>
</GlassCard>
 <ConfirmDialog :open="!!pending" title="删除冻结快照" message="仅删除本地 Diff 快照，不修改源码或 Git 历史。删除后无法恢复。" severity="danger" :busy="busy" confirm-text="删除快照" @confirm="remove" @cancel="pending=null"/>
</div></template>
