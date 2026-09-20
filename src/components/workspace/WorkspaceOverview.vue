<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { Activity, ArrowRight, GitBranch, ShieldCheck, Target } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import StatusPill from "../ui/StatusPill.vue";
import type { PlanningStateDto } from "$lib/api/planning";
import { createWorkspaceReviewUrl, deleteWorkspaceReview, issueWorkspaceReviewUrl, openWorkspaceInIde, type DetectedIdeDto, type GlobalMcpOverviewDto, type WorkspaceActivityMetricsDto, type WorkspaceGitSummaryDto, type WorkspaceReviewSummaryDto } from "$lib/api/workspaces";
import {
  getCachedDetectedIdes,
  getCachedGlobalMcpOverview,
  getCachedPlanningState,
  getCachedWorkspaceActivityMetrics,
  getCachedWorkspaceGitSummary,
  getCachedWorkspaceReviews,
  peekCachedDetectedIdes,
  peekCachedGlobalMcpOverview,
  peekCachedPlanningState,
  peekCachedWorkspaceActivityMetrics,
  peekCachedWorkspaceGitSummary,
  peekCachedWorkspaceReviews,
  setCachedWorkspaceReviews,
} from "$lib/workspace-status-cache";
import { showToast } from "$lib/stores/toast";
import { openUrl } from "$lib/api/app-info";
import type { WorkspaceProfile } from "$lib/types";
import type { WorkspaceTab } from "$lib/workspace-page";

const props = defineProps<{ workspaceId: string; profile: WorkspaceProfile }>();
const emit = defineEmits<{ navigate: [tab: WorkspaceTab]; revealDirectory: [] }>();
const planning = ref<PlanningStateDto | null>(null);
const git = ref<WorkspaceGitSummaryDto | null>(null);
const mcp = ref<GlobalMcpOverviewDto | null>(null);
const activityMetrics = ref<WorkspaceActivityMetricsDto | null>(null);
const loading = ref(false);
const ides = ref<DetectedIdeDto[]>([]);
const reviews = ref<WorkspaceReviewSummaryDto[]>([]);
const creatingWorkspaceReview = ref(false);
const operationReviews = computed(() => reviews.value.filter(review => review.scope === "operation"));
const ideMenuOpen = ref(false);
let loadGeneration = 0;
const focusedGoal = computed(() => planning.value?.goals.find(item => item.id === planning.value?.focus_goal_id) ?? null);
const focusedPlan = computed(() => planning.value?.plans.find(item => item.id === planning.value?.focus_plan_id) ?? null);
const completedSteps = computed(() => focusedPlan.value?.steps.filter(step => step.status === "completed").length ?? 0);
const activeSessions = computed(() => mcp.value?.sessions.filter(session => session.workspaceId === props.workspaceId).length ?? 0);
const verificationCount = computed(() => planning.value?.execution.verification.length ?? 0);
const subGitChangedFiles = computed(() => git.value?.subRepositories.reduce((total, repo) => total + repo.changedFiles, 0) ?? 0);
const changedFiles = computed(() => {
  if (git.value && (git.value.available || git.value.subRepositories.length)) {
    return (git.value.available ? git.value.changedFiles : 0) + subGitChangedFiles.value;
  }
  return planning.value?.execution.changed_files.length ?? 0;
});
const activityDays = computed(() => {
  const days = Array.from({ length: 7 }, (_, index) => {
    const date = new Date();
    date.setHours(0, 0, 0, 0);
    date.setDate(date.getDate() - (6 - index));
    return {
      key: date.toDateString(),
      label: index === 6 ? "今天" : (date.getMonth() + 1) + "/" + date.getDate(),
      operations: 0,
      files: 0,
      additions: 0,
      deletions: 0,
    };
  });
  for (const event of activityMetrics.value?.events ?? []) {
    const date = new Date(event.createdAt * 1000);
    const key = new Date(date.getFullYear(), date.getMonth(), date.getDate()).toDateString();
    const bucket = days.find(day => day.key === key);
    if (!bucket) continue;
    bucket.operations += event.operations;
    bucket.files += event.files;
    bucket.additions += event.additions;
    bucket.deletions += event.deletions;
  }
  return days;
});
const activityMaxOperations = computed(() => Math.max(1, ...activityDays.value.map(day => day.operations)));
const activityTotals = computed(() => activityDays.value.reduce((total, day) => ({
  operations: total.operations + day.operations,
  files: total.files + day.files,
  additions: total.additions + day.additions,
  deletions: total.deletions + day.deletions,
}), { operations: 0, files: 0, additions: 0, deletions: 0 }));
const gitHealthRows = computed(() => {
  if (!git.value) return [];
  const rows = git.value.subRepositories.map(repo => ({
    name: repo.name,
    branch: repo.branch || "detached",
    changed: repo.changedFiles,
    ahead: repo.ahead,
    behind: repo.behind,
  }));
  if (git.value.available) {
    rows.unshift({
      name: "Workspace",
      branch: git.value.branch || "detached",
      changed: git.value.changedFiles,
      ahead: git.value.ahead,
      behind: git.value.behind,
    });
  }
  return rows;
});
const gitHealthMaxChanged = computed(() => Math.max(1, ...gitHealthRows.value.map(repo => repo.changed)));
const goalCriteriaProgress = computed(() => {
  const total = focusedGoal.value?.success_criteria.length ?? 0;
  const completed = focusedGoal.value?.success_criteria.filter(item => item.completed).length ?? 0;
  return { total, completed, percent: total ? Math.round(completed / total * 100) : 0 };
});
const planStepProgress = computed(() => {
  const total = focusedPlan.value?.steps.length ?? 0;
  return { total, completed: completedSteps.value, percent: total ? Math.round(completedSteps.value / total * 100) : 0 };
});

