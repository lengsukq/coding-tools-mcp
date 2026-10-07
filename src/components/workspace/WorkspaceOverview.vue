<script setup lang="ts">
import { useRouter } from "vue-router";
import { computed, onMounted, ref } from "vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import StatusPill from "../ui/StatusPill.vue";
import EmptyState from "../ui/EmptyState.vue";
import { useWorkspaceStatus, getCachedDetectedIdes, getCachedGlobalMcpOverview, peekCachedGlobalMcpOverview,globalResourceState,getCachedWorkspaceActivityMetrics,peekCachedWorkspaceActivityMetrics } from "$lib/workspace-status-cache";
import { openWorkspaceInIde, type DetectedIdeDto } from "$lib/api/workspaces";
import { showToast } from "$lib/stores/toast";
import { taskFocus, statusPresentation } from "$lib/ui-model";
import type { WorkspaceProfile } from "$lib/types";
import type { WorkspaceTab } from "$lib/workspace-page";
const router = useRouter();
const props = defineProps<{
    workspaceId: string;
    profile: WorkspaceProfile;
}>();
const emit = defineEmits<{
    navigate: [
        tab: WorkspaceTab
    ];
    revealDirectory: [
    ];
}>();
const { planning, git, query, gitQuery, refresh } = useWorkspaceStatus(() => props.workspaceId);
const focus = computed(() => taskFocus(planning.value));
const ides = ref<DetectedIdeDto[]>([]);
const globalQuery=computed(globalResourceState);
const activity=computed(()=>peekCachedWorkspaceActivityMetrics(props.workspaceId));
const activityError=ref("");
async function loadActivity(){try{await getCachedWorkspaceActivityMetrics(props.workspaceId,true);activityError.value="";}catch(e){activityError.value=String(e);}}
const mcp = computed(() => peekCachedGlobalMcpOverview());
const changes = computed(() => git.value ? (git.value.available ? git.value.changedFiles : 0) + git.value.subRepositories.reduce((n, r) => n + r.changedFiles, 0) : null);
const pending = computed(() => [...(planning.value?.plans ?? []), ...(planning.value?.goals ?? [])].filter(i => i.status === 'awaiting_acceptance'));
async function openIde(id: string) { try {
    await openWorkspaceInIde(props.workspaceId, id);
}
catch (e) {
    showToast(String(e), { kind: 'error' });
} }
onMounted(() => { void getCachedDetectedIdes().then(v => ides.value = v).catch(() => { }); void getCachedGlobalMcpOverview().catch(() => { }); });
</script>
<template><div class="ui-stack">
 <div v-if="query.error||query.stale||gitQuery.error" class="ui-banner ui-banner--warning" role="status">
<span>摘要{{query.error||gitQuery.error?'更新失败':'已过期'}}，以下为上次数据。{{query.error||gitQuery.error}}</span>
<BaseButton variant="secondary" size="sm" @click="refresh">重试</BaseButton>
</div>
 <GlassCard v-if="pending.length||planning?.execution.last_error">
<div class="ui-section-heading">
<h2>需要处理</h2>
</div>
<button v-for="item in pending" :key="item.id" class="ui-list-action" @click="router.push({path:`/workspace/${workspaceId}/planning`,query:{item:item.id}})">
<strong>待验收 · {{item.title}}</strong>
<span>查看任务 →</span>
</button>
<button v-if="planning?.execution.last_error" class="ui-list-action" @click="emit('navigate','planning')">
<span>
<strong>最近执行异常</strong>
<small>{{planning.execution.last_error}}</small>
</span>
<span>查看详情 →</span>
</button>
</GlassCard>
 <GlassCard class="ui-focus-panel">
<div class="ui-section-heading">
<h2>当前任务</h2>
<StatusPill :status="query.error||query.stale?'unknown':focus.plan?.status??focus.goal?.status??planning?.execution.state"/>
</div>
<template v-if="focus.plan||focus.goal">
<h3 class="ui-focus-title">{{focus.title}}</h3>
<p class="ui-secondary line-clamp-2">{{focus.detail}}</p>
<p v-if="focus.step" class="mt-4">当前步骤：{{focus.step.title}}</p>
<p class="ui-muted mt-3">{{focus.progress.label}}</p>
<div class="ui-progress">
<span :style="{width:focus.progress.percent+'%'}"/>
</div>
<div class="ui-actions mt-4">
<BaseButton @click="emit('navigate','planning')">查看任务 →</BaseButton>
<BaseButton variant="secondary" @click="emit('navigate','changes')">查看变更与证据</BaseButton>
</div>
</template>
<EmptyState v-else :title="query.loading?'正在加载任务':'暂无当前任务'" description="通过 AI 会话发起目标或计划后，可以在这里查看进度。"/>
</GlassCard>
 <div class="ui-main-grid">
