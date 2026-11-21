<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import BaseButton from "$src/components/ui/BaseButton.vue";
import AppShell from "$src/components/AppShell.vue";
import GlobalSearchDialog from "$src/components/GlobalSearchDialog.vue";
import CloseConfirmDialog from "$src/components/CloseConfirmDialog.vue";
import UnsavedChangesDialog from "./components/UnsavedChangesDialog.vue";
import { confirmUnsavedChanges } from "$lib/unsaved-changes";
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
import { enterCompactMode } from "$lib/api/window-chrome";
import {
  dashboardPreferences,
  loadDashboardPreferences,
  sortWorkspaceIds,
} from "$lib/dashboard-preferences";
import {
  buildGlobalSearchEntries,
  filterGlobalSearchEntries,
  type GlobalSearchEntry,
} from "$lib/global-search";
import { isMacPlatform } from "$lib/platform";

const router = useRouter();
const route = useRoute();
const closeConfirmOpen = ref(false);
const workspaceLoadError = ref("");
const workspaceLoadBusy = ref(false);
const workspaceFilter = ref("");
const globalSearchOpen = ref(false);
const globalSearchQuery = ref("");
const canCompact = isMacPlatform;

const sidebarWorkspaces = computed(() => {
  const orderedIds = sortWorkspaceIds(
    workspaces.value.map((workspace) => workspace.id),
    dashboardPreferences.value,
  );
  const byId = new Map(workspaces.value.map((workspace) => [workspace.id, workspace]));
  const items = orderedIds.map((id) => byId.get(id)).filter(Boolean);
  const q = workspaceFilter.value.trim().toLowerCase();
  if (!q) return items;
  return items.filter(
    (ws) => ws!.name.toLowerCase().includes(q) || ws!.path.toLowerCase().includes(q),
  );
});

const allSearchEntries = computed(() =>
  globalSearchOpen.value ? buildGlobalSearchEntries(workspaces.value, { canCompact }) : [],
);
const filteredSearchEntries = computed(() =>
  globalSearchOpen.value
    ? filterGlobalSearchEntries(allSearchEntries.value, globalSearchQuery.value)
    : [],
);

const settingsActive = computed(() => route.path.startsWith("/settings"));
const dashboardActive = computed(() => route.path === "/");
const auditActive = computed(() => route.path === "/audit");

async function refreshWorkspaces() {
  if (workspaceLoadBusy.value) return;
  workspaceLoadBusy.value = true;
  try {
    workspaces.value = await listWorkspaces();
    workspaceLoadError.value = "";
    try {
      globalMcpRuntimeState.value = (await getRuntimeStatus()).state;
    } catch {
      globalMcpRuntimeState.value = "unknown";
    }
  } catch (error) {
    workspaceLoadError.value = String(error);
    throw error;
  } finally {
    workspaceLoadBusy.value = false;
  }
}

async function compactUi() {
  if (!await confirmUnsavedChanges()) return;
  try {
    await enterCompactMode();
  } catch (error) {
    showToast(String(error), { title: "进入轻量模式失败", kind: "error" });
  }
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

function openGlobalSearch(initialQuery = "") {
  if (initialQuery) globalSearchQuery.value = initialQuery;
  globalSearchOpen.value = true;
}

function handleSelectSearchEntry(entry: GlobalSearchEntry) {
  globalSearchOpen.value = false;
  globalSearchQuery.value = "";
  if (entry.to) {
    void router.push(entry.to);
    return;
  }
  if (entry.action === "add-workspace") {
    void addWorkspace();
  } else if (entry.action === "toggle-theme") {
    window.dispatchEvent(new CustomEvent("coding-tools:toggle-theme"));
  } else if (entry.action === "compact-ui") {
    void compactUi();
  }
}

function handleGlobalKeydown(event: KeyboardEvent) {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    globalSearchOpen.value = !globalSearchOpen.value;
  }
}