async function load(force = false) {
  const generation = ++loadGeneration;
  const workspaceId = props.workspaceId;

  const cachedPlanning = peekCachedPlanningState(workspaceId);
  const cachedGit = peekCachedWorkspaceGitSummary(workspaceId);
  const cachedActivity = peekCachedWorkspaceActivityMetrics(workspaceId);
  const cachedReviews = peekCachedWorkspaceReviews(workspaceId);
  const cachedMcp = peekCachedGlobalMcpOverview();
  const cachedIdes = peekCachedDetectedIdes();

  planning.value = cachedPlanning;
  git.value = cachedGit;
  activityMetrics.value = cachedActivity;
  reviews.value = cachedReviews ?? [];
  if (cachedMcp) mcp.value = cachedMcp;
  if (cachedIdes) ides.value = cachedIdes;
  loading.value = !cachedPlanning && !cachedGit && !cachedActivity && !cachedReviews;

  const assign = async <T,>(request: Promise<T>, apply: (value: T) => void, fallback: () => void) => {
    try {
      const value = await request;
      if (generation === loadGeneration) apply(value);
    } catch {
      if (generation === loadGeneration) fallback();
    }
  };

  const requests: Promise<void>[] = [
    assign(getCachedPlanningState(workspaceId, force), value => { planning.value = value; }, () => undefined),
    assign(getCachedWorkspaceGitSummary(workspaceId, force), value => { git.value = value; }, () => undefined),
    assign(getCachedGlobalMcpOverview(force), value => { mcp.value = value; }, () => undefined),
    assign(getCachedWorkspaceActivityMetrics(workspaceId, force), value => { activityMetrics.value = value; }, () => undefined),
    assign(getCachedWorkspaceReviews(workspaceId, force), value => { reviews.value = value; }, () => undefined),
    assign(getCachedDetectedIdes(force), value => { ides.value = value; }, () => undefined),
  ];
  await Promise.all(requests);
  if (generation === loadGeneration) loading.value = false;
}

async function viewWorkspaceReview() {
  if (creatingWorkspaceReview.value) return;
  creatingWorkspaceReview.value = true;
  try {
    const review = await createWorkspaceReviewUrl(props.workspaceId);
    if (!review) {
      showToast("当前工作区相对 HEAD 没有变更", { kind: "success" });
      return;
    }
    await openUrl(review.url);
    void load(true);
  } catch (error) {
    showToast(String(error), { title: "无法生成工作区 Diff", kind: "error" });
  } finally {
    creatingWorkspaceReview.value = false;
  }
}
async function removeReview(id: string) {
  await deleteWorkspaceReview(props.workspaceId, id);
  reviews.value = reviews.value.filter(r => r.id !== id);
  setCachedWorkspaceReviews(props.workspaceId, reviews.value);
}
async function viewReview(id: string) {
  try {
    const url = await issueWorkspaceReviewUrl(props.workspaceId, id);
    await openUrl(url);
  } catch (error) {
    showToast(String(error), { title: "无法打开 Review", kind: "error" });
  }
}
async function openIde(ide: DetectedIdeDto) {
  ideMenuOpen.value = false;
  await openWorkspaceInIde(props.workspaceId, ide.id);
}
onMounted(() => void load());
watch(() => props.workspaceId, () => void load());
</script>

