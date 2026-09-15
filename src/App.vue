<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { open } from "@tauri-apps/plugin-dialog";
import AppShell from "$src/components/AppShell.vue";
import CloseConfirmDialog from "$src/components/CloseConfirmDialog.vue";
import ToastHost from "$src/components/ToastHost.vue";
import WorkspaceNavItem from "$src/components/WorkspaceNavItem.vue";
import {
  createWorkspace,
  getRuntimeStatus,
  listWorkspaces,
  restoreRuntimeState,
} from "$lib/api/workspaces";
import { mcpRuntimeStates, workspaces } from "$lib/stores/app";
import { showToast } from "$lib/stores/toast";
import { startUiMemoryGuard } from "$lib/ui-memory-guard";
import { startCloseGuard } from "$lib/close-guard";
import type { RuntimeState } from "$lib/types";
import {
  dashboardPreferences,
  loadDashboardPreferences,
  sortWorkspaceIds,
} from "$lib/dashboard-preferences";

const router = useRouter();
const route = useRoute();
const closeConfirmOpen = ref(false);

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
const activeSettingsNav = computed(() => {
  if (route.path === "/settings/keys") return "keys";
  if (["/settings/gateway", "/settings/frp", "/settings/software"].includes(route.path)) {
    return "connection";
  }
  return "general";
});

async function refreshWorkspaces() {
  const items = await listWorkspaces();
  workspaces.value = items;
  const states: Record<string, RuntimeState> = {};
  await Promise.all(
    items.map(async (item) => {
      try {
        states[item.id] = (await getRuntimeStatus(item.id)).state;
      } catch {
        states[item.id] = "stopped";
      }
    }),
  );
  mcpRuntimeStates.value = states;
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
  if (route.path === `/workspace/${id}`) return;
  void router.push(`/workspace/${id}`);
}

function handleSettingsNavChange(value: string) {
  if (value === "keys") void router.push("/settings/keys");
  else if (value === "connection") void router.push("/settings/gateway");
  else void router.push("/settings/general");
}

let stopMemoryGuard: (() => void) | undefined;
let stopCloseGuard: (() => void) | undefined;
const handleAddWorkspace = () => void addWorkspace();

onMounted(() => {
  document.title = "Coding Tools MCP";
  loadDashboardPreferences();
  stopMemoryGuard = startUiMemoryGuard();
  stopCloseGuard = startCloseGuard(() => {
    closeConfirmOpen.value = true;
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
  <AppShell
    :dashboard-active="dashboardActive"
    :settings-active="settingsActive"
    :active-settings-nav="activeSettingsNav"
    @open-dashboard="router.push('/')"
    @add-workspace="addWorkspace"
    @open-settings="router.push('/settings/general')"
    @settings-nav-change="handleSettingsNavChange"
  >
    <template #sidebar>
      <div class="ios-sidebar-workspaces">
        <WorkspaceNavItem
          v-for="workspace in sidebarWorkspaces"
          :key="workspace!.id"
          :workspace="workspace!"
          :active="route.path === `/workspace/${workspace!.id}`"
          :mcp-state="mcpRuntimeStates[workspace!.id] ?? 'stopped'"
          @click="openWorkspace(workspace!.id)"
        />
      </div>
    </template>

    <RouterView v-slot="{ Component }">
      <component :is="Component" :key="route.fullPath" />
    </RouterView>
  </AppShell>

  <ToastHost />
  <CloseConfirmDialog v-model:open="closeConfirmOpen" />
</template>

