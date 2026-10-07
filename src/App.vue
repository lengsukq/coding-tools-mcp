<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { open } from "@tauri-apps/plugin-dialog";
import BaseButton from "$src/components/ui/BaseButton.vue";
import AppShell from "$src/components/AppShell.vue";
import CloseConfirmDialog from "$src/components/CloseConfirmDialog.vue";
import UnsavedChangesDialog from "./components/UnsavedChangesDialog.vue";
import {confirmUnsavedChanges} from "$lib/unsaved-changes";
import ToastHost from "$src/components/ToastHost.vue";
import WorkspaceNavItem from "$src/components/WorkspaceNavItem.vue";
import {
  createWorkspace,
  getRuntimeStatus,
  listWorkspaces,
  restoreRuntimeState,
} from "$lib/api/workspaces";
import { globalMcpRuntimeState, workspaces } from "$lib/stores/app";
import { showToast } from "$lib/stores/toast";
import { startUiMemoryGuard } from "$lib/ui-memory-guard";
import { startCloseGuard } from "$lib/close-guard";
import {
  dashboardPreferences,
  loadDashboardPreferences,
  sortWorkspaceIds,
} from "$lib/dashboard-preferences";

const router = useRouter();
const route = useRoute();
const closeConfirmOpen = ref(false);
const workspaceLoadError=ref("");const workspaceLoadBusy=ref(false);

const sidebarWorkspaces = computed(() => {
  const orderedIds = sortWorkspaceIds(
    workspaces.value.map((workspace) => workspace.id),
    dashboardPreferences.value,
  );
  const byId = new Map(workspaces.value.map((workspace) => [workspace.id, workspace]));
  return orderedIds.map((id) => byId.get(id)).filter(Boolean);
});

const settingsActive = computed(() => route.path.startsWith("/settings"));
const dashboardActive = computed(() => route.path === "/");
const auditActive = computed(() => route.path === "/audit");
async function refreshWorkspaces() {
  if(workspaceLoadBusy.value)return;
  workspaceLoadBusy.value=true;
  try{workspaces.value=await listWorkspaces();workspaceLoadError.value="";
    try{globalMcpRuntimeState.value=(await getRuntimeStatus()).state;}catch{globalMcpRuntimeState.value="unknown";}
  }catch(error){workspaceLoadError.value=String(error);throw error;}
  finally{workspaceLoadBusy.value=false;}

}

async function addWorkspace() {
  try {
    const selected = await open({ directory: true, multiple: false });
    if (!selected || Array.isArray(selected)) return;
    const profile = await createWorkspace(selected);
    await refreshWorkspaces();
    await router.push(`/workspace/${profile.id}`);
  } catch (error) {
    showToast(String(error), {
      title: "添加工作区失败",
      kind: "error",
      duration: 8000,
    });
  }
}

function openWorkspace(id: string) {
  if (route.params.id === id) return;
  void router.push(`/workspace/${id}`);
}

let stopMemoryGuard: (() => void) | undefined;
let stopCloseGuard: (() => void) | undefined;
const handleAddWorkspace = () => void addWorkspace();

onMounted(() => {
  document.title = "Coding Tools MCP";
  loadDashboardPreferences();
  stopMemoryGuard = startUiMemoryGuard();
  stopCloseGuard = startCloseGuard(() => {
    void confirmUnsavedChanges().then(leave => { if(leave) closeConfirmOpen.value = true; });
  });
  window.addEventListener("coding-tools:add-workspace", handleAddWorkspace);

  void (async () => {
    try {
      await restoreRuntimeState();
    } catch (error) {
      showToast(String(error), {
        title: "恢复上次运行状态失败",
        kind: "warning",
        duration: 8000,
      });
    }
    try {
      await refreshWorkspaces();
    } catch (error) {
      showToast(String(error), { title: "加载工作区失败", kind: "error" });
    }
  })();
});

onUnmounted(() => {
  stopMemoryGuard?.();
  stopCloseGuard?.();
  window.removeEventListener("coding-tools:add-workspace", handleAddWorkspace);
});
</script>

<template>
  <div class="relative min-h-screen w-full font-body overflow-x-hidden">
    <AppShell
      :dashboard-active="dashboardActive"
      :settings-active="settingsActive"
      :audit-active="auditActive"
      @open-dashboard="router.push('/')"
      @open-audit="router.push('/audit')"
      @add-workspace="addWorkspace"
      @open-settings="router.push('/settings/general')"
      @open-health="router.push({path:'/settings/connection',query:{panel:'health'}})"
    >
      <template #sidebar>
        <div class="ui-sidebar-workspaces">
          <WorkspaceNavItem
            v-for="workspace in sidebarWorkspaces"
            :key="workspace!.id"
            :workspace="workspace!"
            :active="route.params.id === workspace!.id"
            @click="openWorkspace(workspace!.id)"
          />
        </div>
      </template>

      <div class="flex h-full min-h-0 flex-col"><div v-if="workspaceLoadError" class="ui-banner ui-banner--warning m-4" role="alert"><span>工作区列表加载失败：{{workspaceLoadError}}</span><BaseButton variant="secondary" size="sm" :busy="workspaceLoadBusy" @click="refreshWorkspaces().catch(()=>{})">重试</BaseButton></div><div class="min-h-0 flex-1">
      <RouterView v-slot="{ Component }">
        <component :key="route.name === 'workspace' ? String(route.params.id) : undefined" :is="Component" />
      </RouterView></div></div>
    </AppShell>

    <ToastHost />
    <UnsavedChangesDialog />
    <CloseConfirmDialog v-model:open="closeConfirmOpen" />
  </div>
</template>