<template>
  <div class="grid min-w-0 gap-4 overflow-x-hidden">
    <div class="workspace-overview-primary">
      <GlassCard>
        <div class="flex flex-wrap items-start justify-between gap-4">
          <div class="min-w-0"><p class="eyebrow">Workspace Status</p><h2 class="mt-1 text-sm font-semibold">代码与会话状态</h2><p class="mt-1 text-[11px] text-[var(--text-muted)]">这里保留需要快速判断的状态；名称、路径与目录操作统一放在上方 Workspace Header。</p></div>
          <div class="flex gap-2">
            <div v-if="ides.length" class="relative">
              <BaseButton size="sm" @click="ideMenuOpen = !ideMenuOpen">使用 IDE 打开<ArrowRight :size="13" /></BaseButton>
              <div v-if="ideMenuOpen" class="ide-menu">
                <button v-for="ide in ides" :key="ide.id" type="button" @click="openIde(ide)">{{ ide.name }}</button>
              </div>
            </div>
          </div>
        </div>
        <div class="overview-stat-grid mt-5">
          <div class="overview-stat">
            <div class="overview-stat-icon is-blue"><GitBranch :size="15" /></div>
            <strong>{{ git === null ? "—" : (git.available ? (git.branch || "detached") : (git.subRepositories.length ? git.subRepositories.length + " 个子 Git" : "非 Git")) }}</strong>
            <small>{{ git?.available ? '当前分支' : (git?.subRepositories.length ? '一级子仓库' : '当前目录') }}</small>
          </div>
          <button type="button" class="overview-stat is-interactive text-left" @click="viewWorkspaceReview">
            <div class="overview-stat-icon is-cyan"><Activity :size="15" /></div>
            <strong>{{ changedFiles }}</strong>
            <small>{{ creatingWorkspaceReview ? '正在生成 Diff…' : '变更文件 · 查看全部 Diff' }}</small>
          </button>
          <div class="overview-stat">
            <div class="overview-stat-icon is-purple"><Activity :size="15" /></div>
            <strong>{{ activeSessions }}</strong>
            <small>活跃会话</small>
          </div>
          <div class="overview-stat">
            <div class="overview-stat-icon is-green"><ShieldCheck :size="15" /></div>
            <strong>{{ verificationCount }}</strong>
            <small>验证证据</small>
          </div>
        </div>
        <p v-if="git?.available && git.lastCommit" class="mt-3 truncate text-[11px] text-[var(--text-secondary)]">最近提交 · {{ git.lastCommit }}</p>
        <div v-if="git?.subRepositories.length" class="mt-3">
          <div class="mb-1.5 flex items-center justify-between gap-3"><span class="text-[10px] font-medium text-[var(--text-muted)]">一级 Git 仓库</span><span class="text-[10px] text-[var(--text-muted)]">{{ git.subRepositories.length }} repositories</span></div>
          <div class="subgit-strip">
            <div v-for="repo in git.subRepositories" :key="repo.path" class="subgit-pill" :title="repo.path">
              <GitBranch :size="12" class="shrink-0 text-[var(--ios-blue)]" />
              <span class="font-semibold">{{ repo.name }}</span>
              <span class="text-[var(--text-muted)]">{{ repo.branch || 'detached' }}</span>
              <span v-if="repo.changedFiles" class="rounded-full bg-[#ff9f0a]/10 px-1.5 py-0.5 text-[9px] font-semibold text-[#d97706]">{{ repo.changedFiles }} changed</span>
              <span v-else class="text-[9px] text-[#30d158]">clean</span>
            </div>
          </div>
        </div>
      </GlassCard>
      <GlassCard>
        <div class="flex items-center justify-between"><h3 class="text-sm font-semibold">运行状态</h3><StatusPill :status="mcp?.state ?? 'unknown'" :label="mcp?.state ?? '未知'" /></div>
        <div class="mt-4 space-y-2 text-xs">
          <div class="overview-row"><span>Tool Profile</span><strong>{{ profile.runtime.tool_profile }}</strong></div>
          <div class="overview-row"><span>权限模式</span><strong>{{ profile.runtime.permission_mode }}</strong></div>
          <div class="overview-row"><span>History</span><strong>{{ profile.runtime.history_recording === false ? "关闭" : "开启" }}</strong></div>
          <div class="overview-row"><span>Planning</span><strong>{{ planning?.mode ?? "—" }}</strong></div>
        </div>
      </GlassCard>
    </div>
    <div class="workspace-overview-metrics">
      <GlassCard class="min-h-[220px]">
        <div class="flex items-start justify-between gap-3">
          <div><p class="eyebrow">AI Activity · 7 Days</p><h3 class="mt-1 text-sm font-semibold">修改趋势</h3></div>
          <div class="text-right"><strong class="block text-lg tabular-nums">{{ activityTotals.operations }}</strong><span class="text-[10px] text-[var(--text-muted)]">operations</span></div>
        </div>
        <div class="activity-chart mt-5">
          <div
            v-for="day in activityDays"
            :key="day.key"
            class="activity-day"
            :title="day.label + ' · ' + day.operations + ' ops · ' + day.files + ' files · +' + day.additions + ' -' + day.deletions"
          >
            <div class="activity-bar-wrap">
              <div class="activity-bar" :style="{ height: Math.max(day.operations ? 12 : 3, day.operations / activityMaxOperations * 100) + '%' }" />
            </div>
            <strong>{{ day.operations }}</strong>
            <span>{{ day.label }}</span>
          </div>
        </div>
        <div class="mt-4 grid grid-cols-3 gap-2 border-t border-black/[.045] pt-3 text-center dark:border-white/[.05]">
          <div><strong class="block text-xs tabular-nums">{{ activityTotals.files }}</strong><span class="text-[9px] text-[var(--text-muted)]">files touched</span></div>
          <div><strong class="block text-xs tabular-nums text-[var(--accent-success)]">+{{ activityTotals.additions }}</strong><span class="text-[9px] text-[var(--text-muted)]">additions</span></div>
          <div><strong class="block text-xs tabular-nums text-[var(--accent-danger)]">-{{ activityTotals.deletions }}</strong><span class="text-[9px] text-[var(--text-muted)]">deletions</span></div>
        </div>
      </GlassCard>

      <GlassCard class="min-h-[220px]">
        <div class="flex items-start justify-between gap-3">
          <div><p class="eyebrow">Git Health</p><h3 class="mt-1 text-sm font-semibold">仓库状态</h3></div>
          <span class="text-[10px] text-[var(--text-muted)]">{{ gitHealthRows.length }} repos</span>
        </div>
        <div v-if="gitHealthRows.length" class="mt-4 space-y-3">
          <div v-for="repo in gitHealthRows.slice(0, 6)" :key="repo.name" class="git-health-row">
            <div class="flex min-w-0 items-center justify-between gap-3">
              <div class="min-w-0"><strong class="block truncate text-[11px]">{{ repo.name }}</strong><span class="text-[9px] text-[var(--text-muted)]">{{ repo.branch }} · ↑{{ repo.ahead }} ↓{{ repo.behind }}</span></div>
              <span class="shrink-0 text-[10px] tabular-nums" :class="repo.changed ? 'text-[var(--accent-warning)]' : 'text-[var(--accent-success)]'">{{ repo.changed ? repo.changed + ' changed' : 'clean' }}</span>
            </div>
            <div class="git-health-track"><div class="git-health-fill" :class="{ clean: !repo.changed }" :style="{ width: (repo.changed ? Math.max(8, repo.changed / gitHealthMaxChanged * 100) : 4) + '%' }" /></div>
          </div>
        </div>
        <p v-else class="py-10 text-center text-xs text-[var(--text-muted)]">未检测到 Git 仓库</p>
      </GlassCard>

      <GlassCard class="min-h-[220px]">
        <div class="flex items-start justify-between gap-3">
          <div><p class="eyebrow">Execution Quality</p><h3 class="mt-1 text-sm font-semibold">执行与验证信号</h3></div>
          <StatusPill :status="planning?.execution.state ?? 'idle'" :label="planning?.execution.state ?? 'idle'" />
        </div>
        <div class="mt-4 grid grid-cols-2 gap-2">
          <div class="quality-metric"><span>Verification</span><strong>{{ verificationCount }}</strong><small>evidence</small></div>
          <div class="quality-metric" :class="{ danger: !!planning?.execution.last_error }"><span>Last Error</span><strong>{{ planning?.execution.last_error ? '1' : '0' }}</strong><small>{{ planning?.execution.last_error ? 'needs attention' : 'clear' }}</small></div>
        </div>
        <div class="mt-4 space-y-3">
          <div>
            <div class="mb-1 flex items-center justify-between text-[10px]"><span class="text-[var(--text-muted)]">Goal criteria</span><strong class="tabular-nums">{{ goalCriteriaProgress.completed }}/{{ goalCriteriaProgress.total }}</strong></div>
            <div class="quality-track"><div class="quality-fill" :style="{ width: goalCriteriaProgress.percent + '%' }" /></div>
          </div>
          <div>
            <div class="mb-1 flex items-center justify-between text-[10px]"><span class="text-[var(--text-muted)]">Plan steps</span><strong class="tabular-nums">{{ planStepProgress.completed }}/{{ planStepProgress.total }}</strong></div>
            <div class="quality-track"><div class="quality-fill purple" :style="{ width: planStepProgress.percent + '%' }" /></div>
          </div>
        </div>
      </GlassCard>
    </div>
    <GlassCard>
        <div class="mb-4 flex items-center justify-between"><div class="flex items-center gap-2"><Target :size="16" class="text-[var(--accent-purple)]" /><h3 class="text-sm font-semibold">当前工作</h3></div><BaseButton variant="ghost" size="sm" @click="emit('navigate', 'planning')">查看计划<ArrowRight :size="13" /></BaseButton></div>
        <template v-if="focusedPlan || focusedGoal">
          <p v-if="focusedGoal" class="eyebrow">Goal · {{ focusedGoal.status }}</p>
          <h4 class="mt-1 text-sm font-semibold">{{ focusedPlan?.title || focusedGoal?.title }}</h4>
          <p class="mt-1 line-clamp-2 text-[11px] leading-5 text-[var(--text-secondary)]">{{ focusedPlan?.objective || focusedGoal?.objective }}</p>
          <div v-if="focusedPlan" class="mt-4">
            <div class="mb-1.5 flex justify-between text-[10px] text-[var(--text-muted)]"><span>Plan Progress</span><span>{{ completedSteps }} / {{ focusedPlan.steps.length }}</span></div>
            <div class="h-1.5 overflow-hidden rounded-full bg-black/5 dark:bg-white/8"><div class="h-full rounded-full bg-[var(--accent-indigo)]" :style="{ width: (focusedPlan.steps.length ? completedSteps / focusedPlan.steps.length * 100 : 0) + '%' }" /></div>
          </div>
        </template>
        <p v-else class="py-6 text-center text-xs text-[var(--text-muted)]">当前没有 focused Goal / Plan</p>
    </GlassCard>
    <GlassCard>
      <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
        <div><h3 class="text-sm font-semibold">最近 AI 修改</h3><p class="mt-1 text-[10px] text-[var(--text-muted)]">这里展示 Operation Review；Session Diff 在 History 中，Workspace Diff 可直接从右侧生成。</p></div>
        <div class="flex items-center gap-2">
          <span class="text-[10px] text-[var(--text-muted)]">{{ operationReviews.length }} Reviews</span>
          <BaseButton variant="secondary" size="sm" :busy="creatingWorkspaceReview" @click="viewWorkspaceReview">查看工作区全部 Diff</BaseButton>
        </div>
      </div>
      <div v-if="operationReviews.length" class="space-y-2">
        <div v-for="review in operationReviews.slice(0, 5)" :key="review.id" class="overview-row min-w-0">
          <div class="review-summary min-w-0 flex-1">
            <strong class="review-summary-title">{{ review.summary || review.id }}</strong>
            <span class="block text-[10px] text-[var(--text-muted)]">{{ review.scope }} · {{ review.files }} files · +{{ review.additions }} -{{ review.deletions }} · {{ review.operationIds.length }} operations</span>
          </div>
          <div class="flex shrink-0 gap-1"><BaseButton variant="ghost" size="sm" @click="viewReview(review.id)">查看 Diff</BaseButton><BaseButton variant="ghost" size="sm" @click="removeReview(review.id)">删除</BaseButton></div>
        </div>
      </div>
      <p v-if="reviews.length" class="mt-2 text-[10px] text-[var(--text-muted)]">本地快照约 {{ Math.ceil((reviews[0]?.storageBytes || 0) / 1024) }} KB；聚合 Review 也是冻结快照，不会随着后续工作区变化而变化。</p>
      <p v-else class="py-4 text-center text-xs text-[var(--text-muted)]">暂无 AI Change Review</p>
    </GlassCard>
    <p v-if="loading" class="text-center text-[10px] text-[var(--text-muted)]">正在刷新 Workspace 摘要…</p>
  </div>
