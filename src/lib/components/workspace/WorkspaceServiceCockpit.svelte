<script lang="ts">
  import { Check, Copy, Play, Radio, RotateCw, Square } from "@lucide/svelte";
  import AuthConfigForm from "$lib/components/AuthConfigForm.svelte";
  import GptQuickCopy from "$lib/components/GptQuickCopy.svelte";
  import HistoryContextPanel from "$lib/components/HistoryContextPanel.svelte";
  import RuntimePolicyForm, { type RuntimePolicyDraft } from "$lib/components/RuntimePolicyForm.svelte";
  import TunnelConfigForm, {
    type SaveTunnelOptions,
    type TunnelFormConfig,
  } from "$lib/components/TunnelConfigForm.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import SegmentedControl from "$lib/components/ui/SegmentedControl.svelte";
  import StatusBadge from "$lib/components/ui/StatusBadge.svelte";
  import {
    MCP_CONFIG_TABS,
    stateLabel,
    tunnelFormFromProfile,
    type McpConfigSection,
  } from "$lib/workspace-page";
  import type { AuthConfig, RuntimeState, WorkspaceProfile } from "$lib/types";

  export interface WorkspaceRuntimeView {
    state: RuntimeState;
    busy: boolean;
    localEndpoint: string;
    publicEndpoint: string;
    defaultLocalEndpoint: string;
    endpointCopied: string | null;
  }

  export interface WorkspaceServiceActions {
    toggle: () => void | Promise<void>;
    restart: () => void | Promise<void>;
    copyEndpoint: (url: string, id: string) => void;
    saveTunnel: (config: TunnelFormConfig, options?: SaveTunnelOptions) => void | Promise<void>;
    saveAuth: (auth: AuthConfig, options?: { skipRuntimeRestart?: boolean }) => void | Promise<void>;
    savePolicy: (draft: RuntimePolicyDraft) => void | Promise<void>;
    saveHistory: (recording: boolean, selectedSessions: number[]) => void | Promise<void>;
  }

  interface Props {
    workspaceId: string;
    profile: WorkspaceProfile;
    runtime: WorkspaceRuntimeView;
    actions: WorkspaceServiceActions;
  }

  let { workspaceId, profile, runtime, actions }: Props = $props();
  let configSection = $state<McpConfigSection>("connection");
  const tunnelForm = $derived<TunnelFormConfig>(tunnelFormFromProfile(profile));
</script>

