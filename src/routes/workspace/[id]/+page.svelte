<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { RotateCw } from "@lucide/svelte";
  import PlanningControlPanel from "$lib/components/PlanningControlPanel.svelte";
  import type { RuntimePolicyDraft } from "$lib/components/RuntimePolicyForm.svelte";
  import type { SaveTunnelOptions, TunnelFormConfig } from "$lib/components/TunnelConfigForm.svelte";
  import ConfirmDialog from "$lib/components/ui/ConfirmDialog.svelte";
  import SegmentedControl from "$lib/components/ui/SegmentedControl.svelte";
  import WorkspaceDiagnosticsPanel from "$lib/components/workspace/WorkspaceDiagnosticsPanel.svelte";
  import WorkspaceHeader from "$lib/components/workspace/WorkspaceHeader.svelte";
  import WorkspaceServiceCockpit from "$lib/components/workspace/WorkspaceServiceCockpit.svelte";
  import WorkspaceSettingsPanel from "$lib/components/workspace/WorkspaceSettingsPanel.svelte";
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
    type WorkspaceTab,
  } from "$lib/workspace-page";
  import {
    mcpLocalEndpoint,
    type AuthConfig,
    type RuntimeState,
    type RuntimeStatus,
    type WorkspaceProfile,
  } from "$lib/types";

  let profile = $state<WorkspaceProfile | null>(null);
  let mcpStatus = $state<RuntimeState>("stopped");
  let mcpBusy = $state(false);
  let mcpLocal = $state("");
  let mcpPublic = $state("");
  let activeWorkspaceTab = $state<WorkspaceTab>("services");
  let pathCopied = $state(false);
  let endpointCopied = $state<string | null>(null);
  let deleteConfirmOpen = $state(false);
  let deleteBusy = $state(false);
  let loadGeneration = 0;

  const workspaceTabs = WORKSPACE_TABS;
  const workspaceId = $derived($page.params.id);
  const defaultMcpLocal = $derived(profile ? mcpLocalEndpoint(profile.runtime.local_port) : "");

  function applyMcpRuntime(runtime: RuntimeStatus, id = workspaceId) {
    if (!id || id !== workspaceId) return;
    mcpStatus = runtime.state;
    mcpLocal = runtime.localEndpoint;
    mcpPublic = runtime.publicEndpoint;
    mcpRuntimeStates.update((current) => ({ ...current, [id]: runtime.state }));
  }

  async function load(id = workspaceId) {
    if (!id) return;
    const generation = ++loadGeneration;
    const snapshot = await loadWorkspaceSnapshot(id);
    if (generation !== loadGeneration || id !== workspaceId) return;

    workspaces.set(snapshot.items);
    profile = snapshot.profile;
    if (profile) await setLastWorkspace(profile.id);
    if (generation !== loadGeneration || id !== workspaceId) return;

    if (!profile) {
      await goto("/");
      return;
    }
    if (snapshot.runtime) applyMcpRuntime(snapshot.runtime, id);
  }

  async function refreshProfile(id = workspaceId): Promise<WorkspaceProfile | null> {
    if (!id) return null;
    const snapshot = await refreshWorkspaceSnapshot(id);
    if (id !== workspaceId) return null;
    workspaces.set(snapshot.items);
    profile = snapshot.profile;
    return profile;
  }

  async function afterServiceStart(
    runtime: { state: RuntimeState; publicEndpoint: string },
    id: string,
  ) {
    const nextProfile = await refreshProfile(id);
    if (id !== workspaceId) return;
    const needsTunnel = tunnelConfigured(nextProfile?.tunnel.type);
    if (runtime.state === "running" && needsTunnel && !runtime.publicEndpoint) {
      showToast(
        "服务已启动，但公网地址尚未就绪。如使用 Cloudflare Quick Tunnel，请稍候；若未自动重连，可在设置中重新连接隧道。",
        { title: "隧道连接中", kind: "warning", duration: 6000 },
      );
    }
  }

  async function toggleMcp() {
    const id = workspaceId;
    if (!profile || !id || mcpBusy) return;
    const wasRunning = mcpStatus === "running";
    mcpBusy = true;
    try {
      const runtime = await runServiceToggle(
        wasRunning,
        () => startRuntime(id),
        () => stopRuntime(id),
        "MCP",
      );
      if (!runtime || id !== workspaceId) return;
      applyMcpRuntime(runtime, id);
      if (!wasRunning) {
        if (runtime.state === "running") await afterServiceStart(runtime, id);
        else notifyStartFailure("MCP", runtime);
      }
    } finally {
      if (id === workspaceId) mcpBusy = false;
    }
  }

  async function handleRestartService() {
    if (!workspaceId || mcpBusy) return;
    mcpBusy = true;
    try {
      applyMcpRuntime(await restartRuntime(workspaceId));
      showToast("MCP 服务已重启", { kind: "success" });
    } catch (error) {
      showToast(String(error), { title: "重启失败", kind: "error" });
    } finally {
      mcpBusy = false;
    }
  }

  async function handleRevealDirectory() {
    if (!profile?.path) return;
    try {
      await openWorkspaceDirectory(profile.path);
    } catch (error) {
      showToast(String(error), { title: "打开目录失败", kind: "error" });
    }
  }

  function copyPath() {
    if (!profile?.path) return;
    void navigator.clipboard.writeText(profile.path);
    pathCopied = true;
    setTimeout(() => { pathCopied = false; }, 1800);
    showToast("路径已复制到剪贴板", { kind: "info" });
  }

  function copyEndpoint(url: string, id: string) {
    if (!url) return;
    void navigator.clipboard.writeText(url);
    endpointCopied = id;
    setTimeout(() => { endpointCopied = null; }, 1800);
    showToast("地址已复制到剪贴板", { kind: "info" });
  }

  async function saveMcpTunnel(config: TunnelFormConfig, options?: SaveTunnelOptions) {
    if (!profile || !workspaceId) return;
    const next = withTunnelConfig(profile, config);
    await updateWorkspace(next);
    profile = next;
    if (mcpStatus === "running" && !options?.skipTunnelRestart) {
      try {
        if (config.type === "none") await stopTunnel(workspaceId);
        else await restartTunnel(workspaceId);
      } catch (error) {
        showToast(String(error), { title: "隧道重启失败", kind: "error", duration: 8000 });
      }
    }
    if (!options?.skipServicePrompt) {
      await promptServiceRestart(mcpStatus === "running", "MCP 服务");
    }
  }

  async function saveMcpPolicy(draft: RuntimePolicyDraft) {
    if (!profile) return;
    profile = withRuntimePolicy(profile, draft);
    await updateWorkspace(profile);
    await load();
    await promptServiceRestart(mcpStatus === "running", "MCP 服务");
  }

  async function saveHistoryContext(recording: boolean, selectedSessions: number[]) {
    if (!profile) return;
    profile = withHistoryContext(profile, recording, selectedSessions);
    await updateWorkspace(profile);
    await load();
    await promptServiceRestart(mcpStatus === "running", "MCP 服务");
  }

  async function saveMcpAuth(auth: AuthConfig, options?: { skipRuntimeRestart?: boolean }) {
    if (!profile || !workspaceId) return;
    profile = withAuth(profile, auth);
    await updateWorkspace(profile);
    if (!options?.skipRuntimeRestart && mcpStatus === "running") {
      try {
        applyMcpRuntime(await restartRuntime(workspaceId));
      } catch (error) {
        showToast(String(error), { title: "服务重启失败", kind: "error", duration: 8000 });
      }
    }
  }

  async function saveWorkspaceName(name: string) {
    if (!profile || profile.name === name) return;
    const next: WorkspaceProfile = { ...profile, name };
    await updateWorkspace(next);
    profile = next;
    workspaces.update((items) =>
      items.map((item) => (item.id === next.id ? { ...item, name: next.name } : item)),
    );
  }

  async function saveWorkspacePath(path: string) {
    if (!profile || profile.path === path) return;
    profile = { ...profile, path };
    await updateWorkspace(profile);
    showToast("工作区目录已更新", { kind: "success" });
    await promptServiceRestart(mcpStatus === "running", "MCP 服务");
  }

  function requestRemoveWorkspace() {
    deleteConfirmOpen = true;
  }

  async function handleConfirmDelete() {
    if (!profile || !workspaceId || deleteBusy) return;
    deleteBusy = true;
    try {
      await deleteWorkspace(workspaceId);
      workspaces.update((items) => items.filter((item) => item.id !== workspaceId));
      mcpRuntimeStates.update((states) => {
        const next = { ...states };
        delete next[workspaceId];
        return next;
      });
      deleteConfirmOpen = false;
      await goto("/");
    } catch (error) {
      showToast(String(error), { title: "删除工作区失败", kind: "error" });
    } finally {
      deleteBusy = false;
    }
  }

  $effect(() => {
    const id = workspaceId;
    if (!id) return;
    profile = null;
    void load(id);
    return () => { loadGeneration += 1; };
  });