</template>

<style scoped>
.eyebrow { font-size:.625rem; font-weight:600; text-transform:uppercase; letter-spacing:.16em; color:var(--text-muted); }
.workspace-overview-primary {
  display: grid;
  grid-template-columns: minmax(0, 1.7fr) minmax(300px, .72fr);
  gap: 1rem;
  min-width: 0;
}
.workspace-overview-metrics {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 1rem;
  min-width: 0;
}
.overview-stat-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: .75rem;
  min-width: 0;
}
.overview-stat {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: .35rem;
  border-radius: 16px;
  border: 1px solid color-mix(in srgb, var(--text-main) 6%, transparent);
  background: color-mix(in srgb, var(--text-main) 2.5%, transparent);
  padding: .85rem;
  transition: all 200ms var(--ease-apple-spring);
}
.overview-stat.is-interactive { cursor: pointer; }
.overview-stat.is-interactive:hover,
.overview-stat:hover {
  transform: translateY(-2px);
  border-color: color-mix(in srgb, var(--ios-blue) 25%, transparent);
  background: color-mix(in srgb, var(--text-main) 4.5%, transparent);
}
.overview-stat strong {
  margin-top: .2rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: .92rem;
  font-weight: 750;
  letter-spacing: -0.02em;
  color: var(--text-main);
}
.overview-stat small {
  font-size: .65rem;
  color: var(--text-muted);
}
.overview-stat-icon {
  display: grid;
  width: 30px;
  height: 30px;
  place-items: center;
  border-radius: 9px;
  color: white;
}
.overview-stat-icon.is-blue {
  background: linear-gradient(135deg, var(--ios-blue), var(--ios-indigo));
  box-shadow: 0 3px 10px rgba(0, 113, 227, 0.28);
}
.overview-stat-icon.is-cyan {
  background: linear-gradient(135deg, var(--ios-cyan), var(--ios-blue));
  box-shadow: 0 3px 10px rgba(100, 210, 255, 0.28);
}
.overview-stat-icon.is-purple {
  background: linear-gradient(135deg, var(--ios-purple), #9333ea);
  box-shadow: 0 3px 10px rgba(191, 90, 242, 0.28);
}
.overview-stat-icon.is-green {
  background: linear-gradient(135deg, var(--ios-green), #16a34a);
  box-shadow: 0 3px 10px rgba(48, 209, 88, 0.28);
}
.overview-row { display:flex; align-items:center; justify-content:space-between; gap:1rem; border-bottom:1px solid color-mix(in srgb,var(--text-primary) 7%,transparent); padding:.42rem 0; color:var(--text-muted); }
.overview-row:last-child { border-bottom:0; }
.overview-row strong { color:var(--text-secondary); font-weight:600; }
.review-summary { max-width:100%; overflow-x:auto; overflow-y:hidden; overscroll-behavior-inline:contain; scrollbar-width:thin; scrollbar-color:color-mix(in srgb,var(--text-muted) 30%,transparent) transparent; padding-bottom:.15rem; }
.review-summary-title { display:block; width:max-content; min-width:100%; white-space:nowrap; color:var(--text-secondary); font-weight:600; }
.subgit-strip { display:flex; max-width:100%; gap:.45rem; overflow-x:auto; overscroll-behavior-inline:contain; padding:.15rem 0 .35rem; scrollbar-width:thin; scrollbar-color:color-mix(in srgb,var(--text-muted) 28%,transparent) transparent; }
.subgit-pill { display:flex; flex:0 0 auto; align-items:center; gap:.4rem; border:1px solid color-mix(in srgb,var(--text-primary) 7%,transparent); border-radius:.75rem; background:color-mix(in srgb,var(--text-primary) 2.5%,transparent); padding:.42rem .58rem; font-size:.625rem; color:var(--text-secondary); }
.activity-chart { display:grid; grid-template-columns:repeat(7,minmax(0,1fr)); align-items:end; gap:.45rem; height:92px; }
.activity-day { display:grid; height:100%; grid-template-rows:1fr auto auto; align-items:end; gap:.15rem; text-align:center; }
.activity-bar-wrap { display:flex; height:60px; align-items:flex-end; justify-content:center; }
.activity-bar { width:min(22px,72%); min-height:3px; border-radius:999px 999px 5px 5px; background:linear-gradient(180deg,var(--ios-blue),var(--accent-indigo)); box-shadow:0 6px 18px color-mix(in srgb,var(--ios-blue) 16%,transparent); transition:height .2s ease; }
.activity-day strong { font-size:.625rem; font-variant-numeric:tabular-nums; color:var(--text-secondary); }
.activity-day span { font-size:.55rem; white-space:nowrap; color:var(--text-muted); }
.git-health-row { min-width:0; }
.git-health-track,.quality-track { margin-top:.3rem; height:.34rem; overflow:hidden; border-radius:999px; background:color-mix(in srgb,var(--text-primary) 6%,transparent); }
.git-health-fill { height:100%; border-radius:inherit; background:linear-gradient(90deg,var(--accent-warning),#ffcc00); transition:width .2s ease; }
.git-health-fill.clean { background:var(--accent-success); }
.quality-metric { display:grid; grid-template-columns:1fr auto; align-items:baseline; gap:.15rem .5rem; border-radius:.85rem; background:color-mix(in srgb,var(--text-primary) 3%,transparent); padding:.65rem .7rem; }
.quality-metric span { grid-column:1/-1; font-size:.55rem; text-transform:uppercase; letter-spacing:.08em; color:var(--text-muted); }
.quality-metric strong { font-size:1rem; font-variant-numeric:tabular-nums; }
.quality-metric small { font-size:.55rem; color:var(--accent-success); }
.quality-metric.danger small { color:var(--accent-danger); }
.quality-fill { height:100%; border-radius:inherit; background:linear-gradient(90deg,var(--accent-success),#34c759); transition:width .2s ease; }
.quality-fill.purple { background:linear-gradient(90deg,var(--accent-indigo),var(--accent-purple)); }
.ide-menu { position:absolute; right:0; z-index:30; margin-top:.4rem; min-width:11rem; border:1px solid color-mix(in srgb,var(--text-primary) 10%,transparent); border-radius:.9rem; background:color-mix(in srgb,var(--surface) 94%,transparent); padding:.3rem; box-shadow:0 14px 36px rgba(0,0,0,.14); backdrop-filter:blur(24px); }
.ide-menu button { display:block; width:100%; border-radius:.65rem; padding:.5rem .65rem; text-align:left; font-size:.72rem; color:var(--text-secondary); }
.ide-menu button:hover { background:color-mix(in srgb,var(--text-primary) 6%,transparent); color:var(--text-primary); }

@container workspace-page (min-width: 1450px) {
  .workspace-overview-primary,
  .workspace-overview-metrics {
    gap: 1.15rem;
  }
}

@container workspace-page (max-width: 900px) {
  .workspace-overview-primary {
    grid-template-columns: 1fr;
  }

  .workspace-overview-metrics {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@container workspace-page (max-width: 620px) {
  .workspace-overview-metrics {
    grid-template-columns: 1fr;
  }

  .activity-chart {
    gap: .25rem;
  }

  .overview-stat-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@container workspace-page (max-width: 420px) {
  .overview-stat-grid {
    grid-template-columns: 1fr;
  }
}
</style>