<div class="grid gap-5">
  <div class="flex items-center justify-between gap-4">
    <div>
      <h3 class="text-sm font-semibold text-[var(--text-main)]">MCP 服务控制台</h3>
      <p class="text-xs text-[var(--color-text-muted)] mt-0.5">本地端点、公网穿透隧道与一键客户端配置直达</p>
    </div>
  </div>

  <div class="grid gap-6">
    <Card class="p-5 flex flex-col gap-4 border-[var(--border)] shadow-sm">
      <div class="flex items-start justify-between gap-4">
        <div class="flex items-center gap-3">
          <div class="flex size-10 items-center justify-center rounded-xl bg-[var(--primary-soft)] text-[var(--primary)] border border-[var(--primary)]/20 shadow-sm">
            <Radio size={20} />
          </div>
          <div>
            <div class="flex items-center gap-2">
              <h4 class="text-base font-bold text-[var(--text-main)]">MCP 运行时</h4>
              <StatusBadge
                status={runtime.state === "running" ? "running" : runtime.state === "error" ? "error" : "stopped"}
                text={stateLabel(runtime.state)}
                size="sm"
              />
            </div>
            <p class="text-xs text-[var(--text-secondary)] mt-0.5">Streamable HTTP · Claude / Cursor 工具运行时</p>
          </div>
        </div>

        <div class="flex items-center gap-2">
          {#if runtime.state === "running"}
            <Button variant="secondary" size="sm" title="重启 MCP 服务" busy={runtime.busy} onclick={actions.restart}>
              <RotateCw size={13} />
              <span>重启</span>
            </Button>
          {/if}
          <Button
            variant={runtime.state === "running" ? "danger" : "primary"}
            size="sm"
            busy={runtime.busy}
            disabled={runtime.state === "starting" || runtime.state === "stopping"}
            onclick={actions.toggle}
          >
            {#if runtime.state === "running"}
              <Square size={13} />
              <span>停止服务</span>
            {:else}
              <Play size={13} />
              <span>启动服务</span>
            {/if}
          </Button>
        </div>
      </div>

      <div class="grid gap-2.5 rounded-xl border border-[var(--border)] bg-[var(--surface-main)] p-3.5">
        <div class="flex flex-wrap items-center justify-between gap-2 text-xs">
          <span class="font-medium text-[var(--text-secondary)]">本地端点</span>
          <div class="flex items-center gap-2">
            <code class="font-mono text-[11px] text-[var(--text-main)] bg-[var(--card-bg)] px-2 py-0.5 rounded border border-[var(--border)]">
              {runtime.localEndpoint || runtime.defaultLocalEndpoint}
            </code>
            <Button
              variant="ghost"
              size="sm"
              class="px-2 py-0.5 text-[11px]"
              onclick={() => actions.copyEndpoint(runtime.localEndpoint || runtime.defaultLocalEndpoint, "mcp-local")}
            >
              {#if runtime.endpointCopied === "mcp-local"}<Check size={12} class="text-[var(--success)]" />{:else}<Copy size={12} />{/if}
              <span>复制</span>
            </Button>
          </div>
        </div>

        <div class="flex flex-wrap items-center justify-between gap-2 text-xs pt-2 border-t border-[var(--border)]">
          <div class="flex items-center gap-1.5">
            <span class="font-medium text-[var(--text-secondary)]">公网端点</span>
            <span class="rounded px-1.5 py-0.2 text-[10px] uppercase font-semibold bg-[var(--primary-soft)] text-[var(--primary)] border border-[var(--primary)]/20">
              {profile.tunnel.use_global_gateway ? "Global Gateway" : profile.tunnel.type}
            </span>
          </div>
          <div class="flex items-center gap-2">
            {#if runtime.publicEndpoint}
              <code class="font-mono text-[11px] text-[var(--text-main)] bg-[var(--card-bg)] px-2 py-0.5 rounded border border-[var(--border)] truncate max-w-xs">
                {runtime.publicEndpoint}
              </code>
              <Button
                variant="ghost"
                size="sm"
                class="px-2 py-0.5 text-[11px]"
                onclick={() => actions.copyEndpoint(runtime.publicEndpoint, "mcp-public")}
              >
                {#if runtime.endpointCopied === "mcp-public"}<Check size={12} class="text-[var(--success)]" />{:else}<Copy size={12} />{/if}
                <span>复制</span>
              </Button>
            {:else}
              <span class="text-[11px] text-[var(--color-text-muted)]">未连接 / 未启动公网隧道</span>
            {/if}
          </div>
        </div>
      </div>

      <div class="pt-1">
        <GptQuickCopy {workspaceId} {profile} publicMcpEndpoint={runtime.publicEndpoint} />
      </div>

      <div class="mt-2 pt-4 border-t border-[var(--border)]">
        <div class="mb-3.5 flex items-center justify-between">
          <p class="text-xs font-semibold text-[var(--text-main)] uppercase tracking-wider">MCP 高级配置</p>
          <div class="w-80">
            <SegmentedControl
              items={MCP_CONFIG_TABS}
              value={configSection}
              size="sm"
              onchange={(value) => { configSection = value as McpConfigSection; }}
            />
          </div>
        </div>

        <div class="rounded-xl border border-[var(--border)] bg-[var(--card-bg)] p-4">
          {#if configSection === "connection"}
            <TunnelConfigForm {workspaceId} config={tunnelForm} onSave={actions.saveTunnel} />
          {:else if configSection === "auth"}
            <AuthConfigForm {workspaceId} auth={profile.auth} onSaveProfile={actions.saveAuth} />
          {:else if configSection === "policy"}
            <RuntimePolicyForm
              {workspaceId}
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
              onSave={actions.savePolicy}
            />
          {:else}
            <HistoryContextPanel
              {workspaceId}
              recording={profile.runtime.history_recording ?? true}
              selectedSessions={profile.runtime.history_context_sessions ?? []}
              onSave={actions.saveHistory}
            />
          {/if}
        </div>
      </div>
    </Card>
  </div>
</div>
