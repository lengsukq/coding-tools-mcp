<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { ArrowUp, ArrowDown, Pin, FolderOpen, Copy, Command, RefreshCw } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import PageHeader from "$src/components/ui/PageHeader.vue";
import PageShell from "$src/components/ui/PageShell.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import EmptyState from "$src/components/ui/EmptyState.vue";
import DashboardCommandPalette from "$src/components/dashboard/DashboardCommandPalette.vue";
import DashboardPreferencesPopover from "$src/components/dashboard/DashboardPreferencesPopover.vue";
import { dashboardPreferences, loadDashboardPreferences, sortWorkspaceIds, moveWorkspace, togglePinnedWorkspace, toggleDashboardModule, updateDashboardPreferences } from "$lib/dashboard-preferences";
import { workspaces } from "$lib/stores/app";
import { globalHealthSummary } from "$lib/stores/health";
import { getLastWorkspaceId } from "$lib/api/settings";
import { openWorkspaceDirectory } from "$lib/api/workspaces";
import { peekCachedPlanningState, peekCachedWorkspaceGitSummary, workspaceResourceState, statusRevision, getCachedPlanningState, getCachedWorkspaceGitSummary } from "$lib/workspace-status-cache";
import { taskFocus, statusPresentation } from "$lib/ui-model";
import { listHistorySessions, type HistorySessionSummary } from "$lib/api/history";
import { summarizeUsage, formatCount, buildUsagePoint, buildUsageChart, type UsagePoint } from "$lib/dashboard";
import {getServiceUsageStats, type ServiceUsageStats } from "$lib/api/usage";
import { showToast } from "$lib/stores/toast";
const router = useRouter();
const lastId = ref('');
const preferencesOpen = ref(false);
const commandOpen = ref(false);
const commandQuery = ref('');
const history = ref<Record<string, HistorySessionSummary[]>>({});
const usage = ref<Record<string, ServiceUsageStats[]>>({});
const analyticsOpen = ref(false);
const auxiliaryError = ref('');
let timer = 0;
let generation = 0;
let inFlight = false;
const ordered = computed(() => { statusRevision.value; const byId = new Map(workspaces.value.map(w => [w.id, w])); return sortWorkspaceIds([...byId.keys()], dashboardPreferences.value).map(id => { const workspace = byId.get(id)!; const state = peekCachedPlanningState(id); const git = peekCachedWorkspaceGitSummary(id); const query = workspaceResourceState(id); return { workspace, state, git, query, focus: taskFocus(state) }; }); });
const focused = computed(() => ordered.value.find(w => w.workspace.id === lastId.value && (w.focus.plan || w.focus.goal)) ?? ordered.value.find(w => w.focus.plan || w.focus.goal));
const attention = computed(() => ordered.value.flatMap(row => {
    const result: Array<{
        key: string;
        title: string;
        detail: string;
        path: string;
        query?: Record<string, string>;
    }> = [];
    const id = row.workspace.id;
    if (row.query.error || row.query.stale)
        result.push({ key: id + 'fetch', title: row.workspace.name + ' · 状态待更新', detail: row.query.error || '上次数据已过期，正在重新检查。', path: `/workspace/${id}/planning` });
    for (const item of [...(row.state?.plans ?? []), ...(row.state?.goals ?? [])].filter(i => i.status === 'awaiting_acceptance'))
        result.push({ key: id + item.id, title: row.workspace.name + ' · 待验收', detail: item.title, path: `/workspace/${id}/planning`, query: { item: item.id } });
    if (row.state?.execution.last_error || ['blocked', 'failed', 'error'].includes(row.state?.execution.state ?? ''))
        result.push({ key: id + 'error', title: row.workspace.name + ' · 执行需要处理', detail: row.state?.execution.last_error || '执行已阻塞，请查看诊断。', path: `/workspace/${id}/diagnostics` });
    return result;
}));
const activities = computed(() => ordered.value.flatMap(row => (history.value[row.workspace.id] ?? []).map(session => ({ workspace: row.workspace, session }))).sort((a, b) => new Date(b.session.updated_at ?? b.session.created_at ?? 0).getTime() - new Date(a.session.updated_at ?? a.session.created_at ?? 0).getTime()).slice(0, 6));
const samples=ref<UsagePoint[]>([]);
const chart=computed(()=>buildUsageChart(samples.value));
const totals = computed(() => summarizeUsage(usage.value));
const quickItems = computed(() => [{ label: '添加工作区', hint: '项目', run: () => addWorkspace() }, { label: 'MCP 连接设置', hint: '应用设置', run: () => router.push('/settings/connection') }, ...ordered.value.map(row => ({ label: '打开 ' + row.workspace.name, hint: '工作区', run: () => router.push(`/workspace/${row.workspace.id}`) }))]);
const filteredCommands = computed(() => quickItems.value.filter(x => x.label.toLowerCase().includes(commandQuery.value.toLowerCase())));
function addWorkspace() { window.dispatchEvent(new CustomEvent('coding-tools:add-workspace')); }
function visible(id: string) { return !dashboardPreferences.value.hiddenModules.includes(id as any); }
async function loadAuxiliary() { if (inFlight)
    return; inFlight = true; const id = ++generation; const items = [...workspaces.value]; try {
    const sessions = await Promise.all(items.map(async (w) => [w.id, (await listHistorySessions(w.id)).sessions] as const));
    if (id === generation)
        history.value = Object.fromEntries(sessions);
    auxiliaryError.value = '';
    if (analyticsOpen.value) {
        const next=Object.fromEntries(await Promise.all(items.map(async w=>[w.id,await getServiceUsageStats(w.id)] as const)));
        if (id === generation)
            {usage.value=next;samples.value=[...samples.value,buildUsagePoint(next,samples.value.at(-1))].slice(-60);}
    }
}
catch (error) {
    auxiliaryError.value = String(error);
}
finally {
    inFlight = false;
} }
async function refresh() { await Promise.allSettled(ordered.value.flatMap(row => [getCachedPlanningState(row.workspace.id, true), getCachedWorkspaceGitSummary(row.workspace.id, true)])); await loadAuxiliary(); }
async function reveal(path: string) { try {
    await openWorkspaceDirectory(path);
}
catch (e) {
    showToast(String(e), { kind: 'error' });
} }
async function copy(path: string) { try {
    await navigator.clipboard.writeText(path);
    showToast('路径已复制', { kind: 'success' });
}
catch (e) {
    showToast(String(e), { kind: 'error' });
} }
function keys(e: KeyboardEvent) { if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    commandOpen.value = !commandOpen.value;
} }
watch(() => workspaces.value.map(w => w.id).join(','), () => void loadAuxiliary());
watch(analyticsOpen, open => { if (open)
    void loadAuxiliary(); });