let stopMemoryGuard: (() => void) | undefined;
let stopCloseGuard: (() => void) | undefined;
let stopCompactRequest: (() => void) | undefined;
let compactListenerDisposed = false;
const handleAddWorkspace = () => void addWorkspace();
const handleOpenSearch = (event: Event) => {
  const detail = (event as CustomEvent<{ query?: string }>).detail;
  openGlobalSearch(detail?.query ?? "");
};

onMounted(() => {
  compactListenerDisposed = false;
  void listen("compact-requested", () => {
    void compactUi();
  })
    .then((stop) => {
      if (compactListenerDisposed) stop();
      else stopCompactRequest = stop;
    })
    .catch((error) => {
      console.warn("[App] failed to subscribe compact-requested event:", error);
    });
  document.title = "Coding Tools MCP";
  loadDashboardPreferences();
  stopMemoryGuard = startUiMemoryGuard();
  stopCloseGuard = startCloseGuard(() => {
    void confirmUnsavedChanges().then((leave) => {
      if (leave) closeConfirmOpen.value = true;
    });
  });
  window.addEventListener("coding-tools:add-workspace", handleAddWorkspace);
  window.addEventListener("coding-tools:open-search", handleOpenSearch);
  window.addEventListener("keydown", handleGlobalKeydown);

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
  compactListenerDisposed = true;
  stopCompactRequest?.();
  stopMemoryGuard?.();
  stopCloseGuard?.();
  window.removeEventListener("coding-tools:add-workspace", handleAddWorkspace);
  window.removeEventListener("coding-tools:open-search", handleOpenSearch);
  window.removeEventListener("keydown", handleGlobalKeydown);
});
</script>

<template>
  <div class="relative min-h-screen w-full overflow-x-hidden">
    <AppShell
      v-model:workspace-filter="workspaceFilter"
      :dashboard-active="dashboardActive"
      :settings-active="settingsActive"
      :audit-active="auditActive"
      @open-dashboard="router.push('/')"
      @open-audit="router.push('/audit')"
      @open-search="openGlobalSearch()"
      @add-workspace="addWorkspace"
      @open-settings="router.push('/settings/connection')"
      @open-health="router.push({ path: '/settings/connection', query: { panel: 'health' } })"
      @compact-ui="compactUi"
    >
      <template #sidebar>
        <div>
          <WorkspaceNavItem
            v-for="workspace in sidebarWorkspaces"
            :key="workspace!.id"
            :workspace="workspace!"
            :active="route.params.id === workspace!.id"
            @click="openWorkspace(workspace!.id)"
          />
          <p
            v-if="workspaceFilter && !sidebarWorkspaces.length"
            class="px-3 py-3 text-center text-xs text-[var(--ui-text-muted)]"
          >
            无匹配工作区
          </p>
        </div>
      </template>

      <div class="flex h-full min-h-0 flex-col">
        <div v-if="workspaceLoadError" class="ui-banner ui-banner--warning m-4" role="alert">
          <span>工作区列表加载失败：{{ workspaceLoadError }}</span>
          <BaseButton variant="secondary" size="sm" :busy="workspaceLoadBusy" @click="refreshWorkspaces().catch(() => {})">
            重试
          </BaseButton>
        </div>
        <div class="min-h-0 flex-1">
          <RouterView v-slot="{ Component }">
            <component :key="route.name === 'workspace' ? String(route.params.id) : undefined" :is="Component" />
          </RouterView>
        </div>
      </div>
    </AppShell>

    <GlobalSearchDialog
      v-model:query="globalSearchQuery"
      :open="globalSearchOpen"
      :entries="filteredSearchEntries"
      @close="globalSearchOpen = false"
      @select="handleSelectSearchEntry"
    />
    <ToastHost />
    <UnsavedChangesDialog />
    <CloseConfirmDialog v-model:open="closeConfirmOpen" />
  </div>
</template>
