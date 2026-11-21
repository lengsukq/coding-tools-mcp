<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onErrorCaptured, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  Activity,
  Bot,
  FolderCog,
  GitPullRequestDraft,
  History,
  LayoutDashboard,
  ListTodo,
  RotateCw,
  TerminalSquare,
} from "@lucide/vue";
import BaseButton from "../components/ui/BaseButton.vue";
import ModalDialog from "../components/ui/ModalDialog.vue";
import { evictWorkspaceStatus, invalidateWorkspaceStatus } from "$lib/workspace-status-cache";
import { enqueueWorkspaceSettingsWrite } from "$lib/queued-writes";
import GlassCard from "../components/ui/GlassCard.vue";
import WorkspaceHeader from "../components/workspace/WorkspaceHeader.vue";
import WorkspaceOverview from "../components/workspace/WorkspaceOverview.vue";
import { setLastWorkspace } from "$lib/api/settings";
import {
  listWorkspaces,
  deleteWorkspace,
  openWorkspaceDirectory,
  updateWorkspace,
} from "$lib/api/workspaces";
import { useClipboard } from "$lib/composables/use-clipboard";
import { workspaces } from "$lib/stores/app";
import { showToast } from "$lib/stores/toast";
import {
  WORKSPACE_TABS,
  loadWorkspaceSnapshot,
  normalizeWorkspaceProfile,
  withHistoryContext,
  withRuntimePolicy,
  type RuntimePolicyDraft,
  type WorkspaceTab,
} from "$lib/workspace-page";
import { WORKSPACE_SIDEBAR_GROUPS, type WorkspaceSubNavItem } from "$lib/global-search";
import type { WorkspaceProfile } from "$lib/types";

const WorkspacePlanning = defineAsyncComponent(() => import("../components/workspace/WorkspacePlanning.vue"));
const WorkspaceChanges = defineAsyncComponent(() => import("../components/workspace/WorkspaceChanges.vue"));
const HistoryContextPanel = defineAsyncComponent(() => import("../components/workspace/HistoryContextPanel.vue"));
const WorkspaceDiagnostics = defineAsyncComponent(() => import("../components/workspace/WorkspaceDiagnostics.vue"));
const WorkspaceConfiguration = defineAsyncComponent(() => import("../components/workspace/WorkspaceConfiguration.vue"));

const route = useRoute();
const router = useRouter();
const profile = ref<WorkspaceProfile | null>(null);

const activeTab = computed<WorkspaceTab>(() =>
  WORKSPACE_TABS.some((item) => item.value === route.params.tab)
    ? (route.params.tab as WorkspaceTab)
    : "overview",
);

const settingsPanel = computed<"project" | "policy" | "context">(() => {
  const p = String(route.query.panel ?? "project");
  return p === "policy" || p === "context" ? p : "project";
});

const activeSubItemId = computed(() => {
  if (activeTab.value !== "settings") return activeTab.value;
  return `settings-${settingsPanel.value}`;
});

function navigate(tab: WorkspaceTab) {
  void router.push(`/workspace/${workspaceId.value}/${tab}`);
}

function selectSubItem(item: WorkspaceSubNavItem) {
  void router.push({
    path: `/workspace/${workspaceId.value}/${item.tab}`,
    ...(item.panel ? { query: { panel: item.panel } } : {}),
  });
}

function subItemIcon(id: string) {
  switch (id) {
    case "planning":
      return ListTodo;
    case "changes":
      return GitPullRequestDraft;
    case "history":
      return History;
    case "diagnostics":
      return Activity;
    case "settings-project":
      return FolderCog;
    case "settings-policy":
      return TerminalSquare;
    case "settings-context":
      return Bot;
    default:
      return LayoutDashboard;
  }
}

const { copiedKey, copyText } = useClipboard(1600);
const pathCopied = computed(() => copiedKey.value === "workspace-path");
const deleteConfirmOpen = ref(false);
const deleteBusy = ref(false);
const loadError = ref("");
const renderError = ref("");
let loadGeneration = 0;

const workspaceId = computed(() => String(route.params.id ?? ""));

async function load(id = workspaceId.value) {
  if (!id) return;
  const generation = ++loadGeneration;
  loadError.value = "";
  renderError.value = "";
  try {
    const snapshot = await loadWorkspaceSnapshot(id);
    if (generation !== loadGeneration || id !== workspaceId.value) return;
    workspaces.value = snapshot.items;
    profile.value = snapshot.profile;
    if (!profile.value) {
      await router.replace("/");
      return;
    }
    void setLastWorkspace(profile.value.id).catch((err) => {
      console.warn("[WorkspaceView] failed to record last workspace:", err);
    });
  } catch (error) {
    if (generation !== loadGeneration || id !== workspaceId.value) return;
    profile.value = null;
    loadError.value = error instanceof Error ? error.message : String(error);
  }
}

