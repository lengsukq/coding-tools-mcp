<script setup lang="ts">
import { computed, onBeforeUnmount, onErrorCaptured, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { RotateCw } from "@lucide/vue";
import BaseButton from "../components/ui/BaseButton.vue";
import ModalDialog from "../components/ui/ModalDialog.vue";
import PageTabs from "../components/ui/PageTabs.vue";
import {invalidateWorkspaceStatus} from "$lib/workspace-status-cache";
import {enqueueWorkspaceSettingsWrite} from "$lib/queued-writes";
import GlassCard from "../components/ui/GlassCard.vue";
import WorkspaceChanges from "../components/workspace/WorkspaceChanges.vue";
import HistoryContextPanel from "../components/workspace/HistoryContextPanel.vue";
import WorkspaceDiagnostics from "../components/workspace/WorkspaceDiagnostics.vue";
import WorkspaceConfiguration from "../components/workspace/WorkspaceConfiguration.vue";
import WorkspaceHeader from "../components/workspace/WorkspaceHeader.vue";
import WorkspaceOverview from "../components/workspace/WorkspaceOverview.vue";
import WorkspacePlanning from "../components/workspace/WorkspacePlanning.vue";


import { setLastWorkspace } from "$lib/api/settings";
import {
  listWorkspaces,
  deleteWorkspace,
  openWorkspaceDirectory,
  updateWorkspace,
} from "$lib/api/workspaces";
import { workspaces } from "$lib/stores/app";
import { showToast } from "$lib/stores/toast";
import {
  WORKSPACE_TABS,
  loadWorkspaceSnapshot,
  normalizeWorkspaceProfile,
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
const activeTab = computed<WorkspaceTab>(() => WORKSPACE_TABS.some(item=>item.value===route.params.tab)?route.params.tab as WorkspaceTab:"overview");
function navigate(tab: WorkspaceTab) { void router.push(`/workspace/${workspaceId.value}/${tab}`); }
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

const enqueueWorkspaceWrite=enqueueWorkspaceSettingsWrite;
function saveWorkspaceGroup(transform:(current:WorkspaceProfile)=>WorkspaceProfile){
  const id=workspaceId.value;
  return enqueueWorkspaceWrite(async()=>{
    const current=(await listWorkspaces()).find(item=>item.id===id);
    if(!current)throw new Error("工作区已不存在，请重新加载。");
    const next=transform(current);
    await updateWorkspace(next);
    if(next.path!==current.path)invalidateWorkspaceStatus(id);
    workspaces.value=workspaces.value.map(item=>item.id===id?next:item);
    if(workspaceId.value===id)profile.value=next;
  });
}
async function savePolicy(draft:RuntimePolicyDraft){await saveWorkspaceGroup(current=>withRuntimePolicy(current,draft));}
async function saveHistory(recording:boolean,sessions:number[]){await saveWorkspaceGroup(current=>withHistoryContext(current,recording,sessions));}
async function saveWorkspaceSettings(draft:{name:string;path:string;allowHighRiskWrites:boolean}){
 await saveWorkspaceGroup(current=>({...current,name:draft.name,path:draft.path,runtime:{...current.runtime,allow_high_risk_writes:draft.allowHighRiskWrites}}));
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
    <div class="ui-surface w-full max-w-lg rounded-[24px] p-6 text-center">
      <div class="mx-auto mb-3 grid size-11 place-items-center rounded-2xl bg-[var(--ui-danger-soft)] text-[var(--ui-danger)]">!</div>
      <h2 class="text-base font-semibold">工作区页面加载失败</h2>
      <p class="mt-2 break-words text-xs leading-5 text-[var(--text-muted)]">{{ loadError || renderError }}</p>
      <BaseButton class="mt-5" @click="load(workspaceId)">重新加载</BaseButton>
    </div>
  </div>
  <div v-else-if="!profile" class="flex min-h-0 flex-1 items-center justify-center overflow-y-auto">
    <div class="flex flex-col items-center gap-3 text-[var(--text-muted)]"><RotateCw :size="23" class="animate-spin text-[var(--primary)]" /><p class="text-xs">正在加载工作区数据…</p></div>
  </div>
  <section v-else class="min-h-0 flex-1 overflow-y-auto pb-14">
    <WorkspaceHeader
      :profile="profile"
      :path-copied="pathCopied"
      @reveal-directory="revealDirectory"
      @copy-path="copyPath"
    />
    <div class="workspace-shell pt-4">
      <PageTabs :items="WORKSPACE_TABS" :model-value="activeTab" aria-label="工作区页面" @update:model-value="navigate($event as WorkspaceTab)">
        <WorkspaceOverview v-if="activeTab==='overview'" :workspace-id="workspaceId" :profile="profile" @navigate="navigate" @reveal-directory="revealDirectory" />
        <WorkspacePlanning v-else-if="activeTab==='planning'" :workspace-id="workspaceId" @navigate="navigate" />
        <WorkspaceChanges v-else-if="activeTab==='changes'" :workspace-id="workspaceId" />
        <GlassCard v-else-if="activeTab==='history'"><div class="ui-section-heading"><div><h2>历史会话与上下文</h2><p>查看开发会话，选择需要注入 AI 上下文的历史。</p></div></div><HistoryContextPanel :workspace-id="workspaceId" :profile="profile" :on-save="saveHistory" :show-recording="false" /></GlassCard>
        <WorkspaceDiagnostics v-else-if="activeTab==='diagnostics'" :workspace-id="workspaceId" />
        <WorkspaceConfiguration v-else :workspace-id="workspaceId" :profile="profile" :on-save="saveWorkspaceSettings" :on-save-policy="savePolicy" :on-save-history="saveHistory" @delete="deleteConfirmOpen=true" />
      </PageTabs>
    </div>
  </section>

  <ModalDialog v-if="profile" :open="deleteConfirmOpen" title="删除工作区" :description="`确定删除「${profile.name}」？本地磁盘中的源码文件不会被删除。`" @close="deleteConfirmOpen = false">
    <div class="mb-4 rounded-2xl bg-[var(--ui-surface-subtle)] px-3 py-2.5 font-mono text-xs text-[var(--text-secondary)]">{{ profile.path }}</div>
    <div class="flex justify-end gap-2"><BaseButton variant="ghost" :disabled="deleteBusy" @click="deleteConfirmOpen = false">取消</BaseButton><BaseButton variant="danger" :busy="deleteBusy" @click="confirmDelete">确认删除</BaseButton></div>
  </ModalDialog>
  </div>
</template>

<style scoped>
.workspace-route {
  --workspace-content-max: var(--ui-page-max);
  --workspace-gutter: var(--ui-page-gutter);
  height:100%;
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

.workspace-tabs {
  width: 100%;
}

.workspace-shell {
  width: 100%;
  min-width: 0;
  max-width: var(--workspace-content-max);
  margin-inline: auto;
  padding-inline: var(--workspace-gutter);
  box-sizing: border-box;
}

.workspace-content {
  padding-bottom: 32px;
}

@container workspace-page (max-width: 760px) {
  .workspace-route {
    --workspace-gutter: 16px;
  }

  .workspace-content {
    padding-top: 16px;
  }
}
</style>