onMounted(() => { loadDashboardPreferences(); void getLastWorkspaceId().then(id => lastId.value = id).catch(() => { }); void loadAuxiliary(); timer = window.setInterval(() => { if (document.visibilityState === 'visible')
    void loadAuxiliary(); }, 15000); window.addEventListener('keydown', keys); });
onUnmounted(() => { generation++; window.clearInterval(timer); window.removeEventListener('keydown', keys); });
</script>
<template><section class="page-scroll" :data-density="dashboardPreferences.density">
<PageShell>
 <PageHeader kicker="工作台" title="今天的开发工作" description="先处理阻塞与待验收，再查看当前任务和代码变更。">
<template #actions>
<BaseButton variant="secondary" size="sm" @click="commandOpen=true">
<Command :size="15"/>快捷操作 ⌘K</BaseButton>
<BaseButton variant="ghost" size="sm" @click="refresh">
<RefreshCw :size="15"/>刷新</BaseButton>
<DashboardPreferencesPopover :open="preferencesOpen" :density="dashboardPreferences.density" :hidden-modules="dashboardPreferences.hiddenModules" @toggle-open="preferencesOpen=!preferencesOpen" @toggle-density="updateDashboardPreferences(p=>({...p,density:p.density==='compact'?'comfortable':'compact'}))" @toggle-module="toggleDashboardModule" />
</template>
</PageHeader>
 <GlassCard>
<div class="ui-section-heading">
<div>
<h2>需要处理</h2>
<p>待验收与异常始终可见。</p>
</div>
<span class="ui-muted">{{attention.length+(['warning','error'].includes(globalHealthSummary.state)?1:0)}} 项</span>
</div>
  <button v-if="globalHealthSummary.state==='warning'||globalHealthSummary.state==='error'" class="ui-list-action" @click="router.push('/settings/connection?panel=health')">