<GlassCard>
<div class="ui-section-heading">
<h2>代码与会话</h2>
<BaseButton variant="ghost" size="sm" @click="refresh">刷新</BaseButton>
</div>
<div class="ui-metric-grid">
<div>
<span>当前分支</span>
<strong>{{git?.available?(git.branch||'分离 HEAD'):git?.subRepositories.length?'子仓库':'—'}}</strong>
</div>
<button class="ui-metric-action" @click="emit('navigate','changes')">
<span>变更文件</span>
<strong>{{changes??'—'}}</strong>
</button>
<div>
<span>活跃会话</span>
<strong>{{mcp?.sessions.filter(s=>s.workspaceId===workspaceId).length??'—'}}</strong>
</div>
<button class="ui-metric-action" @click="emit('navigate','planning')">
<span>已有验证记录</span>
<strong>{{planning?.execution.verification.length??'—'}}</strong>
</button>
</div>
<p v-if="git?.lastCommit" class="ui-muted mt-4 break-words">最近提交 · {{git.lastCommit}}</p>
<div v-if="ides.length" class="ui-actions mt-4">
<BaseButton v-for="ide in ides" :key="ide.id" variant="secondary" size="sm" @click="openIde(ide.id)">使用 {{ide.name}} 打开</BaseButton>
</div>
<details v-if="git?.subRepositories.length" class="ui-disclosure mt-4">
<summary>子仓库状态 · {{git.subRepositories.length}}</summary>
<div class="ui-disclosure-body">
<div v-for="repo in git.subRepositories" :key="repo.path" class="ui-data-row">
<span>{{repo.name}} · {{repo.branch||'分离 HEAD'}}</span>
<span>{{repo.changedFiles}} 个变更 · ↑{{repo.ahead}} ↓{{repo.behind}}</span>
</div>
</div>
</details>
</GlassCard>
 <GlassCard>
<div class="ui-section-heading">
<h2>执行环境</h2>
<BaseButton variant="ghost" size="sm" @click="emit('navigate','settings')">配置</BaseButton>
</div>
<p v-if="globalQuery.error||globalQuery.stale" class="ui-muted">全局运行摘要待更新。</p><div class="ui-data-row">
<span>全局 MCP</span>
<StatusPill :status="globalQuery.error||globalQuery.stale?'unknown':mcp?.state"/>
</div>
<div class="ui-data-row">
<span>执行模式</span>
<strong>{{statusPresentation(planning?.mode).label}}</strong>
</div>
<div class="ui-data-row">
<span>工具档位</span>
<strong>{{profile.runtime.tool_profile}}</strong>
</div>
<div class="ui-data-row">
<span>权限</span>
<strong>{{profile.runtime.inherit_global_execution_policy?'继承全局':profile.runtime.permission_mode==='trusted'?'受信任':profile.runtime.permission_mode==='safe'?'安全受限':'完全放开'}}</strong>
</div>
<div class="ui-data-row">
<span>历史记录</span>
<strong>{{profile.runtime.history_recording===false?'关闭':'开启'}}</strong>
</div>
<p class="ui-muted mt-4">验证记录是执行证据，不代表所有验收条件已经通过。</p>
</GlassCard>
</div>
<details class="ui-disclosure" @toggle="($event.target as HTMLDetailsElement).open && loadActivity()"><summary>变更活动统计</summary><div class="ui-disclosure-body"><p v-if="activityError" class="ui-danger" role="alert">更新失败：{{activityError}}</p><div v-if="activity" class="ui-metric-grid"><div><span>操作</span><strong>{{activity.totalOperations}}</strong></div><div><span>文件</span><strong>{{activity.totalFiles}}</strong></div><div><span>新增行</span><strong>+{{activity.totalAdditions}}</strong></div><div><span>删除行</span><strong>−{{activity.totalDeletions}}</strong></div></div><EmptyState v-else title="暂无活动统计" description="启用历史记录后的操作会形成统计。"/></div></details></div></template>