</script>

{#if !profile}
  <div class="flex h-full min-h-96 items-center justify-center">
    <div class="flex flex-col items-center gap-3">
      <RotateCw size={24} class="animate-spin text-[var(--primary)]" />
      <p class="text-xs text-[var(--color-text-muted)]">正在加载工作区数据…</p>
    </div>
  </div>
{:else}
  <section class="page-scroll wb-workspace-page">
    <WorkspaceHeader
      {profile}
      runtimeState={mcpStatus}
      runtimeBusy={mcpBusy}
      {pathCopied}
      onRevealDirectory={handleRevealDirectory}
      onCopyPath={copyPath}
      onToggleRuntime={toggleMcp}
    />

    <div class="wb-workspace-tabs sticky top-0 z-10 px-7 pt-3 pb-2 sm:px-8">
      <div class="max-w-xl">
        <SegmentedControl
          items={workspaceTabs}
          value={activeWorkspaceTab}
          size="md"
          onchange={(value) => { activeWorkspaceTab = value as WorkspaceTab; }}
        />
      </div>
    </div>

    <div class="page-body pt-7 pb-14 sm:pt-8">
      {#key activeWorkspaceTab}
        <div class="tx-tab-content-wrapper">
          {#if activeWorkspaceTab === "services"}
            <WorkspaceServiceCockpit
              workspaceId={workspaceId!}
              {profile}
              runtime={{
                state: mcpStatus,
                busy: mcpBusy,
                localEndpoint: mcpLocal,
                publicEndpoint: mcpPublic,
                defaultLocalEndpoint: defaultMcpLocal,
                endpointCopied,
              }}
              actions={{
                toggle: toggleMcp,
                restart: handleRestartService,
                copyEndpoint,
                saveTunnel: saveMcpTunnel,
                saveAuth: saveMcpAuth,
                savePolicy: saveMcpPolicy,
                saveHistory: saveHistoryContext,
              }}
            />
          {:else if activeWorkspaceTab === "diagnostics"}
            <WorkspaceDiagnosticsPanel workspaceId={workspaceId!} />
          {:else if activeWorkspaceTab === "planning"}
            <div class="grid gap-4">
              <PlanningControlPanel workspaceId={workspaceId!} />
            </div>
          {:else}
            <WorkspaceSettingsPanel
              {profile}
              onSaveName={saveWorkspaceName}
              onUpdatePath={saveWorkspacePath}
              onDelete={requestRemoveWorkspace}
            />
          {/if}
        </div>
      {/key}
    </div>
  </section>

  <ConfirmDialog
    open={deleteConfirmOpen}
    title="删除工作区"
    message={`确定删除工作区「${profile.name}」？此操作仅移除应用内的配置与路由，本地磁盘中的源码文件和项目目录将完好无损。`}
    detail={profile.path}
    confirmText="确认删除"
    cancelText="取消"
    severity="danger"
    busy={deleteBusy}
    onConfirm={handleConfirmDelete}
    onCancel={() => { deleteConfirmOpen = false; }}
  />
{/if}
