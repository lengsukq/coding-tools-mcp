<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
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
  import ServicePanel from "$lib/components/ServicePanel.svelte";
  import GptQuickCopy from "$lib/components/GptQuickCopy.svelte";
  import StatusOrb from "$lib/components/StatusOrb.svelte";
  import Tabs from "$lib/components/Tabs.svelte";
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
  } from "$lib/api/workspaces";
  import { listFrpProfiles, setLastWorkspace, type FrpProfileDto } from "$lib/api/settings";
  import { confirm } from "@tauri-apps/plugin-dialog";
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
    type WorkspaceProfile,
  } from "$lib/types";

  type WorkspaceTab = "overview" | "mcp" | "actions" | "planning";
  type SubTab = "config" | "logs" | "health";
  type ConfigSection = "connection" | "auth" | "policy" | "history";

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

  let activeWorkspaceTab = $state<WorkspaceTab>("overview");
  let mcpSubTab = $state<SubTab>("config");
  let actionsSubTab = $state<SubTab>("config");
  let mcpConfigSection = $state<ConfigSection>("connection");
  let actionsConfigSection = $state<ConfigSection>("connection");
  let loadGeneration = 0;

  const workspaceTabs = [
    { value: "overview", label: "概览" },
    { value: "mcp", label: "MCP" },
    { value: "actions", label: "Actions" },
    { value: "planning", label: "规划" },
  ];

  const subTabs = [
    { value: "config", label: "设置" },
    { value: "logs", label: "日志" },
    { value: "health", label: "健康" },
  ];

  const configSections = [
    { value: "connection", label: "连接", description: "端口、隧道与公网访问" },
    { value: "auth", label: "认证", description: "访问身份与授权方式" },
    { value: "policy", label: "权限", description: "工具、命令与执行边界" },
    { value: "history", label: "历史上下文", description: "记录与选择注入的旧会话" },
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

  function stateLabel(state: RuntimeState): string {
    switch (state) {
      case "running":
        return "运行中";
      case "starting":
        return "启动中";
      case "stopping":
        return "停止中";
      case "error":
        return "错误";
      default:
        return "已停止";
    }
  }

  function applyMcpRuntime(
    runtime: { state: RuntimeState; localEndpoint: string; publicEndpoint: string; localMessage?: string },
    id = workspaceId,
  ) {
    if (!id || id !== workspaceId) return;
    mcpStatus = runtime.state;
    mcpStatusMessage = runtime.localMessage ?? "";
    mcpLocal = runtime.localEndpoint;
    mcpPublic = runtime.publicEndpoint;
    mcpRuntimeStates.update((current) => ({ ...current, [id]: runtime.state }));
  }

  function applyActionsRuntime(runtime: {
    state: RuntimeState;
    localEndpoint: string;
    publicEndpoint: string;
    localMessage?: string;
  },
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
    if (runtime.state === "running" && tunnelConfigured(tunnelType) && !runtime.publicEndpoint) {
      showToast(
        "本地服务已启动，但隧道未能自动连接。请检查代理设置与隧道配置，或查看日志。",
        { title: "隧道未连接", kind: "warning", duration: 8000 },
      );
    }
  }

  async function toggleMcp() {
    const id = workspaceId;
    if (!id || mcpBusy) return;
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
      mcpBusy = false;
    }
  }

  async function toggleActions() {
    const id = workspaceId;
    if (!id || actionsBusy) return;
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
      actionsBusy = false;
    }
  }

  async function saveMcpPort(port: number) {
    if (!profile || profile.runtime.local_port === port) return;
    const next: WorkspaceProfile = {
      ...profile,
      runtime: { ...profile.runtime, local_port: port },
    };
    await updateWorkspace(next);
    profile = next;
    mcpLocal = mcpLocalEndpoint(port);
    await load();
  }

  async function saveActionsPort(port: number) {
    if (!profile) return;
    const current = actionsConfig(profile);
    if (current.local_port === port) return;
    const next: WorkspaceProfile = {
      ...profile,
      actions: { ...current, local_port: port },
    };
    await updateWorkspace(next);
    profile = next;
    actionsLocal = actionsLocalEndpoint(port);
    await load();
  }

  function publicEndpointFromTunnel(config: TunnelFormConfig, suffix: string): string {
    const base = frpPublicUrl(
      config.type,
      config.frp_subdomain,
      config.frp_server,
      config.frp_profile_id,
      frpProfiles,
      config.public_url,
    );
    if (base) {
      return `${base.replace(/\/$/, "")}${suffix}`;
    }
    return "";
  }

  async function restartTunnelIfConfigured(
    targetWorkspaceId: string,
    config: TunnelFormConfig,
    service: "mcp" | "actions",
  ) {
    if (config.type === "none") {
      await stopTunnel(targetWorkspaceId, service);
      return;
    }
    const status = await restartTunnel(targetWorkspaceId, service);
    if (workspaceId !== targetWorkspaceId) return;
    if (status.publicUrl) {
      if (service === "mcp") {
        mcpPublic = `${status.publicUrl.replace(/\/$/, "")}/mcp`;
      } else {
        actionsPublic = `${status.publicUrl.replace(/\/$/, "")}/openapi.json`;
      }
    }
  }

  async function saveMcpTunnel(config: TunnelFormConfig, options?: SaveTunnelOptions) {
    if (!profile) return;
    const targetWorkspaceId = workspaceId;
    if (!targetWorkspaceId) return;
    const next: WorkspaceProfile = {
      ...profile,
      tunnel: {
        ...profile.tunnel,
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
    if (!options?.skipTunnelRestart) {
      await restartTunnelIfConfigured(targetWorkspaceId, config, "mcp");
    }
    if (workspaceId !== targetWorkspaceId) return;
    profile = next;
    mcpPublic = publicEndpointFromTunnel(config, "/mcp");
    if (!options?.skipTunnelRestart && !options?.skipServicePrompt) {
      await load();
      if (workspaceId !== targetWorkspaceId) return;
    }
    if (!options?.skipServicePrompt) {
      await promptServiceRestart(mcpStatus === "running", "MCP 服务");
    }
  }

  async function saveActionsTunnel(config: TunnelFormConfig, options?: SaveTunnelOptions) {
    if (!profile) return;
    const targetWorkspaceId = workspaceId;
    if (!targetWorkspaceId) return;
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
    if (!options?.skipTunnelRestart) {
      await restartTunnelIfConfigured(targetWorkspaceId, config, "actions");
    }
    if (workspaceId !== targetWorkspaceId) return;
    profile = next;
    actionsPublic = publicEndpointFromTunnel(config, "/openapi.json");
    if (!options?.skipTunnelRestart && !options?.skipServicePrompt) {
      await load();
      if (workspaceId !== targetWorkspaceId) return;
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

  async function removeWorkspace() {
    if (!profile || !workspaceId) return;
    const confirmed = await confirm(`确定删除工作区「${profile.name}」？此操作不可撤销。`, {
      title: "删除工作区",
      kind: "warning",
      okLabel: "删除",
      cancelLabel: "取消",
    });
    if (!confirmed) return;
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
    goto("/");
  }

  $effect(() => {
    const id = workspaceId;
    if (!id) return;
    profile = null;
    void load(id);

    return () => {
      loadGeneration += 1;
    };
  });
</script>

{#if profile && actions}
  <section class="tx-workspace-page">
    <div class="tx-workspace-scroll">
      <header class="tx-workspace-header">
        <div class="tx-workspace-header-inner">
          <div class="min-w-0">
            <p class="page-kicker">工作区</p>
            <div class="mt-1 flex min-w-0 items-center gap-3">
              <h2 class="page-title !mt-0 truncate">{profile.name}</h2>
              <span class="tx-workspace-path truncate">{profile.path}</span>
            </div>
          </div>
          <div class="tx-workspace-statuses" aria-label="服务运行状态">
            <button
              type="button"
              class="tx-service-status-chip"
              class:active={mcpStatus === "running"}
              onclick={() => (activeWorkspaceTab = "mcp")}
            >
              <StatusOrb state={mcpStatus} />
              <span>MCP</span>
              <small>{stateLabel(mcpStatus)}</small>
            </button>
            <button
              type="button"
              class="tx-service-status-chip"
              class:active={actionsStatus === "running"}
              onclick={() => (activeWorkspaceTab = "actions")}
            >
              <StatusOrb state={actionsStatus} />
              <span>Actions</span>
              <small>{stateLabel(actionsStatus)}</small>
            </button>
          </div>
        </div>
      </header>

      <div class="tx-workspace-tabs">
        <Tabs
          items={workspaceTabs}
          value={activeWorkspaceTab}
          onchange={(value) => {
            activeWorkspaceTab = value as WorkspaceTab;
          }}
        />
      </div>

      <div class="page-body tx-workspace-body">
      {#if activeWorkspaceTab === "overview"}
        <div class="tx-workspace-section-stack">
          <div class="tx-summary-grid">
            <button
              type="button"
              class="tx-summary-card tx-summary-card-button"
              onclick={() => (activeWorkspaceTab = "mcp")}
            >
              <span class="tx-summary-label">MCP</span>
              <strong class="flex items-center gap-2">
                <StatusOrb state={mcpStatus} />
                {stateLabel(mcpStatus)}
              </strong>
              <small>{mcpLocal || "尚未启动"}</small>
            </button>
            <button
              type="button"
              class="tx-summary-card tx-summary-card-button"
              onclick={() => (activeWorkspaceTab = "actions")}
            >
              <span class="tx-summary-label">Actions</span>
              <strong class="flex items-center gap-2">
                <StatusOrb state={actionsStatus} />
                {stateLabel(actionsStatus)}
              </strong>
              <small>{actionsLocal || "尚未启动"}</small>
            </button>
            <div class="tx-summary-card">
              <span class="tx-summary-label">工作区目录</span>
              <strong>{profile.name}</strong>
              <small>{profile.path}</small>
            </div>
          </div>

          <div class="tx-card p-5">
            <div class="tx-workspace-section-heading">
              <div>
                <p class="tx-section-label">工作区信息</p>
                <p class="tx-workspace-section-description">名称和本地目录只在这里维护，不再混入服务配置。</p>
              </div>
            </div>
            <div class="mt-4">
              <WorkspaceMetaForm
                name={profile.name}
                path={profile.path}
                onSave={saveWorkspaceName}
                onUpdatePath={saveWorkspacePath}
              />
            </div>
          </div>

          <div class="tx-card p-5">
            <div class="tx-workspace-section-heading">
              <div>
                <p class="tx-section-label">ChatGPT 会话</p>
                <p class="tx-workspace-section-description">复制当前工作区对应的会话初始化提示。</p>
              </div>
            </div>
            <div class="mt-4">
              <ChatGptSessionPrompt />
            </div>
          </div>

          <div class="tx-danger-zone">
            <div>
              <strong>删除工作区</strong>
              <p>仅删除 Coding Tools 中的工作区配置，不会删除本地项目文件。</p>
            </div>
            <button
              type="button"
              class="tx-btn-ghost text-[var(--danger)]"
              onclick={() => void removeWorkspace()}
            >
              删除工作区
            </button>
          </div>
        </div>
      {:else if activeWorkspaceTab === "planning"}
        <div class="tx-workspace-section-stack">
          <div class="tx-workspace-section-heading">
            <div>
              <p class="tx-section-label">规划</p>
              <p class="tx-workspace-section-description">规划状态和执行控制独立放置，避免与运行时配置互相干扰。</p>
            </div>
          </div>
          <PlanningControlPanel workspaceId={workspaceId!} />
        </div>
      {:else if activeWorkspaceTab === "mcp"}
        <div class="tx-workspace-section-stack">
          <ServicePanel
            title="MCP"
            subtitle="Streamable HTTP · 工具运行时"
            status={mcpStatus}
            statusMessage={mcpStatusMessage}
            port={profile.runtime.local_port}
            portEditable={true}
            busy={mcpBusy}
            tunnelType={profile.tunnel.type}
            localEndpoint={mcpLocal || mcpLocalEndpoint(profile.runtime.local_port)}
            publicEndpoint={mcpPublic}
            publicLabel="公网 MCP"
            showToggle={false}
            onToggle={toggleMcp}
            onPortChange={saveMcpPort}
          />
          <GptQuickCopy
            workspaceId={workspaceId!}
            service="mcp"
            {profile}
            publicMcpEndpoint={mcpPublic}
            {frpProfiles}
          />
        </div>

        <div class="mt-5 tx-service-subtabs">
          <Tabs
            items={subTabs}
            value={mcpSubTab}
            onchange={(v) => {
              mcpSubTab = v as SubTab;
            }}
          />
        </div>

        {#if mcpSubTab === "config"}
          <div class="tx-config-workbench mt-4">
            <nav class="tx-config-nav" aria-label="MCP 设置分类">
              <p class="tx-config-nav-title">MCP 设置</p>
              {#each configSections as section}
                <button
                  type="button"
                  class="tx-config-nav-item"
                  class:active={mcpConfigSection === section.value}
                  onclick={() => (mcpConfigSection = section.value as ConfigSection)}
                >
                  <strong>{section.label}</strong>
                  <small>{section.description}</small>
                </button>
              {/each}
            </nav>

            <div class="tx-card tx-config-stage p-5">
              {#if mcpConfigSection === "connection"}
                <div class="tx-config-stage-heading">
                  <div>
                    <p class="tx-section-label">连接与隧道</p>
                    <p>管理 MCP 的公网访问方式。本地端口可直接在上方服务卡修改。</p>
                  </div>
                </div>
                <TunnelConfigForm
                  workspaceId={workspaceId!}
                  service="mcp"
                  config={mcpTunnelForm}
                  onSave={saveMcpTunnel}
                />
              {:else if mcpConfigSection === "auth"}
                <div class="tx-config-stage-heading">
                  <div>
                    <p class="tx-section-label">访问认证</p>
                    <p>只处理谁可以访问 MCP，不与执行权限混在一起。</p>
                  </div>
                </div>
                <AuthConfigForm
                  workspaceId={workspaceId!}
                  auth={profile.auth}
                  onSaveProfile={saveMcpAuth}
                />
              {:else if mcpConfigSection === "policy"}
                <div class="tx-config-stage-heading">
                  <div>
                    <p class="tx-section-label">执行权限</p>
                    <p>控制 AI 可以使用哪些工具、命令和本地执行能力。</p>
                  </div>
                </div>
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
        {:else if mcpSubTab === "logs"}
          <div class="mt-4 tx-card p-4">
            <LogViewer workspaceId={workspaceId!} service="mcp" />
          </div>
        {:else}
          <div class="mt-4 tx-card p-4">
            <HealthPanel workspaceId={workspaceId!} />
          </div>
        {/if}
      {:else if activeWorkspaceTab === "actions"}
        <div class="tx-workspace-section-stack">
          <ServicePanel
            title="Actions"
            subtitle="OpenAPI 网关 · ChatGPT Actions"
            status={actionsStatus}
            statusMessage={actionsStatusMessage}
            port={actions.local_port}
            portEditable={true}
            busy={actionsBusy}
            tunnelType={actions.tunnel_type}
            localEndpoint={actionsLocal || actionsLocalEndpoint(actions.local_port)}
            publicEndpoint={actionsPublic || actionsOpenApiUrl(profile, frpProfiles)}
            publicLabel="OpenAPI"
            showToggle={false}
            onToggle={toggleActions}
            onPortChange={saveActionsPort}
          />
          <GptQuickCopy
            workspaceId={workspaceId!}
            service="actions"
            {profile}
            {frpProfiles}
          />
        </div>

        <div class="mt-5 tx-service-subtabs">
          <Tabs
            items={subTabs}
            value={actionsSubTab}
            onchange={(v) => {
              actionsSubTab = v as SubTab;
            }}
          />
        </div>

        {#if actionsSubTab === "config"}
          <div class="tx-config-workbench mt-4">
            <nav class="tx-config-nav" aria-label="Actions 设置分类">
              <p class="tx-config-nav-title">Actions 设置</p>
              {#each configSections as section}
                <button
                  type="button"
                  class="tx-config-nav-item"
                  class:active={actionsConfigSection === section.value}
                  onclick={() => (actionsConfigSection = section.value as ConfigSection)}
                >
                  <strong>{section.label}</strong>
                  <small>{section.description}</small>
                </button>
              {/each}
            </nav>

            <div class="tx-card tx-config-stage p-5">
              {#if actionsConfigSection === "connection"}
                <div class="tx-config-stage-heading">
                  <div>
                    <p class="tx-section-label">连接与隧道</p>
                    <p>管理 Actions 的公网访问方式。本地端口可直接在上方服务卡修改。</p>
                  </div>
                </div>
                <TunnelConfigForm
                  workspaceId={workspaceId!}
                  service="actions"
                  config={actionsTunnelForm}
                  onSave={saveActionsTunnel}
                />
              {:else if actionsConfigSection === "auth"}
                <div class="tx-config-stage-heading">
                  <div>
                    <p class="tx-section-label">访问认证</p>
                    <p>集中管理 ChatGPT Actions 的认证方式和 OAuth 配置。</p>
                  </div>
                </div>
                <ActionsAuthForm
                  workspaceId={workspaceId!}
                  authType={actions.auth_type}
                  oauthClientId={actions.oauth_client_id ?? ""}
                  oauthScopes={actions.oauth_scopes ?? ""}
                  openapiUrl={actionsOpenApiUrl(profile, frpProfiles)}
                  privacyUrl={actionsPrivacyUrl(profile, frpProfiles)}
                  oauthAuthorizeUrl={actionsOAuthAuthorizeUrl(profile, frpProfiles)}
                  oauthTokenUrl={actionsOAuthTokenUrl(profile, frpProfiles)}
                  useSharedSecrets={actions.use_shared_secrets ?? false}
                  onSave={saveActionsAuth}
                />
              {:else}
                <div class="tx-config-stage-heading">
                  <div>
                    <p class="tx-section-label">执行权限</p>
                    <p>控制 Actions 可以执行的命令范围和补丁写入边界。</p>
                  </div>
                </div>
                <ActionsPolicyForm
                  allowedCommands={actions.allowed_commands ?? ""}
                  maxPatchBytes={actions.max_patch_bytes ?? 200_000}
                  permissionMode={actions.permission_mode}
                  onSave={saveActionsPolicy}
                />
              {/if}
            </div>
          </div>
        {:else if actionsSubTab === "logs"}
          <div class="mt-4 tx-card p-4">
            <LogViewer workspaceId={workspaceId!} service="actions" />
          </div>
        {:else}
          <div class="mt-4 tx-card p-4">
            <HealthPanel workspaceId={workspaceId!} />
          </div>
        {/if}
        {/if}
      </div>
    </div>

    <div class="tx-workspace-action-bar">
      <div class="tx-workspace-action-bar-inner">
        <button
          type="button"
          class="tx-runtime-jump"
          onclick={() => (activeWorkspaceTab = "mcp")}
        >
          <StatusOrb state={mcpStatus} />
          <span>
            <strong>MCP</strong>
            <small>{stateLabel(mcpStatus)}</small>
          </span>
        </button>
        <button
          type="button"
          class="tx-btn-primary tx-runtime-action"
          class:tx-btn-danger={mcpStatus === "running"}
          disabled={mcpBusy || mcpStatus === "starting" || mcpStatus === "stopping"}
          onclick={toggleMcp}
        >
          {#if mcpBusy}
            处理中…
          {:else if mcpStatus === "running"}
            停止 MCP
          {:else}
            启动 MCP
          {/if}
        </button>

        <div class="tx-runtime-divider"></div>

        <button
          type="button"
          class="tx-runtime-jump"
          onclick={() => (activeWorkspaceTab = "actions")}
        >
          <StatusOrb state={actionsStatus} />
          <span>
            <strong>Actions</strong>
            <small>{stateLabel(actionsStatus)}</small>
          </span>
        </button>
        <button
          type="button"
          class="tx-btn-primary tx-runtime-action"
          class:tx-btn-danger={actionsStatus === "running"}
          disabled={actionsBusy || actionsStatus === "starting" || actionsStatus === "stopping"}
          onclick={toggleActions}
        >
          {#if actionsBusy}
            处理中…
          {:else if actionsStatus === "running"}
            停止 Actions
          {:else}
            启动 Actions
          {/if}
        </button>
      </div>
    </div>
  </section>
{/if}
