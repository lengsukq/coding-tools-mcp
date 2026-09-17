<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { Activity, ArrowRight, FolderOpen, GitBranch, ListChecks, ShieldCheck, Target } from "@lucide/vue";
import BaseButton from "../ui/BaseButton.vue";
import GlassCard from "../ui/GlassCard.vue";
import StatusPill from "../ui/StatusPill.vue";
import { getPlanningState, type PlanningStateDto } from "$lib/api/planning";
import { detectInstalledIdes, getGlobalMcpOverview, getWorkspaceGitSummary, openWorkspaceInIde, type DetectedIdeDto, type GlobalMcpOverviewDto, type WorkspaceGitSummaryDto } from "$lib/api/workspaces";
import type { WorkspaceProfile } from "$lib/types";
import type { WorkspaceTab } from "$lib/workspace-page";

const props = defineProps<{ workspaceId: string; profile: WorkspaceProfile }>();
const emit = defineEmits<{ navigate: [tab: WorkspaceTab]; revealDirectory: [] }>();
const planning = ref<PlanningStateDto | null>(null);
const git = ref<WorkspaceGitSummaryDto | null>(null);
const mcp = ref<GlobalMcpOverviewDto | null>(null);
const loading = ref(false);
const ides = ref<DetectedIdeDto[]>([]);
const ideMenuOpen = ref(false);
let loadGeneration = 0;
const focusedGoal = computed(() => planning.value?.goals.find(item => item.id === planning.value?.focus_goal_id) ?? null);
const focusedPlan = computed(() => planning.value?.plans.find(item => item.id === planning.value?.focus_plan_id) ?? null);
const completedSteps = computed(() => focusedPlan.value?.steps.filter(step => step.status === "completed").length ?? 0);
const activeSessions = computed(() => mcp.value?.sessions.filter(session => session.workspaceId === props.workspaceId).length ?? 0);
const verificationCount = computed(() => planning.value?.execution.verification.length ?? 0);
const changedFiles = computed(() => git.value?.available ? git.value.changedFiles : (planning.value?.execution.changed_files.length ?? 0));

