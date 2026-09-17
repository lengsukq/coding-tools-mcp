<script setup lang="ts">
import { computed, onBeforeUnmount, onErrorCaptured, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { RotateCw } from "@lucide/vue";
import BaseButton from "../components/ui/BaseButton.vue";
import ModalDialog from "../components/ui/ModalDialog.vue";
import SegmentedControl from "../components/ui/SegmentedControl.vue";
import WorkspaceDiagnostics from "../components/workspace/WorkspaceDiagnostics.vue";
import WorkspaceHeader from "../components/workspace/WorkspaceHeader.vue";
import WorkspaceOverview from "../components/workspace/WorkspaceOverview.vue";
import WorkspacePlanning from "../components/workspace/WorkspacePlanning.vue";
import WorkspaceServices from "../components/workspace/WorkspaceServices.vue";
import WorkspaceSettings from "../components/workspace/WorkspaceSettings.vue";
import { setLastWorkspace } from "$lib/api/settings";
import {
  deleteWorkspace,
  openWorkspaceDirectory,
  updateWorkspace,
} from "$lib/api/workspaces";
import { workspaces } from "$lib/stores/app";
import { showToast } from "$lib/stores/toast";
import {
  WORKSPACE_TABS,
  loadWorkspaceSnapshot,
  refreshWorkspaceSnapshot,
  withHistoryContext,
  withRuntimePolicy,
  type RuntimePolicyDraft,
  type WorkspaceTab,
} from "$lib/workspace-page";
import type { WorkspaceProfile } from "$lib/types";

const route = useRoute();
const router = useRouter();
const profile = ref<WorkspaceProfile | null>(null);
const activeTab = ref<WorkspaceTab>("overview");
const pathCopied = ref(false);
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
    void setLastWorkspace(profile.value.id).catch(() => undefined);
  } catch (error) {
    if (generation !== loadGeneration || id !== workspaceId.value) return;
    profile.value = null;
    loadError.value = error instanceof Error ? error.message : String(error);
  }
}

async function refreshProfile(id = workspaceId.value): Promise<WorkspaceProfile | null> {
  if (!id) return null;
  const snapshot = await refreshWorkspaceSnapshot(id);
  if (id !== workspaceId.value) return null;
  workspaces.value = snapshot.items;
  profile.value = snapshot.profile;
  return profile.value;
}

async function revealDirectory() {
  if (!profile.value?.path) return;
  try { await openWorkspaceDirectory(profile.value.path); }
  catch (error) { showToast(String(error), { title: "打开目录失败", kind: "error" }); }
}

function copyPath() {
  if (!profile.value?.path) return;
  void navigator.clipboard.writeText(profile.value.path);
  pathCopied.value = true;
  setTimeout(() => { pathCopied.value = false; }, 1600);
}

async function savePolicy(draft: RuntimePolicyDraft) {
  if (!profile.value) return;
  const current = profile.value;
  const next = withRuntimePolicy(current, draft);
  await updateWorkspace(next);
  if (workspaceId.value !== current.id) return;
  profile.value = next;
}

async function saveHistory(recording: boolean, sessions: number[]) {
  if (!profile.value) return;
  const current = profile.value;
  const next = withHistoryContext(current, recording, sessions);
  await updateWorkspace(next);
  if (workspaceId.value !== current.id) return;
  profile.value = next;
}

async function saveWorkspaceSettings(draft: { name: string; path: string }) {
  if (!profile.value) return;
  const current = profile.value;
  if (current.name === draft.name && current.path === draft.path) return;
  const next = { ...current, name: draft.name, path: draft.path };
  await updateWorkspace(next);
  if (workspaceId.value !== current.id) return;
  profile.value = next;
  workspaces.value = workspaces.value.map((item) => item.id === next.id
    ? { ...item, name: next.name, path: next.path }
    : item);
}