async function refreshProfile(id = workspaceId.value): Promise<WorkspaceProfile | null> {
  if (!id) return null;
  const snapshot = await loadWorkspaceSnapshot(id);
  if (id !== workspaceId.value) return null;
  workspaces.value = snapshot.items;
  profile.value = snapshot.profile;
  return profile.value;
}

async function revealDirectory() {
  if (!profile.value?.path) return;
  try {
    await openWorkspaceDirectory(profile.value.path);
  } catch (error) {
    showToast(String(error), { title: "打开目录失败", kind: "error" });
  }
}

async function copyPath() {
  if (!profile.value?.path) return;
  await copyText(profile.value.path, { key: "workspace-path", label: "工作区路径", silentSuccess: true });
}

const enqueueWorkspaceWrite = enqueueWorkspaceSettingsWrite;
function saveWorkspaceGroup(transform: (current: WorkspaceProfile) => WorkspaceProfile) {
  const id = workspaceId.value;
  return enqueueWorkspaceWrite(async () => {
    const current = (await listWorkspaces()).find((item) => item.id === id);
    if (!current) throw new Error("工作区已不存在，请重新加载。");
    const next = transform(current);
    await updateWorkspace(next);
    if (next.path !== current.path) invalidateWorkspaceStatus(id);
    workspaces.value = workspaces.value.map((item) => (item.id === id ? next : item));
    if (workspaceId.value === id) profile.value = next;
  });
}

async function savePolicy(draft: RuntimePolicyDraft) {
  await saveWorkspaceGroup((current) => withRuntimePolicy(current, draft));
}
async function saveHistory(recording: boolean, sessions: number[]) {
  await saveWorkspaceGroup((current) => withHistoryContext(current, recording, sessions));
}
async function saveWorkspaceSettings(draft: { name: string; path: string; allowHighRiskWrites: boolean }) {
  await saveWorkspaceGroup((current) => ({
    ...current,
    name: draft.name,
    path: draft.path,
    runtime: { ...current.runtime, allow_high_risk_writes: draft.allowHighRiskWrites },
  }));
}

async function confirmDelete() {
  if (!profile.value || !workspaceId.value || deleteBusy.value) return;
  deleteBusy.value = true;
  const id = workspaceId.value;
  try {
    await deleteWorkspace(id);
    evictWorkspaceStatus(id);
    workspaces.value = workspaces.value.filter((item) => item.id !== id);
    deleteConfirmOpen.value = false;
    await router.replace("/");
  } catch (error) {
    showToast(String(error), { title: "删除工作区失败", kind: "error" });
  } finally {
    deleteBusy.value = false;
  }
}

watch(
  workspaceId,
  (id) => {
    const cached = workspaces.value.find((item) => item.id === id);
    if (cached) {
      profile.value = normalizeWorkspaceProfile(cached);
      loadError.value = "";
      renderError.value = "";
      void setLastWorkspace(id).catch(() => undefined);
      void refreshProfile(id).catch(() => undefined);
      return;
    }
    profile.value = null;
    void load(id);
  },
  { immediate: true },
);

watch(activeTab, () => {
  renderError.value = "";
});

onErrorCaptured((error) => {
  renderError.value = error instanceof Error ? error.message : String(error);
  return false;
});

onBeforeUnmount(() => {
  loadGeneration += 1;
});
</script>

