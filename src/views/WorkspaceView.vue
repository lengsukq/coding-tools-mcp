<script setup lang="ts">
import { computed, onBeforeUnmount, onErrorCaptured, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { RotateCw } from "@lucide/vue";
import BaseButton from "../components/ui/BaseButton.vue";
import ModalDialog from "../components/ui/ModalDialog.vue";
import SegmentedControl from "../components/ui/SegmentedControl.vue";
import WorkspaceDiagnostics from "../components/workspace/WorkspaceDiagnostics.vue";
import WorkspaceHeader from "../components/workspace/WorkspaceHeader.vue";
import WorkspacePlanning from "../components/workspace/WorkspacePlanning.vue";
import WorkspaceServices from "../components/workspace/WorkspaceServices.vue";
import WorkspaceSettings from "../components/workspace/WorkspaceSettings.vue";
import { setLastWorkspace } from "$lib/api/settings";
import { restartTunnel, stopTunnel } from "$lib/api/tunnel";
import {
  deleteWorkspace,
  openWorkspaceDirectory,
  restartRuntime,
  startRuntime,
  stopRuntime,
  updateWorkspace,
} from "$lib/api/workspaces";
import { notifyStartFailure, runServiceToggle } from "$lib/runtime/service";
import { promptServiceRestart } from "$lib/runtime/restart-hint";
import { mcpRuntimeStates, workspaces } from "$lib/stores/app";
import { showToast } from "$lib/stores/toast";
import {
  WORKSPACE_TABS,
  loadWorkspaceSnapshot,
  refreshWorkspaceSnapshot,
  tunnelConfigured,
  withAuth,
  withHistoryContext,
  withRuntimePolicy,
  withTunnelConfig,
  type RuntimePolicyDraft,
  type SaveTunnelOptions,
  type TunnelFormConfig,
  type WorkspaceTab,
} from "$lib/workspace-page";
import {
  mcpLocalEndpoint,
  type AuthConfig,
  type RuntimeState,
  type RuntimeStatus,
  type WorkspaceProfile,
} from "$lib/types";

const route = useRoute();
const router = useRouter();
const profile = ref<WorkspaceProfile | null>(null);
const mcpStatus = ref<RuntimeState>("stopped");
const mcpBusy = ref(false);
const mcpLocal = ref("");
const mcpPublic = ref("");
const activeTab = ref<WorkspaceTab>("services");
const pathCopied = ref(false);
const deleteConfirmOpen = ref(false);
const deleteBusy = ref(false);
const loadError = ref("");
const renderError = ref("");
let loadGeneration = 0;

const workspaceId = computed(() => String(route.params.id ?? ""));
const defaultLocalEndpoint = computed(() => profile.value ? mcpLocalEndpoint(profile.value.runtime.local_port) : "");

function applyRuntime(runtime: RuntimeStatus, id = workspaceId.value) {
  if (!id || id !== workspaceId.value) return;
  mcpStatus.value = runtime.state;
  mcpLocal.value = runtime.localEndpoint;
  mcpPublic.value = runtime.publicEndpoint;
  mcpRuntimeStates.value = { ...mcpRuntimeStates.value, [id]: runtime.state };
}

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
    if (snapshot.runtime) applyRuntime(snapshot.runtime, id);
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

async function afterStart(runtime: { state: RuntimeState; publicEndpoint: string }, id: string) {
  const nextProfile = await refreshProfile(id);
  if (id !== workspaceId.value) return;
  if (runtime.state === "running" && tunnelConfigured(nextProfile?.tunnel.type) && !runtime.publicEndpoint) {
    showToast("服务已启动，但公网地址尚未就绪。Cloudflare Quick Tunnel 可能需要短暂初始化。", {
      title: "隧道连接中",
      kind: "warning",
      duration: 6000,
    });
  }
}

async function toggleRuntime() {
  const id = workspaceId.value;
  if (!profile.value || !id || mcpBusy.value) return;
  const wasRunning = mcpStatus.value === "running";
  mcpBusy.value = true;
  try {
    const runtime = await runServiceToggle(
      wasRunning,
      () => startRuntime(id),
      () => stopRuntime(id),
      "MCP",
    );
    if (!runtime || id !== workspaceId.value) return;
    applyRuntime(runtime, id);
    if (!wasRunning) {
      if (runtime.state === "running") await afterStart(runtime, id);
      else notifyStartFailure("MCP", runtime);
    }
  } finally {
    if (id === workspaceId.value) mcpBusy.value = false;
  }
}

async function restartService() {
  if (!workspaceId.value || mcpBusy.value) return;
  mcpBusy.value = true;
  try {
    applyRuntime(await restartRuntime(workspaceId.value));
    showToast("MCP 服务已重启", { kind: "success" });
  } catch (error) {
    showToast(String(error), { title: "重启失败", kind: "error" });
  } finally {
    mcpBusy.value = false;
  }
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

async function saveTunnel(config: TunnelFormConfig, options?: SaveTunnelOptions) {
  if (!profile.value || !workspaceId.value) return;
  const next = withTunnelConfig(profile.value, config);
  await updateWorkspace(next);
  profile.value = next;
  if (mcpStatus.value === "running" && !options?.skipTunnelRestart) {
    try {
      if (config.type === "none") await stopTunnel(workspaceId.value);
      else await restartTunnel(workspaceId.value);
    } catch (error) {
      showToast(String(error), { title: "隧道重启失败", kind: "error", duration: 8000 });
    }
  }
  if (!options?.skipServicePrompt) {
    await promptServiceRestart(mcpStatus.value === "running", "MCP 服务");
  }
}

async function savePolicy(draft: RuntimePolicyDraft) {
  if (!profile.value) return;
  profile.value = withRuntimePolicy(profile.value, draft);
  await updateWorkspace(profile.value);
  await load();
  await promptServiceRestart(mcpStatus.value === "running", "MCP 服务");
}

async function saveHistory(recording: boolean, sessions: number[]) {
  if (!profile.value) return;
  profile.value = withHistoryContext(profile.value, recording, sessions);
  await updateWorkspace(profile.value);
  await load();
  await promptServiceRestart(mcpStatus.value === "running", "MCP 服务");
}

async function saveAuth(auth: AuthConfig) {
  if (!profile.value || !workspaceId.value) return;
  profile.value = withAuth(profile.value, auth);
  await updateWorkspace(profile.value);
  if (mcpStatus.value === "running") {
    try { applyRuntime(await restartRuntime(workspaceId.value)); }
    catch (error) { showToast(String(error), { title: "服务重启失败", kind: "error", duration: 8000 }); }
  }
}

async function saveName(name: string) {
  if (!profile.value || !name || profile.value.name === name) return;
  const next = { ...profile.value, name };
  await updateWorkspace(next);
  profile.value = next;
  workspaces.value = workspaces.value.map((item) => item.id === next.id ? { ...item, name } : item);
}

async function savePath(path: string) {
  if (!profile.value || !path || profile.value.path === path) return;
  profile.value = { ...profile.value, path };
  await updateWorkspace(profile.value);
  showToast("工作区目录已更新", { kind: "success" });
  await promptServiceRestart(mcpStatus.value === "running", "MCP 服务");
}

async function confirmDelete() {
  if (!profile.value || !workspaceId.value || deleteBusy.value) return;
  deleteBusy.value = true;
  const id = workspaceId.value;
  try {
    await deleteWorkspace(id);
    workspaces.value = workspaces.value.filter((item) => item.id !== id);
    const next = { ...mcpRuntimeStates.value };
    delete next[id];
    mcpRuntimeStates.value = next;
    deleteConfirmOpen.value = false;
    await router.replace("/");
  } catch (error) {
    showToast(String(error), { title: "删除工作区失败", kind: "error" });
  } finally { deleteBusy.value = false; }
}

watch(workspaceId, (id) => {
  profile.value = null;
  activeTab.value = "services";
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
      :runtime-state="mcpStatus"
      :runtime-busy="mcpBusy"
      :path-copied="pathCopied"
      @reveal-directory="revealDirectory"
      @copy-path="copyPath"
      @toggle-runtime="toggleRuntime"
    />
    <div class="sticky top-0 z-20 mt-3 border-y border-white/35 bg-white/38 px-7 py-2.5 backdrop-blur-2xl dark:border-white/6 dark:bg-black/15 sm:px-8">
      <SegmentedControl :items="WORKSPACE_TABS" :model-value="activeTab" @update:model-value="activeTab = $event as WorkspaceTab" />
    </div>
    <div class="mx-auto max-w-[1380px] px-7 pt-6 sm:px-8">
      <WorkspaceServices
          v-if="activeTab === 'services'"
          :workspace-id="workspaceId"
          :profile="profile"
          :state="mcpStatus"
          :busy="mcpBusy"
          :local-endpoint="mcpLocal"
          :public-endpoint="mcpPublic"
          :default-local-endpoint="defaultLocalEndpoint"
          @toggle="toggleRuntime"
          @restart="restartService"
          @save-tunnel="saveTunnel"
          @save-auth="saveAuth"
          @save-policy="savePolicy"
          @save-history="saveHistory"
        />
      <WorkspaceDiagnostics v-else-if="activeTab === 'diagnostics'" :workspace-id="workspaceId" />
      <WorkspacePlanning v-else-if="activeTab === 'planning'" :workspace-id="workspaceId" />
      <WorkspaceSettings v-else :profile="profile" @save-name="saveName" @update-path="savePath" @delete="deleteConfirmOpen = true" />
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
