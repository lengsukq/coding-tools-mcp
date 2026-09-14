<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import {
    Folder,
    FolderOpen,
    Copy,
    Check,
    Play,
    Square,
    RotateCw,
    Settings,
    Radio,
    Activity,
    FileText,
    Sliders,
    Shield,
    ListChecks,
    ExternalLink,
    ChevronRight,
    Terminal,
    Layers,
    Cpu,
    RefreshCw,
    Trash2,
    HardDrive,
  } from "@lucide/svelte";
  import ActionsAuthForm from "$lib/components/ActionsAuthForm.svelte";
  import ActionsPolicyForm, {
    type ActionsPolicyDraft,
  } from "$lib/components/ActionsPolicyForm.svelte";
  import AuthConfigForm from "$lib/components/AuthConfigForm.svelte";
  import HealthPanel from "$lib/components/HealthPanel.svelte";
  import HistoryContextPanel from "$lib/components/HistoryContextPanel.svelte";
  import LogViewer from "$lib/components/LogViewer.svelte";
  import RuntimePolicyForm, {
    type RuntimePolicyDraft,
  } from "$lib/components/RuntimePolicyForm.svelte";
  import ChatGptSessionPrompt from "$lib/components/ChatGptSessionPrompt.svelte";
  import PlanningControlPanel from "$lib/components/PlanningControlPanel.svelte";
  import GptQuickCopy from "$lib/components/GptQuickCopy.svelte";
  import StatusOrb from "$lib/components/StatusOrb.svelte";
  import SegmentedControl from "$lib/components/ui/SegmentedControl.svelte";
  import ConfirmDialog from "$lib/components/ui/ConfirmDialog.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import StatusBadge from "$lib/components/ui/StatusBadge.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import TunnelConfigForm, {
    type TunnelFormConfig,
    type SaveTunnelOptions,
  } from "$lib/components/TunnelConfigForm.svelte";
  import WorkspaceMetaForm from "$lib/components/WorkspaceMetaForm.svelte";
  import {
    deleteWorkspace,
    getActionsRuntimeStatus,
    getRuntimeStatus,
    listWorkspaces,
    startActionsRuntime,
    startRuntime,
    restartRuntime,
    restartActionsRuntime,
    stopActionsRuntime,
    stopRuntime,
    updateWorkspace,
    openWorkspaceDirectory,
  } from "$lib/api/workspaces";
  import { listFrpProfiles, setLastWorkspace, type FrpProfileDto } from "$lib/api/settings";
  import { restartTunnel, stopTunnel } from "$lib/api/tunnel";
  import { runServiceToggle, notifyStartFailure } from "$lib/runtime/service";
  import { showToast } from "$lib/stores/toast";
  import { promptServiceRestart } from "$lib/runtime/restart-hint";
  import { actionsRuntimeStates, mcpRuntimeStates, workspaces } from "$lib/stores/app";
  import {
    actionsConfig,
    actionsLocalEndpoint,
    actionsOAuthAuthorizeUrl,
    actionsOAuthTokenUrl,
    actionsOpenApiUrl,
    actionsPrivacyUrl,
    frpPublicUrl,
    mcpLocalEndpoint,
    type AuthConfig,
    type ActionsAuthDraft,
    type RuntimeState,
    type RuntimeStatus,
    type WorkspaceProfile,
  } from "$lib/types";

  type WorkspaceTab = "services" | "diagnostics" | "planning" | "settings";
  type ServiceKind = "mcp" | "actions" | "both";
  type McpConfigSection = "connection" | "auth" | "policy" | "history";
  type ActionsConfigSection = "connection" | "auth" | "policy";

  let profile = $state<WorkspaceProfile | null>(null);
  let mcpStatus = $state<RuntimeState>("stopped");
  let actionsStatus = $state<RuntimeState>("stopped");
  let mcpStatusMessage = $state("");
  let actionsStatusMessage = $state("");
  let mcpBusy = $state(false);
  let actionsBusy = $state(false);
  let mcpLocal = $state("");
  let mcpPublic = $state("");
  let actionsLocal = $state("");
  let actionsPublic = $state("");
  let frpProfiles = $state<FrpProfileDto[]>([]);

  let activeWorkspaceTab = $state<WorkspaceTab>("services");
  let serviceViewMode = $state<ServiceKind>("mcp");
  let mcpConfigSection = $state<McpConfigSection>("connection");
  let actionsConfigSection = $state<ActionsConfigSection>("connection");
  let diagnosticLogService = $state<"mcp" | "actions">("mcp");
  let pathCopied = $state(false);
  let endpointCopied = $state<string | null>(null);
  let loadGeneration = 0;

  const workspaceTabs = [
    { value: "services", label: "服务与端点" },
    { value: "diagnostics", label: "诊断与日志" },
    { value: "planning", label: "任务规划" },
    { value: "settings", label: "工作区设置" },
  ];

  const serviceViewTabs = [
    { value: "mcp", label: "MCP 运行时" },
    { value: "actions", label: "Actions 网关" },
    { value: "both", label: "双服务全景" },
  ];

  const mcpConfigTabs = [
    { value: "connection", label: "连接与隧道" },
    { value: "auth", label: "访问认证" },
    { value: "policy", label: "执行权限" },
    { value: "history", label: "历史上下文" },
  ];

  const actionsConfigTabs = [
    { value: "connection", label: "连接与隧道" },
    { value: "auth", label: "访问认证" },
    { value: "policy", label: "执行权限" },
  ];

  const diagnosticLogTabs = [
    { value: "mcp", label: "MCP 日志" },
    { value: "actions", label: "Actions 日志" },
  ];

  const workspaceId = $derived($page.params.id);
  const actions = $derived(profile ? actionsConfig(profile) : null);

  const mcpTunnelForm = $derived<TunnelFormConfig>({
    type: profile?.tunnel.type ?? "none",
    public_url: profile?.tunnel.public_url ?? "",
    frp_server: profile?.tunnel.frp_server ?? "",
    frp_subdomain: profile?.tunnel.frp_subdomain ?? "",
    frp_profile_id: profile?.tunnel.frp_profile_id ?? "",
    frp_server_port: profile?.tunnel.frp_server_port ?? 7000,
    cloudflare_mode: profile?.tunnel.cloudflare_mode ?? "quick",
    use_proxy: profile?.tunnel.use_proxy ?? true,
    use_global_gateway: profile?.tunnel.use_global_gateway ?? false,
  });

  const actionsTunnelForm = $derived<TunnelFormConfig>({
    type: actions?.tunnel_type ?? "none",
    public_url: actions?.public_url ?? "",
    frp_server: actions?.frp_server ?? "",
    frp_subdomain: actions?.frp_subdomain ?? "",
    frp_profile_id: actions?.frp_profile_id ?? "",
    frp_server_port: actions?.frp_server_port ?? 7000,
    cloudflare_mode: actions?.cloudflare_mode ?? "quick",
    use_proxy: actions?.use_proxy ?? true,
    use_global_gateway: actions?.use_global_gateway ?? false,
  });

  const actionsOpenApi = $derived(profile ? actionsOpenApiUrl(profile, frpProfiles) : "");
  const actionsPrivacy = $derived(profile ? actionsPrivacyUrl(profile, frpProfiles) : "");
  const actionsOAuthAuth = $derived(profile ? actionsOAuthAuthorizeUrl(profile, frpProfiles) : "");
  const actionsOAuthToken = $derived(profile ? actionsOAuthTokenUrl(profile, frpProfiles) : "");
  const defaultMcpLocal = $derived(profile ? mcpLocalEndpoint(profile.runtime.local_port) : "");
  const defaultActionsLocal = $derived(actions ? actionsLocalEndpoint(actions.local_port) : "");

  function stateLabel(state: RuntimeState): string {
    switch (state) {
      case "running":
        return "运行中";
      case "starting":
        return "启动中";
      case "stopping":
        return "停止中";
      case "error":
        return "异常";
      default:
        return "已停止";
    }
  }

  function applyMcpRuntime(
    runtime: RuntimeStatus,
    id = workspaceId,
  ) {
    if (!id || id !== workspaceId) return;
    mcpStatus = runtime.state;
    mcpStatusMessage = runtime.localMessage ?? "";
    mcpLocal = runtime.localEndpoint;
    mcpPublic = runtime.publicEndpoint;
    mcpRuntimeStates.update((current) => ({ ...current, [id]: runtime.state }));
  }

  function applyActionsRuntime(
    runtime: RuntimeStatus,
    id = workspaceId,
  ) {
    if (!id || id !== workspaceId) return;
    actionsStatus = runtime.state;
    actionsStatusMessage = runtime.localMessage ?? "";
    actionsLocal = runtime.localEndpoint;
    actionsPublic = runtime.publicEndpoint;
    actionsRuntimeStates.update((current) => ({ ...current, [id]: runtime.state }));
  }

  async function load(id = workspaceId) {
    if (!id) return;
    const generation = ++loadGeneration;
    const items = await listWorkspaces();
    if (generation !== loadGeneration || id !== workspaceId) return;
    workspaces.set(items);
    frpProfiles = await listFrpProfiles();
    if (generation !== loadGeneration || id !== workspaceId) return;
    const nextProfile = items.find((item) => item.id === id) ?? null;
    if (generation !== loadGeneration || id !== workspaceId) return;
    profile = nextProfile;
    if (nextProfile) {
      await setLastWorkspace(nextProfile.id);
    }
    if (generation !== loadGeneration || id !== workspaceId) return;
    if (!nextProfile) {
      await goto("/");
      return;
    }

    const [mcpRuntime, actionsRuntime] = await Promise.all([
      getRuntimeStatus(id),
      getActionsRuntimeStatus(id),
    ]);
    if (generation !== loadGeneration || id !== workspaceId) return;
    applyMcpRuntime(mcpRuntime, id);
    applyActionsRuntime(actionsRuntime, id);
  }

  async function refreshProfile(id = workspaceId): Promise<WorkspaceProfile | null> {
    if (!id) return null;
    const items = await listWorkspaces();
    if (id !== workspaceId) return null;
    workspaces.set(items);
    const nextProfile = items.find((item) => item.id === id) ?? null;
    profile = nextProfile;
    return nextProfile;
  }

  function tunnelConfigured(type: string | undefined): boolean {
    return type === "cloudflare" || type === "frp";
  }

  async function afterServiceStart(
    service: "mcp" | "actions",
    runtime: { state: RuntimeState; publicEndpoint: string },
    id: string,
  ) {
    const nextProfile = await refreshProfile(id);
    if (id !== workspaceId) return;
    const tunnelType =
      service === "mcp"
        ? nextProfile?.tunnel.type
        : nextProfile
          ? actionsConfig(nextProfile).tunnel_type
          : undefined;
    const needsTunnel = tunnelConfigured(tunnelType);
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
      if (runtime && id === workspaceId) {
        applyMcpRuntime(runtime, id);
        if (!wasRunning) {
          if (runtime.state === "running") {
            await afterServiceStart("mcp", runtime, id);
          } else {
            notifyStartFailure("MCP", runtime);
          }
        }
      }
    } finally {
      if (id === workspaceId) {
        mcpBusy = false;
      }
    }
  }

  async function toggleActions() {
    const id = workspaceId;
    if (!profile || !id || actionsBusy) return;
    const wasRunning = actionsStatus === "running";
    actionsBusy = true;
    try {
      const runtime = await runServiceToggle(
        wasRunning,
        () => startActionsRuntime(id),
        () => stopActionsRuntime(id),
        "Actions",
      );
      if (runtime && id === workspaceId) {
        applyActionsRuntime(runtime, id);
        if (!wasRunning) {
          if (runtime.state === "running") {
            await afterServiceStart("actions", runtime, id);
          } else {
            notifyStartFailure("Actions", runtime);
          }
        }
      }
    } finally {
      if (id === workspaceId) {
        actionsBusy = false;
      }
    }
  }

  async function handleRestartService(service: "mcp" | "actions") {
    if (!workspaceId) return;
    if (service === "mcp") {
      mcpBusy = true;
      try {
        const res = await restartRuntime(workspaceId);
        applyMcpRuntime(res);
        showToast("MCP 服务已重启", { kind: "success" });
      } catch (err) {
        showToast(String(err), { title: "重启失败", kind: "error" });
      } finally {
        mcpBusy = false;
      }
    } else {
      actionsBusy = true;
      try {
        const res = await restartActionsRuntime(workspaceId);
        applyActionsRuntime(res);
        showToast("Actions 服务已重启", { kind: "success" });
      } catch (err) {
        showToast(String(err), { title: "重启失败", kind: "error" });
      } finally {
        actionsBusy = false;
      }
    }
  }

  async function handleRevealDirectory() {
    if (!profile?.path) return;
    try {
      await openWorkspaceDirectory(profile.path);
    } catch (err) {
      showToast(String(err), { title: "打开目录失败", kind: "error" });
    }
  }

  function copyPath() {
    if (!profile?.path) return;
    navigator.clipboard.writeText(profile.path);
    pathCopied = true;
    setTimeout(() => { pathCopied = false; }, 1800);
    showToast("路径已复制到剪贴板", { kind: "info" });
  }

  function copyEndpoint(url: string, id: string) {
    if (!url) return;
    navigator.clipboard.writeText(url);
    endpointCopied = id;
    setTimeout(() => { endpointCopied = null; }, 1800);
    showToast("地址已复制到剪贴板", { kind: "info" });
  }

  async function saveMcpPort(port: number) {
    if (!profile) return;
    const next: WorkspaceProfile = {
      ...profile,
      runtime: { ...profile.runtime, local_port: port },
    };
    await updateWorkspace(next);
    profile = next;
    await promptServiceRestart(mcpStatus === "running", "MCP 服务");
  }

  async function saveActionsPort(port: number) {
    if (!profile) return;
    const current = actionsConfig(profile);
    const next: WorkspaceProfile = {
      ...profile,
      actions: { ...current, local_port: port },
    };
    await updateWorkspace(next);
    profile = next;
    await promptServiceRestart(actionsStatus === "running", "Actions 服务");
  }

  async function saveMcpTunnel(config: TunnelFormConfig, options?: SaveTunnelOptions) {
    if (!profile || !workspaceId) return;
    const next: WorkspaceProfile = {
      ...profile,
      tunnel: {
        type: config.type,
        public_url: config.public_url,
        frp_server: config.frp_server,
        frp_subdomain: config.frp_subdomain,
        frp_profile_id: config.frp_profile_id,
        frp_server_port: config.frp_server_port,
        cloudflare_mode: config.cloudflare_mode,
        use_proxy: config.use_proxy,
        use_global_gateway: config.use_global_gateway,
      },
    };
    await updateWorkspace(next);
    profile = next;
    if (mcpStatus === "running" && !options?.skipTunnelRestart) {
      try {
        if (config.type === "none") {
          await stopTunnel(workspaceId, "mcp");
        } else {
          await restartTunnel(workspaceId, "mcp");
        }
      } catch (error) {
        showToast(String(error), { title: "隧道重启失败", kind: "error", duration: 8000 });
      }
    }
    if (!options?.skipServicePrompt) {
      await promptServiceRestart(mcpStatus === "running", "MCP 服务");
    }
  }

  async function saveActionsTunnel(config: TunnelFormConfig, options?: SaveTunnelOptions) {
    if (!profile || !workspaceId) return;
    const current = actionsConfig(profile);
    const next: WorkspaceProfile = {
      ...profile,
      actions: {
        ...current,
        tunnel_type: config.type,
        public_url: config.public_url,
        frp_server: config.frp_server,
        frp_subdomain: config.frp_subdomain,
        frp_profile_id: config.frp_profile_id,
        frp_server_port: config.frp_server_port,
        cloudflare_mode: config.cloudflare_mode,
        use_proxy: config.use_proxy,
        use_global_gateway: config.use_global_gateway,
      },
    };
    await updateWorkspace(next);
    profile = next;
    if (actionsStatus === "running" && !options?.skipTunnelRestart) {
      try {
        if (config.type === "none") {
          await stopTunnel(workspaceId, "actions");
        } else {
          await restartTunnel(workspaceId, "actions");
        }
      } catch (error) {
        showToast(String(error), { title: "隧道重启失败", kind: "error", duration: 8000 });
      }
    }
    if (!options?.skipServicePrompt) {
      await promptServiceRestart(actionsStatus === "running", "Actions 服务");
    }
  }

  async function saveMcpPolicy(draft: RuntimePolicyDraft) {
    if (!profile) return;
    const next: WorkspaceProfile = {
      ...profile,
      runtime: {
        ...profile.runtime,
        tool_profile: draft.toolProfile,
        permission_mode: draft.permissionMode,
        allowed_commands: draft.allowedCommands,
        executable_paths: draft.executablePaths,
        ai_instructions: draft.aiInstructions,
        instruction_sources: draft.instructionSources,
        skill_sources: draft.skillSources,
        custom_instruction_paths: draft.customInstructionPaths,
        custom_skill_paths: draft.customSkillPaths,
        workspace_local_entries: draft.workspaceLocalEntries,
        workspace_script_extensions: draft.workspaceScriptExtensions,
      },
    };
    await updateWorkspace(next);
    profile = next;
    await load();
    await promptServiceRestart(mcpStatus === "running", "MCP 服务");
  }

  async function saveHistoryContext(recording: boolean, selectedSessions: number[]) {
    if (!profile) return;
    const next: WorkspaceProfile = {
      ...profile,
      runtime: {
        ...profile.runtime,
        history_recording: recording,
        history_context_sessions: selectedSessions,
      },
    };
    await updateWorkspace(next);
    profile = next;
    await load();
    await promptServiceRestart(mcpStatus === "running", "MCP 服务");
  }

  async function saveActionsPolicy(draft: ActionsPolicyDraft) {
    if (!profile) return;
    const current = actionsConfig(profile);
    const next: WorkspaceProfile = {
      ...profile,
      actions: {
        ...current,
        allowed_commands: draft.allowedCommands,
        max_patch_bytes: draft.maxPatchBytes,
        permission_mode: draft.permissionMode,
      },
    };
    await updateWorkspace(next);
    profile = next;
    await load();
    await promptServiceRestart(actionsStatus === "running", "Actions 服务");
  }

  async function saveMcpAuth(auth: AuthConfig, options?: { skipRuntimeRestart?: boolean }) {
    if (!profile || !workspaceId) return;
    const next: WorkspaceProfile = { ...profile, auth };
    await updateWorkspace(next);
    profile = next;
    if (!options?.skipRuntimeRestart && mcpStatus === "running") {
      try {
        await restartRuntime(workspaceId);
      } catch (error) {
        showToast(String(error), { title: "服务重启失败", kind: "error", duration: 8000 });
      }
    }
  }

  async function saveActionsAuth(draft: ActionsAuthDraft) {
    if (!profile || !workspaceId) return;
    const current = actionsConfig(profile);
    const next: WorkspaceProfile = {
      ...profile,
      actions: {
        ...current,
        auth_type: draft.authType,
        oauth_client_id: draft.oauthClientId || current.oauth_client_id,
        oauth_scopes: draft.oauthScopes,
        use_shared_secrets: draft.useSharedSecrets,
      },
    };
    await updateWorkspace(next);
    profile = next;
    if (actionsStatus === "running") {
      try {
        await restartActionsRuntime(workspaceId);
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
    const next: WorkspaceProfile = { ...profile, path };
    await updateWorkspace(next);
    profile = next;
    showToast("工作区目录已更新", { kind: "success" });
    await promptServiceRestart(mcpStatus === "running", "MCP 服务");
    await promptServiceRestart(actionsStatus === "running", "Actions 服务");
  }

  let deleteConfirmOpen = $state(false);
  let deleteBusy = $state(false);

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
      actionsRuntimeStates.update((states) => {
        const next = { ...states };
        delete next[workspaceId];
        return next;
      });
      deleteConfirmOpen = false;
      goto("/");
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

    return () => {
      loadGeneration++;
    };
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
  <section class="page-scroll">
    <!-- Desktop Native Header Toolbar -->
    <header class="page-header border-b border-[var(--border)] bg-[var(--glass-bg)] pb-4 backdrop-blur-xl">
      <div class="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
        <!-- Workspace identity & Path -->
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2.5">
            <div class="flex size-9 items-center justify-center rounded-xl bg-[var(--primary-soft)] text-[var(--primary)] border border-[var(--primary)]/20 shadow-sm">
              <Folder size={18} strokeWidth={2.2} />
            </div>
            <div class="min-w-0">
              <h2 class="text-lg font-bold tracking-tight text-[var(--text-main)] truncate leading-tight">
                {profile.name}
              </h2>
            </div>
          </div>

          <!-- Path pill & Native folder trigger -->
          <div class="mt-2 flex flex-wrap items-center gap-2">
            <div class="inline-flex items-center gap-1.5 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-2.5 py-1 text-xs text-[var(--text-secondary)] shadow-sm max-w-full">
              <HardDrive size={12} class="text-[var(--text-muted)] shrink-0" />
              <span class="truncate font-mono text-[11px] select-all">{profile.path}</span>
            </div>

            <Button
              variant="ghost"
              size="sm"
              title="在系统访达/资源管理器中打开"
              onclick={handleRevealDirectory}
            >
              <FolderOpen size={13} />
              <span>打开目录</span>
            </Button>

            <Button
              variant="ghost"
              size="sm"
              title="复制完整物理路径"
              onclick={copyPath}
            >
              {#if pathCopied}
                <Check size={13} class="text-[var(--success)]" />
                <span class="text-[var(--success)]">已复制</span>
              {:else}
                <Copy size={13} />
                <span>复制路径</span>
              {/if}
            </Button>
          </div>
        </div>

        <!-- Persistent Fast Service Controls -->
        <div class="flex flex-wrap items-center gap-2.5 shrink-0">
          <!-- MCP Capsule -->
          <div class="flex items-center gap-2 rounded-xl border border-[var(--border)] bg-[var(--card-bg)] px-3 py-1.5 shadow-sm">
            <StatusOrb state={mcpStatus} />
            <div class="text-xs">
              <span class="font-semibold text-[var(--text-main)]">MCP</span>
              <span class="text-[11px] text-[var(--text-muted)] font-mono ml-1">:{profile.runtime.local_port}</span>
            </div>
            <Button
              variant={mcpStatus === "running" ? "secondary" : "primary"}
              size="sm"
              class="ml-1 px-2 py-0.5 text-[11px]"
              disabled={mcpStatus === "starting" || mcpStatus === "stopping"}
              busy={mcpBusy}
              onclick={toggleMcp}
            >
              {mcpStatus === "running" ? "停止" : "启动"}
            </Button>
          </div>

          <!-- Actions Capsule -->
          {#if actions}
            <div class="flex items-center gap-2 rounded-xl border border-[var(--border)] bg-[var(--card-bg)] px-3 py-1.5 shadow-sm">
              <StatusOrb state={actionsStatus} />
              <div class="text-xs">
                <span class="font-semibold text-[var(--text-main)]">Actions</span>
                <span class="text-[11px] text-[var(--text-muted)] font-mono ml-1">:{actions.local_port}</span>
              </div>
              <Button
                variant={actionsStatus === "running" ? "secondary" : "primary"}
                size="sm"
                class="ml-1 px-2 py-0.5 text-[11px]"
                disabled={actionsStatus === "starting" || actionsStatus === "stopping"}
                busy={actionsBusy}
                onclick={toggleActions}
              >
                {actionsStatus === "running" ? "停止" : "启动"}
              </Button>
            </div>
          {/if}
        </div>
      </div>
    </header>

    <!-- Top-Level Modern View Switcher -->
    <div class="sticky top-0 z-10 bg-[var(--page-bg)]/85 px-4 py-2.5 backdrop-blur-md border-b border-[var(--border)]">
      <div class="max-w-xl">
        <SegmentedControl
          items={workspaceTabs}
          value={activeWorkspaceTab}
          size="md"
          onchange={(value) => { activeWorkspaceTab = value as WorkspaceTab; }}
        />
      </div>
    </div>

    <!-- Workspace Main Body -->
    <div class="page-body pt-5 pb-12">
      <!-- ══════════════ VIEW 1: 服务与端点 (Cockpit) ══════════════ -->
      {#if activeWorkspaceTab === "services"}
        <div class="grid gap-5">
          <!-- View mode filter -->
          <div class="flex items-center justify-between gap-4">
            <div>
              <h3 class="text-sm font-semibold text-[var(--text-main)]">双引擎服务控制台</h3>
              <p class="text-xs text-[var(--color-text-muted)] mt-0.5">本地端点、公网穿透隧道与一键客户端配置直达</p>
            </div>
            <div class="w-72">
              <SegmentedControl
                items={serviceViewTabs}
                value={serviceViewMode}
                size="sm"
                onchange={(v) => { serviceViewMode = v as ServiceKind; }}
              />
            </div>
          </div>

          <!-- Services Grid / Cards -->
          <div class="grid gap-6 {serviceViewMode === 'both' ? 'lg:grid-cols-2' : ''}">
            <!-- MCP Engine Card -->
            {#if serviceViewMode === "mcp" || serviceViewMode === "both"}
              <Card class="p-5 flex flex-col gap-4 border-[var(--border)] shadow-sm">
                <!-- Header & Quick Actions -->
                <div class="flex items-start justify-between gap-4">
                  <div class="flex items-center gap-3">
                    <div class="flex size-10 items-center justify-center rounded-xl bg-[var(--primary-soft)] text-[var(--primary)] border border-[var(--primary)]/20 shadow-sm">
                      <Radio size={20} />
                    </div>
                    <div>
                      <div class="flex items-center gap-2">
                        <h4 class="text-base font-bold text-[var(--text-main)]">MCP 运行时</h4>
                        <StatusBadge
                          status={mcpStatus === "running" ? "running" : mcpStatus === "error" ? "error" : "stopped"}
                          text={stateLabel(mcpStatus)}
                          size="sm"
                        />
                      </div>
                      <p class="text-xs text-[var(--text-secondary)] mt-0.5">Streamable HTTP · Claude / Cursor 工具运行时</p>
                    </div>
                  </div>

                  <div class="flex items-center gap-2">
                    {#if mcpStatus === "running"}
                      <Button
                        variant="secondary"
                        size="sm"
                        title="重启 MCP 服务"
                        busy={mcpBusy}
                        onclick={() => void handleRestartService("mcp")}
                      >
                        <RotateCw size={13} />
                        <span>重启</span>
                      </Button>
                    {/if}
                    <Button
                      variant={mcpStatus === "running" ? "danger" : "primary"}
                      size="sm"
                      busy={mcpBusy}
                      disabled={mcpStatus === "starting" || mcpStatus === "stopping"}
                      onclick={toggleMcp}
                    >
                      {#if mcpStatus === "running"}
                        <Square size={13} />
                        <span>停止服务</span>
                      {:else}
                        <Play size={13} />
                        <span>启动服务</span>
                      {/if}
                    </Button>
                  </div>
                </div>

                <!-- Endpoints Row -->
                <div class="grid gap-2.5 rounded-xl border border-[var(--border)] bg-[var(--surface-main)] p-3.5">
                  <!-- Local endpoint -->
                  <div class="flex flex-wrap items-center justify-between gap-2 text-xs">
                    <span class="font-medium text-[var(--text-secondary)]">本地端点</span>
                    <div class="flex items-center gap-2">
                      <code class="font-mono text-[11px] text-[var(--text-main)] bg-[var(--card-bg)] px-2 py-0.5 rounded border border-[var(--border)]">
                        {mcpLocal || defaultMcpLocal}
                      </code>
                      <Button
                        variant="ghost"
                        size="sm"
                        class="px-2 py-0.5 text-[11px]"
                        onclick={() => copyEndpoint(mcpLocal || defaultMcpLocal, "mcp-local")}
                      >
                        {#if endpointCopied === "mcp-local"}<Check size={12} class="text-[var(--success)]" />{:else}<Copy size={12} />{/if}
                        <span>复制</span>
                      </Button>
                    </div>
                  </div>

                  <!-- Public endpoint -->
                  <div class="flex flex-wrap items-center justify-between gap-2 text-xs pt-2 border-t border-[var(--border)]">
                    <div class="flex items-center gap-1.5">
                      <span class="font-medium text-[var(--text-secondary)]">公网端点</span>
                      <span class="rounded px-1.5 py-0.2 text-[10px] uppercase font-semibold bg-[var(--primary-soft)] text-[var(--primary)] border border-[var(--primary)]/20">
                        {profile.tunnel.use_global_gateway ? "Global Gateway" : profile.tunnel.type}
                      </span>
                    </div>
                    <div class="flex items-center gap-2">
                      {#if mcpPublic}
                        <code class="font-mono text-[11px] text-[var(--text-main)] bg-[var(--card-bg)] px-2 py-0.5 rounded border border-[var(--border)] truncate max-w-xs">
                          {mcpPublic}
                        </code>
                        <Button
                          variant="ghost"
                          size="sm"
                          class="px-2 py-0.5 text-[11px]"
                          onclick={() => copyEndpoint(mcpPublic, "mcp-public")}
                        >
                          {#if endpointCopied === "mcp-public"}<Check size={12} class="text-[var(--success)]" />{:else}<Copy size={12} />{/if}
                          <span>复制</span>
                        </Button>
                      {:else}
                        <span class="text-[11px] text-[var(--color-text-muted)]">未连接 / 未启动公网隧道</span>
                      {/if}
                    </div>
                  </div>
                </div>

                <!-- Integration Hub Quick Copy -->
                <div class="pt-1">
                  <GptQuickCopy
                    workspaceId={workspaceId!}
                    service="mcp"
                    {profile}
                    publicMcpEndpoint={mcpPublic}
                    {frpProfiles}
                  />
                </div>

                <!-- Inline Collapsible Config Hub -->
                <div class="mt-2 pt-4 border-t border-[var(--border)]">
                  <div class="mb-3.5 flex items-center justify-between">
                    <p class="text-xs font-semibold text-[var(--text-main)] uppercase tracking-wider">MCP 高级配置</p>
                    <div class="w-80">
                      <SegmentedControl
                        items={mcpConfigTabs}
                        value={mcpConfigSection}
                        size="sm"
                        onchange={(v) => { mcpConfigSection = v as McpConfigSection; }}
                      />
                    </div>
                  </div>

                  <div class="rounded-xl border border-[var(--border)] bg-[var(--card-bg)] p-4">
                    {#if mcpConfigSection === "connection"}
                      <TunnelConfigForm
                        workspaceId={workspaceId!}
                        service="mcp"
                        config={mcpTunnelForm}
                        onSave={saveMcpTunnel}
                      />
                    {:else if mcpConfigSection === "auth"}
                      <AuthConfigForm
                        workspaceId={workspaceId!}
                        auth={profile.auth}
                        onSaveProfile={saveMcpAuth}
                      />
                    {:else if mcpConfigSection === "policy"}
                      <RuntimePolicyForm
                        workspaceId={workspaceId!}
                        toolProfile={profile.runtime.tool_profile}
                        permissionMode={profile.runtime.permission_mode}
                        allowedCommands={profile.runtime.allowed_commands ?? ""}
                        executablePaths={profile.runtime.executable_paths ?? ""}
                        aiInstructions={profile.runtime.ai_instructions ?? ""}
                        instructionSources={profile.runtime.instruction_sources ?? []}
                        skillSources={profile.runtime.skill_sources ?? []}
                        customInstructionPaths={profile.runtime.custom_instruction_paths ?? ""}
                        customSkillPaths={profile.runtime.custom_skill_paths ?? ""}
                        workspaceLocalEntries={profile.runtime.workspace_local_entries ?? true}
                        workspaceScriptExtensions={profile.runtime.workspace_script_extensions ?? ".exe,.bat,.cmd,.ps1"}
                        onSave={saveMcpPolicy}
                      />
                    {:else}
                      <HistoryContextPanel
                        workspaceId={workspaceId!}
                        recording={profile.runtime.history_recording ?? true}
                        selectedSessions={profile.runtime.history_context_sessions ?? []}
                        onSave={saveHistoryContext}
                      />
                    {/if}
                  </div>
                </div>
              </Card>
            {/if}

            <!-- Actions Engine Card -->
            {#if actions && (serviceViewMode === "actions" || serviceViewMode === "both")}
              <Card class="p-5 flex flex-col gap-4 border-[var(--border)] shadow-sm">
                <!-- Header & Quick Actions -->
                <div class="flex items-start justify-between gap-4">
                  <div class="flex items-center gap-3">
                    <div class="flex size-10 items-center justify-center rounded-xl bg-[var(--accent-soft)] text-[var(--accent)] border border-[var(--accent)]/20 shadow-sm">
                      <Activity size={20} />
                    </div>
                    <div>
                      <div class="flex items-center gap-2">
                        <h4 class="text-base font-bold text-[var(--text-main)]">Actions 网关</h4>
                        <StatusBadge
                          status={actionsStatus === "running" ? "running" : actionsStatus === "error" ? "error" : "stopped"}
                          text={stateLabel(actionsStatus)}
                          size="sm"
                        />
                      </div>
                      <p class="text-xs text-[var(--text-secondary)] mt-0.5">OpenAPI 网关 · ChatGPT Actions 交互运行时</p>
                    </div>
                  </div>

                  <div class="flex items-center gap-2">
                    {#if actionsStatus === "running"}
                      <Button
                        variant="secondary"
                        size="sm"
                        title="重启 Actions 服务"
                        busy={actionsBusy}
                        onclick={() => void handleRestartService("actions")}
                      >
                        <RotateCw size={13} />
                        <span>重启</span>
                      </Button>
                    {/if}
                    <Button
                      variant={actionsStatus === "running" ? "danger" : "primary"}
                      size="sm"
                      busy={actionsBusy}
                      disabled={actionsStatus === "starting" || actionsStatus === "stopping"}
                      onclick={toggleActions}
                    >
                      {#if actionsStatus === "running"}
                        <Square size={13} />
                        <span>停止服务</span>
                      {:else}
                        <Play size={13} />
                        <span>启动服务</span>
                      {/if}
                    </Button>
                  </div>
                </div>

                <!-- Endpoints Row -->
                <div class="grid gap-2.5 rounded-xl border border-[var(--border)] bg-[var(--surface-main)] p-3.5">
                  <!-- Local endpoint -->
                  <div class="flex flex-wrap items-center justify-between gap-2 text-xs">
                    <span class="font-medium text-[var(--text-secondary)]">本地 OpenAPI</span>
                    <div class="flex items-center gap-2">
                      <code class="font-mono text-[11px] text-[var(--text-main)] bg-[var(--card-bg)] px-2 py-0.5 rounded border border-[var(--border)]">
                        {actionsLocal || defaultActionsLocal}
                      </code>
                      <Button
                        variant="ghost"
                        size="sm"
                        class="px-2 py-0.5 text-[11px]"
                        onclick={() => copyEndpoint(actionsLocal || defaultActionsLocal, "actions-local")}
                      >
                        {#if endpointCopied === "actions-local"}<Check size={12} class="text-[var(--success)]" />{:else}<Copy size={12} />{/if}
                        <span>复制</span>
                      </Button>
                    </div>
                  </div>

                  <!-- Public endpoint -->
                  <div class="flex flex-wrap items-center justify-between gap-2 text-xs pt-2 border-t border-[var(--border)]">
                    <div class="flex items-center gap-1.5">
                      <span class="font-medium text-[var(--text-secondary)]">公网 OpenAPI Schema</span>
                      <span class="rounded px-1.5 py-0.2 text-[10px] uppercase font-semibold bg-[var(--accent-soft)] text-[var(--accent)] border border-[var(--accent)]/20">
                        {actions.use_global_gateway ? "Global Gateway" : actions.tunnel_type}
                      </span>
                    </div>
                    <div class="flex items-center gap-2">
                      {#if actionsPublic || actionsOpenApi}
                        <code class="font-mono text-[11px] text-[var(--text-main)] bg-[var(--card-bg)] px-2 py-0.5 rounded border border-[var(--border)] truncate max-w-xs">
                          {actionsPublic || actionsOpenApi}
                        </code>
                        <Button
                          variant="ghost"
                          size="sm"
                          class="px-2 py-0.5 text-[11px]"
                          onclick={() => copyEndpoint(actionsPublic || actionsOpenApi, "actions-public")}
                        >
                          {#if endpointCopied === "actions-public"}<Check size={12} class="text-[var(--success)]" />{:else}<Copy size={12} />{/if}
                          <span>复制</span>
                        </Button>
                      {:else}
                        <span class="text-[11px] text-[var(--color-text-muted)]">未配置公网地址</span>
                      {/if}
                    </div>
                  </div>
                </div>

                <!-- Integration Hub Quick Copy -->
                <div class="pt-1">
                  <GptQuickCopy
                    workspaceId={workspaceId!}
                    service="actions"
                    {profile}
                    {frpProfiles}
                  />
                </div>

                <!-- Inline Collapsible Config Hub -->
                <div class="mt-2 pt-4 border-t border-[var(--border)]">
                  <div class="mb-3.5 flex items-center justify-between">
                    <p class="text-xs font-semibold text-[var(--text-main)] uppercase tracking-wider">Actions 高级配置</p>
                    <div class="w-64">
                      <SegmentedControl
                        items={actionsConfigTabs}
                        value={actionsConfigSection}
                        size="sm"
                        onchange={(v) => { actionsConfigSection = v as ActionsConfigSection; }}
                      />
                    </div>
                  </div>

                  <div class="rounded-xl border border-[var(--border)] bg-[var(--card-bg)] p-4">
                    {#if actionsConfigSection === "connection"}
                      <TunnelConfigForm
                        workspaceId={workspaceId!}
                        service="actions"
                        config={actionsTunnelForm}
                        onSave={saveActionsTunnel}
                      />
                    {:else if actionsConfigSection === "auth"}
                      <ActionsAuthForm
                        workspaceId={workspaceId!}
                        authType={actions.auth_type}
                        oauthClientId={actions.oauth_client_id ?? ""}
                        oauthScopes={actions.oauth_scopes ?? ""}
                        openapiUrl={actionsOpenApi}
                        privacyUrl={actionsPrivacy}
                        oauthAuthorizeUrl={actionsOAuthAuth}
                        oauthTokenUrl={actionsOAuthToken}
                        useSharedSecrets={actions.use_shared_secrets ?? false}
                        onSave={saveActionsAuth}
                      />
                    {:else}
                      <ActionsPolicyForm
                        allowedCommands={actions.allowed_commands ?? ""}
                        maxPatchBytes={actions.max_patch_bytes ?? 200_000}
                        permissionMode={actions.permission_mode}
                        onSave={saveActionsPolicy}
                      />
                    {/if}
                  </div>
                </div>
              </Card>
            {/if}
          </div>
        </div>

      <!-- ══════════════ VIEW 2: 诊断与实时日志 (Diagnostics & Logs) ══════════════ -->
      {:else if activeWorkspaceTab === "diagnostics"}
        <div class="grid gap-6">
          <div class="flex items-center justify-between gap-4">
            <div>
              <h3 class="text-sm font-semibold text-[var(--text-main)]">实时诊断与健康中心</h3>
              <p class="text-xs text-[var(--color-text-muted)] mt-0.5">本地 endpoint、公网穿透、OAuth 签名连通性与服务输出日志</p>
            </div>
          </div>

          <!-- Top: Health Panel -->
          <HealthPanel workspaceId={workspaceId!} />

          <!-- Bottom: Unified Log Viewer with fast switcher -->
          <div class="grid gap-3">
            <div class="flex items-center justify-between gap-3">
              <span class="text-xs font-semibold text-[var(--text-secondary)] uppercase tracking-wider">实时服务日志</span>
              <div class="w-56">
                <SegmentedControl
                  items={diagnosticLogTabs}
                  value={diagnosticLogService}
                  size="sm"
                  onchange={(v) => { diagnosticLogService = v as "mcp" | "actions"; }}
                />
              </div>
            </div>

            <LogViewer workspaceId={workspaceId!} service={diagnosticLogService} />
          </div>
        </div>

      <!-- ══════════════ VIEW 3: 任务规划 (Planning Board) ══════════════ -->
      {:else if activeWorkspaceTab === "planning"}
        <div class="grid gap-4">
          <PlanningControlPanel workspaceId={workspaceId!} />
        </div>

      <!-- ══════════════ VIEW 4: 工作区设置 (Settings & Danger) ══════════════ -->
      {:else if activeWorkspaceTab === "settings"}
        <div class="grid gap-6 max-w-4xl">
          <div>
            <h3 class="text-sm font-semibold text-[var(--text-main)]">工作区基础设置</h3>
            <p class="text-xs text-[var(--color-text-muted)] mt-0.5">维护工作区展示名称、物理存储目录与 ChatGPT 初始 Prompt</p>
          </div>

          <Card class="p-5">
            <h4 class="text-xs font-semibold text-[var(--text-main)] uppercase tracking-wider mb-4">基本属性</h4>
            <WorkspaceMetaForm
              name={profile.name}
              path={profile.path}
              onSave={saveWorkspaceName}
              onUpdatePath={saveWorkspacePath}
            />
          </Card>

          <Card class="p-5">
            <h4 class="text-xs font-semibold text-[var(--text-main)] uppercase tracking-wider mb-2">ChatGPT 初始会话指令</h4>
            <p class="text-xs text-[var(--color-text-muted)] mb-4 leading-relaxed">
              为外部 ChatGPT 会话生成一键粘贴的系统初始化 Prompt，指引大模型连接当前工作区并调用工具。
            </p>
            <ChatGptSessionPrompt />
          </Card>

          <div class="rounded-2xl border border-[var(--danger)]/30 bg-[var(--danger-soft)] p-5 shadow-sm">
            <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <div>
                <h4 class="text-sm font-bold text-[var(--danger)]">删除工作区</h4>
                <p class="text-xs text-[var(--text-secondary)] mt-1 leading-relaxed">
                  仅从 Coding Tools 中移除该工作区配置、历史日志与路由，绝不会删除本地磁盘上的任何源代码项目文件。
                </p>
              </div>
              <Button
                variant="danger"
                size="md"
                onclick={requestRemoveWorkspace}
              >
                <Trash2 size={14} />
                <span>删除工作区</span>
              </Button>
            </div>
          </div>
        </div>
      {/if}
    </div>
  </section>

  <!-- High-Risk Delete Confirmation Dialog -->
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
    onCancel={() => {
      deleteConfirmOpen = false;
    }}
  />
{/if}