<span>
<strong>全局连接 · {{globalHealthSummary.label}}</strong>
<small>{{globalHealthSummary.detail}}</small>
</span>
<span>查看连接 →</span>
</button>
  <button v-for="item in attention" :key="item.key" class="ui-list-action" @click="router.push({path:item.path,query:item.query})">
<span>
<strong>{{item.title}}</strong>
<small>{{item.detail}}</small>
</span>
<span>{{item.path.endsWith('/diagnostics') ? '查看诊断' : '查看任务'}} →</span>
</button>
  <EmptyState v-if="attention.length===0&&!['warning','error'].includes(globalHealthSummary.state)" title="暂无需要处理的事项" description="待验收和执行异常会集中显示在这里。"/>
 </GlassCard>
 <GlassCard v-if="visible('focus')" class="ui-focus-panel">
<div class="ui-section-heading">
<h2>当前工作</h2>
<StatusPill v-if="focused" :status="focused?.query.error||focused?.query.stale?'unknown':focused?.focus.plan?.status??focused?.focus.goal?.status??focused?.state?.execution.state??'unknown'"/>
</div>
<template v-if="focused">
<p class="ui-muted">{{focused.workspace.name}} · {{statusPresentation(focused.state?.mode).label}}</p>
<h3 class="ui-focus-title">{{focused.focus.title}}</h3>
<p class="ui-secondary line-clamp-2">{{focused.focus.detail}}</p>
<p v-if="focused.focus.step" class="mt-3">当前步骤：{{focused.focus.step.title}}</p>
<p class="ui-muted mt-3">{{focused.focus.progress.label}}</p>
<div class="ui-progress" :aria-label="focused.focus.progress.label">
<span :style="{width:focused.focus.progress.percent+'%'}"/>
</div>
<div class="ui-actions mt-4">
<BaseButton @click="router.push(`/workspace/${focused.workspace.id}/planning`)">查看任务 →</BaseButton>
<BaseButton variant="secondary" @click="router.push(`/workspace/${focused.workspace.id}/changes`)">查看变更</BaseButton>
</div>
</template>
<EmptyState v-else title="暂无当前任务" description="通过 AI 会话发起目标或计划后，可以在这里跟踪进度和验收。"/>
</GlassCard>
 <GlassCard v-if="visible('workspaces')">
<div class="ui-section-heading">
<div>
<h2>工作区</h2>
<p>独立管理项目任务、变更与执行权限。</p>
</div>
<BaseButton variant="secondary" size="sm" @click="addWorkspace">添加工作区</BaseButton>
</div>
<EmptyState v-if="!ordered.length" title="还没有工作区" description="选择本地项目目录，开始管理 AI 开发过程。">
<BaseButton @click="addWorkspace">添加工作区</BaseButton>
</EmptyState>
 <div v-else class="ui-workspace-table">
<div class="ui-workspace-row ui-table-heading">
<span>名称</span>
<span>当前任务</span>
<span>执行状态</span>
<span>变更</span>
<span>操作</span>
</div>
<div v-for="(row,index) in ordered" :key="row.workspace.id" class="ui-workspace-row">
<button class="ui-text-link min-w-0" @click="router.push(`/workspace/${row.workspace.id}`)">{{row.workspace.name}}</button>
<span class="ui-truncate" :title="row.focus.title">{{row.focus.title}}</span>
<StatusPill :status="row.query.error?'error':row.query.stale?'unknown':row.state?.execution.state" :label="row.query.error?'更新失败':row.query.stale?'数据过期':undefined"/>
<button class="ui-text-link" @click="router.push(`/workspace/${row.workspace.id}/changes`)">{{row.git?(row.git.available?row.git.changedFiles:0)+row.git.subRepositories.reduce((n,r)=>n+r.changedFiles,0):'—'}}</button>
<details class="ui-row-menu">
<summary>更多</summary>
<div class="ui-menu">
<BaseButton variant="ghost" size="sm" @click="togglePinnedWorkspace(row.workspace.id)">
<Pin :size="14"/>{{dashboardPreferences.pinnedWorkspaceIds.includes(row.workspace.id)?'取消置顶':'置顶'}}</BaseButton>
<BaseButton variant="ghost" size="sm" :disabled="index===0" @click="moveWorkspace(row.workspace.id,-1,ordered.map(w=>w.workspace.id))">
<ArrowUp :size="14"/>上移</BaseButton>
<BaseButton variant="ghost" size="sm" :disabled="index===ordered.length-1" @click="moveWorkspace(row.workspace.id,1,ordered.map(w=>w.workspace.id))">
<ArrowDown :size="14"/>下移</BaseButton>
<BaseButton variant="ghost" size="sm" @click="reveal(row.workspace.path)">
<FolderOpen :size="14"/>打开目录</BaseButton>
<BaseButton variant="ghost" size="sm" @click="copy(row.workspace.path)">
<Copy :size="14"/>复制路径</BaseButton>
<code>{{row.workspace.path}}</code>
</div>
</details>
</div>
</div>
 </GlassCard>
 <GlassCard v-if="visible('activity')">