async function load() {
  const generation = ++loadGeneration;
  loading.value = true;
  try {
    const [p, g, m, i] = await Promise.allSettled([getPlanningState(props.workspaceId), getWorkspaceGitSummary(props.workspaceId), getGlobalMcpOverview(), detectInstalledIdes()]);
    if (generation !== loadGeneration) return;
    planning.value = p.status === "fulfilled" ? p.value : null;
    git.value = g.status === "fulfilled" ? g.value : null;
    mcp.value = m.status === "fulfilled" ? m.value : null;
    ides.value = i.status === "fulfilled" ? i.value : [];
  } finally {
    if (generation === loadGeneration) loading.value = false;
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
  <div class="grid gap-4">
    <div class="grid gap-4 xl:grid-cols-[1.35fr_.65fr]">
      <GlassCard>
        <div class="flex flex-wrap items-start justify-between gap-4">
          <div class="min-w-0"><p class="eyebrow">Workspace Overview</p><h2 class="mt-1 truncate text-xl font-semibold">{{ profile.name }}</h2><p class="mt-1 truncate font-mono text-[11px] text-[var(--text-muted)]">{{ profile.path }}</p></div>
          <div class="flex gap-2">
            <div v-if="ides.length" class="relative">
              <BaseButton size="sm" @click="ideMenuOpen = !ideMenuOpen">使用 IDE 打开<ArrowRight :size="13" /></BaseButton>
              <div v-if="ideMenuOpen" class="ide-menu">
                <button v-for="ide in ides" :key="ide.id" type="button" @click="openIde(ide)">{{ ide.name }}</button>
              </div>
            </div>
            <BaseButton variant="ghost" size="sm" @click="emit('revealDirectory')"><FolderOpen :size="14" />打开目录</BaseButton>
          </div>
        </div>
        <div class="mt-5 grid grid-cols-2 gap-2 sm:grid-cols-4">
          <div class="overview-stat"><GitBranch :size="15" /><strong>{{ git === null ? "—" : (git.available ? (git.branch || "detached") : "非 Git") }}</strong><small>当前分支</small></div>
          <div class="overview-stat"><Activity :size="15" /><strong>{{ changedFiles }}</strong><small>变更文件</small></div>
          <div class="overview-stat"><Activity :size="15" /><strong>{{ activeSessions }}</strong><small>活跃会话</small></div>
          <div class="overview-stat"><ShieldCheck :size="15" /><strong>{{ verificationCount }}</strong><small>验证证据</small></div>
        </div>
        <p v-if="git?.available && git.lastCommit" class="mt-3 truncate text-[11px] text-[var(--text-secondary)]">最近提交 · {{ git.lastCommit }}</p>
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
    <div class="grid gap-4 lg:grid-cols-2">
      <GlassCard>
        <div class="mb-4 flex items-center justify-between"><div class="flex items-center gap-2"><Target :size="16" class="text-[#bf5af2]" /><h3 class="text-sm font-semibold">当前工作</h3></div><BaseButton variant="ghost" size="sm" @click="emit('navigate', 'planning')">查看计划<ArrowRight :size="13" /></BaseButton></div>
        <template v-if="focusedPlan || focusedGoal">
          <p v-if="focusedGoal" class="eyebrow">Goal · {{ focusedGoal.status }}</p>
          <h4 class="mt-1 text-sm font-semibold">{{ focusedPlan?.title || focusedGoal?.title }}</h4>
          <p class="mt-1 line-clamp-2 text-[11px] leading-5 text-[var(--text-secondary)]">{{ focusedPlan?.objective || focusedGoal?.objective }}</p>
          <div v-if="focusedPlan" class="mt-4">
            <div class="mb-1.5 flex justify-between text-[10px] text-[var(--text-muted)]"><span>Plan Progress</span><span>{{ completedSteps }} / {{ focusedPlan.steps.length }}</span></div>
            <div class="h-1.5 overflow-hidden rounded-full bg-black/5 dark:bg-white/8"><div class="h-full rounded-full bg-[#5e5ce6]" :style="{ width: (focusedPlan.steps.length ? completedSteps / focusedPlan.steps.length * 100 : 0) + '%' }" /></div>
          </div>
        </template>
        <p v-else class="py-6 text-center text-xs text-[var(--text-muted)]">当前没有 focused Goal / Plan</p>
      </GlassCard>
      <GlassCard>
        <div class="mb-4 flex items-center gap-2"><ListChecks :size="16" class="text-[#0a84ff]" /><h3 class="text-sm font-semibold">执行与验证</h3></div>
        <div class="grid grid-cols-2 gap-2">
          <div class="overview-mini"><span>Execution</span><strong>{{ planning?.execution.state ?? "idle" }}</strong></div>
          <div class="overview-mini"><span>Changed</span><strong>{{ changedFiles }} files</strong></div>
          <div class="overview-mini"><span>Verified</span><strong>{{ verificationCount }} checks</strong></div>
          <div class="overview-mini"><span>Git Sync</span><strong v-if="git?.available">↑{{ git.ahead }} ↓{{ git.behind }}</strong><strong v-else>—</strong></div>
        </div>
        <p v-if="planning?.execution.last_error" class="mt-3 rounded-xl bg-[#ff453a]/7 px-3 py-2 text-[11px] text-[#ff453a]">{{ planning.execution.last_error }}</p>
      </GlassCard>
    </div>
    <GlassCard>
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div><h3 class="text-sm font-semibold">快捷操作</h3><p class="mt-1 text-[11px] text-[var(--text-muted)]">概览负责摘要与导航，详细操作仍在原模块完成。</p></div>
        <div class="flex flex-wrap gap-2">
          <BaseButton variant="secondary" size="sm" @click="emit('navigate', 'services')">项目上下文</BaseButton>
          <BaseButton variant="secondary" size="sm" @click="emit('navigate', 'diagnostics')">诊断与日志</BaseButton>
          <BaseButton size="sm" @click="emit('navigate', 'planning')">继续 Plan</BaseButton>
        </div>
      </div>
    </GlassCard>
    <p v-if="loading" class="text-center text-[10px] text-[var(--text-muted)]">正在刷新 Workspace 摘要…</p>
  </div>
</template>

<style scoped>
.eyebrow { font-size:.625rem; font-weight:600; text-transform:uppercase; letter-spacing:.16em; color:var(--text-muted); }
.overview-stat,.overview-mini { display:flex; min-width:0; flex-direction:column; gap:.25rem; border-radius:1rem; background:color-mix(in srgb,var(--text-primary) 4%,transparent); padding:.75rem; }
.overview-stat strong { margin-top:.15rem; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:.8rem; }
.overview-stat small,.overview-mini span { font-size:.625rem; color:var(--text-muted); }
.overview-row { display:flex; align-items:center; justify-content:space-between; gap:1rem; border-bottom:1px solid color-mix(in srgb,var(--text-primary) 7%,transparent); padding:.42rem 0; color:var(--text-muted); }
.overview-row:last-child { border-bottom:0; }
.overview-row strong { color:var(--text-secondary); font-weight:600; }
.overview-mini strong { font-size:.75rem; color:var(--text-primary); }
.ide-menu { position:absolute; right:0; z-index:30; margin-top:.4rem; min-width:11rem; border:1px solid color-mix(in srgb,var(--text-primary) 10%,transparent); border-radius:.9rem; background:color-mix(in srgb,var(--surface) 94%,transparent); padding:.3rem; box-shadow:0 14px 36px rgba(0,0,0,.14); backdrop-filter:blur(24px); }
.ide-menu button { display:block; width:100%; border-radius:.65rem; padding:.5rem .65rem; text-align:left; font-size:.72rem; color:var(--text-secondary); }
.ide-menu button:hover { background:color-mix(in srgb,var(--text-primary) 6%,transparent); color:var(--text-primary); }
</style>