<template>
  <div class="workspace-route">
    <div v-if="loadError || renderError" class="flex min-h-0 flex-1 items-center justify-center overflow-y-auto px-8">
      <div class="ui-surface w-full max-w-lg rounded-[24px] p-6 text-center">
        <div class="mx-auto mb-3 grid size-11 place-items-center rounded-2xl bg-[var(--ui-danger-soft)] text-[var(--ui-danger)]">!</div>
        <h2 class="text-base font-semibold">工作区页面加载失败</h2>
        <p class="mt-2 break-words text-xs leading-5 text-[var(--ui-text-muted)]">{{ loadError || renderError }}</p>
        <BaseButton class="mt-5" @click="load(workspaceId)">重新加载</BaseButton>
      </div>
    </div>

    <div v-else-if="!profile" class="flex min-h-0 flex-1 items-center justify-center overflow-y-auto">
      <div class="flex flex-col items-center gap-3 text-[var(--ui-text-muted)]">
        <RotateCw :size="23" class="animate-spin text-[var(--ui-accent)]" />
        <p class="text-xs">正在加载工作区数据…</p>
      </div>
    </div>

    <section v-else class="min-h-0 flex-1 overflow-y-auto pb-14">
      <WorkspaceHeader
        :profile="profile"
        :path-copied="pathCopied"
        @reveal-directory="revealDirectory"
        @copy-path="copyPath"
      />

      <div class="workspace-shell pt-4">
        <div class="ui-subnav-layout">
          <!-- Left Secondary Workspace Navigation Rail -->
          <aside class="ui-subnav-rail" aria-label="工作区子页面导航">
            <div v-for="group in WORKSPACE_SIDEBAR_GROUPS" :key="group.group" class="ui-subnav-group">
              <div class="ui-subnav-group__title">{{ group.group }}</div>
              <button
                v-for="item in group.items"
                :key="item.id"
                type="button"
                class="ui-subnav-item"
                :class="{ active: item.id === activeSubItemId }"
                :aria-current="item.id === activeSubItemId ? 'page' : undefined"
                @click="selectSubItem(item)"
              >
                <component :is="subItemIcon(item.id)" :size="15" class="shrink-0" />
                <span class="truncate">{{ item.label }}</span>
              </button>
            </div>
          </aside>

          <!-- Focused Single-Topic Stage -->
          <div :key="activeTab" class="ui-subnav-stage min-w-0">
            <WorkspaceOverview
              v-if="activeTab === 'overview'"
              :workspace-id="workspaceId"
              :profile="profile"
              @navigate="navigate"
            />
            <WorkspacePlanning
              v-else-if="activeTab === 'planning'"
              :workspace-id="workspaceId"
              @navigate="navigate"
            />
            <WorkspaceChanges v-else-if="activeTab === 'changes'" :workspace-id="workspaceId" />
            <GlassCard v-else-if="activeTab === 'history'">
              <div class="ui-section-heading">
                <div>
                  <h2>历史会话与上下文</h2>
                  <p>查看开发会话归档，按需勾选需要注入后续 AI 上下文的历史检查点。</p>
                </div>
              </div>
              <HistoryContextPanel
                :workspace-id="workspaceId"
                :profile="profile"
                :on-save="saveHistory"
                :show-recording="false"
              />
            </GlassCard>
            <WorkspaceDiagnostics v-else-if="activeTab === 'diagnostics'" :workspace-id="workspaceId" />
            <WorkspaceConfiguration
              v-else
              :workspace-id="workspaceId"
              :profile="profile"
              :panel="settingsPanel"
              :on-save="saveWorkspaceSettings"
              :on-save-policy="savePolicy"
              :on-save-history="saveHistory"
              @delete="deleteConfirmOpen = true"
            />
          </div>
        </div>
      </div>
    </section>

    <ModalDialog
      v-if="profile"
      :open="deleteConfirmOpen"
      title="移除工作区"
      :description="`确定移除「${profile.name}」？本地磁盘中的源码文件不会被删除。`"
      @close="deleteConfirmOpen = false"
    >
      <div class="mb-4 rounded-2xl bg-[var(--ui-surface-subtle)] px-3 py-2.5 font-mono text-xs text-[var(--text-secondary)]">
        {{ profile.path }}
      </div>
      <div class="flex justify-end gap-2">
        <BaseButton variant="ghost" :disabled="deleteBusy" @click="deleteConfirmOpen = false">取消</BaseButton>
        <BaseButton variant="danger" :busy="deleteBusy" @click="confirmDelete">确认移除</BaseButton>
      </div>
    </ModalDialog>
  </div>
</template>

<style scoped>
.workspace-route {
  --workspace-content-max: var(--ui-page-max);
  --workspace-gutter: var(--ui-page-gutter);
  height: 100%;
  display: flex;
  min-height: 0;
  min-width: 0;
  width: 100%;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  container-name: workspace-page;
  container-type: inline-size;
}

.workspace-shell {
  width: 100%;
  min-width: 0;
  max-width: var(--workspace-content-max);
  margin-inline: auto;
  padding-inline: var(--workspace-gutter);
  box-sizing: border-box;
}

@container workspace-page (max-width: 760px) {
  .workspace-route {
    --workspace-gutter: 16px;
  }
}
</style>