<div class="ui-section-heading">
<h2>最近活动</h2>
<span class="ui-muted">开发会话</span>
</div>
<p v-if="auxiliaryError" role="alert" class="ui-danger">历史加载失败：{{auxiliaryError}}</p>
<button v-for="item in activities" :key="item.workspace.id+item.session.number" class="ui-list-action" @click="router.push(`/workspace/${item.workspace.id}/history`)">
<span>
<strong>{{item.workspace.name}} · {{item.session.title||'开发会话'}}</strong>
<small class="line-clamp-2">{{item.session.latest_focus||'查看会话摘要与关联变更'}}</small>
</span>
<span>查看历史 →</span>
</button>
<EmptyState v-if="!activities.length&&!auxiliaryError" title="暂无最近活动" description="启用历史记录后的开发会话会显示在这里。"/>
</GlassCard>
 <details v-if="visible('usage')" class="ui-disclosure" @toggle="analyticsOpen=($event.target as HTMLDetailsElement).open">
<summary>用量分析 <span class="ui-muted">JSON 传输量估算</span>
</summary>
<div class="ui-disclosure-body">
<p v-if="auxiliaryError" class="ui-danger" role="alert">分析更新失败：{{auxiliaryError}}。保留上次数据。</p><p class="ui-secondary">以下 Token 根据 MCP 传输量估算，不代表模型账单。</p>
<div class="ui-metric-grid">
<div>
<span>估算 Tokens</span>
<strong>{{formatCount(totals.estimatedTokens)}}</strong>
</div>
<div>
<span>工具调用</span>
<strong>{{formatCount(totals.toolCallCount)}}</strong>
</div>
<div>
<span>请求数</span>
<strong>{{formatCount(totals.requestCount)}}</strong>
</div>
<div>
<span>请求错误</span>
<strong>{{formatCount(totals.errorCount)}}</strong>
</div>
</div>
<div><h3>传输量估算趋势</h3><p class="ui-muted mt-2">本次打开后的采样增量，约每 15 秒更新。</p><svg v-if="samples.length>1" viewBox="0 0 100 40" preserveAspectRatio="none" class="ui-usage-chart" role="img" aria-label="估算 Token 传输增量趋势"><path :d="chart.path" fill="none" stroke="var(--ui-accent)" stroke-width="1" vector-effect="non-scaling-stroke"/></svg><EmptyState v-else title="正在积累趋势样本" description="保持用量区域展开，下一次采样后显示趋势。"/><p class="ui-muted">估算输入 {{formatCount(totals.estimatedInputTokens)}} · 估算输出 {{formatCount(totals.estimatedOutputTokens)}} · 工具调用 {{formatCount(totals.estimatedToolCallTokens)}}</p></div><p v-for="row in ordered" :key="row.workspace.id" class="ui-data-row">
<span>{{row.workspace.name}}</span>
<strong>{{formatCount(summarizeUsage({[row.workspace.id]:usage[row.workspace.id]??[]}).estimatedTokens)}}</strong>
</p>
</div>
</details>
 <DashboardCommandPalette :open="commandOpen" :entries="filteredCommands" v-model:query="commandQuery" @close="commandOpen=false" @select="commandOpen=false;filteredCommands[$event]?.run()"/>
 </PageShell>
</section></template>