async function confirmDelete() {
  if (!profile.value || !workspaceId.value || deleteBusy.value) return;
  deleteBusy.value = true;
  const id = workspaceId.value;
  try {
    await deleteWorkspace(id);
    workspaces.value = workspaces.value.filter((item) => item.id !== id);
    deleteConfirmOpen.value = false;
    await router.replace("/");
  } catch (error) {
    showToast(String(error), { title: "删除工作区失败", kind: "error" });
  } finally { deleteBusy.value = false; }
}

watch(workspaceId, (id) => {
  profile.value = null;
  activeTab.value = "overview";
  void load(id);
}, { immediate: true });

onErrorCaptured((error) => {
  renderError.value = error instanceof Error ? error.message : String(error);
  return false;
});

onBeforeUnmount(() => { loadGeneration += 1; });
</script>

<template>
  <div class="workspace-route">
  <div v-if="loadError || renderError" class="flex min-h-0 flex-1 items-center justify-center overflow-y-auto px-8">
    <div class="ios-glass-strong w-full max-w-lg rounded-[24px] p-6 text-center">
      <div class="mx-auto mb-3 grid size-11 place-items-center rounded-2xl bg-[#ff453a]/10 text-[#ff453a]">!</div>
      <h2 class="text-base font-semibold">工作区页面加载失败</h2>
      <p class="mt-2 break-words text-xs leading-5 text-[var(--text-muted)]">{{ loadError || renderError }}</p>
      <BaseButton class="mt-5" @click="load(workspaceId)">重新加载</BaseButton>
    </div>
  </div>
  <div v-else-if="!profile" class="flex min-h-0 flex-1 items-center justify-center overflow-y-auto">
    <div class="flex flex-col items-center gap-3 text-[var(--text-muted)]"><RotateCw :size="23" class="animate-spin text-[#0a84ff]" /><p class="text-xs">正在加载工作区数据…</p></div>
  </div>
  <section v-else class="min-h-0 flex-1 overflow-y-auto pb-14">
    <WorkspaceHeader
      :profile="profile"
      :path-copied="pathCopied"
      @reveal-directory="revealDirectory"
      @copy-path="copyPath"
    />
    <div class="sticky top-0 z-20 mt-3 border-y border-white/35 bg-white/38 px-7 py-2.5 backdrop-blur-2xl dark:border-white/6 dark:bg-black/15 sm:px-8">
      <SegmentedControl :items="WORKSPACE_TABS" :model-value="activeTab" @update:model-value="activeTab = $event as WorkspaceTab" />
    </div>
    <div class="mx-auto max-w-[1380px] px-7 pt-6 sm:px-8">
      <WorkspaceOverview
          v-if="activeTab === 'overview'"
          :workspace-id="workspaceId"
          :profile="profile"
          @navigate="activeTab = $event"
          @reveal-directory="revealDirectory"
        />
      <WorkspaceServices
          v-else-if="activeTab === 'services'"
          :workspace-id="workspaceId"
          :profile="profile"
          :on-save-policy="savePolicy"
          :on-save-history="saveHistory"
        />
      <WorkspaceDiagnostics v-else-if="activeTab === 'diagnostics'" :workspace-id="workspaceId" />
      <WorkspacePlanning v-else-if="activeTab === 'planning'" :workspace-id="workspaceId" />
      <WorkspaceSettings v-else :profile="profile" :on-save="saveWorkspaceSettings" @delete="deleteConfirmOpen = true" />
    </div>
  </section>

  <ModalDialog v-if="profile" :open="deleteConfirmOpen" title="删除工作区" :description="`确定删除「${profile.name}」？本地磁盘中的源码文件不会被删除。`" @close="deleteConfirmOpen = false">
    <div class="mb-4 rounded-2xl bg-black/[.035] px-3 py-2.5 font-mono text-xs text-[var(--text-secondary)] dark:bg-white/[.04]">{{ profile.path }}</div>
    <div class="flex justify-end gap-2"><BaseButton variant="ghost" :disabled="deleteBusy" @click="deleteConfirmOpen = false">取消</BaseButton><BaseButton variant="danger" :busy="deleteBusy" @click="confirmDelete">确认删除</BaseButton></div>
  </ModalDialog>
  </div>
</template>

<style scoped>
.workspace-route {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
}
</style>
