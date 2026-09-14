import { getRuntimeStatus, listWorkspaces } from "$lib/api/workspaces";
import type { RuntimePolicyDraft } from "$lib/components/RuntimePolicyForm.svelte";
import type { TunnelFormConfig } from "$lib/components/TunnelConfigForm.svelte";
import type { AuthConfig, RuntimeState, RuntimeStatus, WorkspaceProfile } from "$lib/types";

export type WorkspaceTab = "services" | "diagnostics" | "planning" | "settings";
export type McpConfigSection = "connection" | "auth" | "policy" | "history";

export const WORKSPACE_TABS: Array<{ value: WorkspaceTab; label: string }> = [
  { value: "services", label: "服务与端点" },
  { value: "diagnostics", label: "诊断与日志" },
  { value: "planning", label: "任务规划" },
  { value: "settings", label: "工作区设置" },
];

export const MCP_CONFIG_TABS: Array<{ value: McpConfigSection; label: string }> = [
  { value: "connection", label: "连接与隧道" },
  { value: "auth", label: "访问认证" },
  { value: "policy", label: "执行权限" },
  { value: "history", label: "历史上下文" },
];

export interface WorkspaceSnapshot {
  items: WorkspaceProfile[];
  profile: WorkspaceProfile | null;
  runtime: RuntimeStatus | null;
}

export function stateLabel(state: RuntimeState): string {
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

export function tunnelConfigured(type: string | undefined): boolean {
  return type === "cloudflare" || type === "frp";
}

export function tunnelFormFromProfile(profile: WorkspaceProfile | null): TunnelFormConfig {
  return {
    type: profile?.tunnel.type ?? "none",
    public_url: profile?.tunnel.public_url ?? "",
    frp_server: profile?.tunnel.frp_server ?? "",
    frp_subdomain: profile?.tunnel.frp_subdomain ?? "",
    frp_profile_id: profile?.tunnel.frp_profile_id ?? "",
    frp_server_port: profile?.tunnel.frp_server_port ?? 7000,
    cloudflare_mode: profile?.tunnel.cloudflare_mode ?? "quick",
    use_proxy: profile?.tunnel.use_proxy ?? true,
    use_global_gateway: profile?.tunnel.use_global_gateway ?? false,
  };
}

export async function loadWorkspaceSnapshot(id: string): Promise<WorkspaceSnapshot> {
  const items = await listWorkspaces();
  const profile = items.find((item) => item.id === id) ?? null;
  const runtime = profile ? await getRuntimeStatus(id) : null;
  return { items, profile, runtime };
}

export async function refreshWorkspaceSnapshot(id: string) {
  const items = await listWorkspaces();
  return {
    items,
    profile: items.find((item) => item.id === id) ?? null,
  };
}

export function withRuntimePort(profile: WorkspaceProfile, port: number): WorkspaceProfile {
  return {
    ...profile,
    runtime: { ...profile.runtime, local_port: port },
  };
}

export function withTunnelConfig(
  profile: WorkspaceProfile,
  config: TunnelFormConfig,
): WorkspaceProfile {
  return {
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
}

export function withRuntimePolicy(
  profile: WorkspaceProfile,
  draft: RuntimePolicyDraft,
): WorkspaceProfile {
  return {
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
}

export function withHistoryContext(
  profile: WorkspaceProfile,
  recording: boolean,
  selectedSessions: number[],
): WorkspaceProfile {
  return {
    ...profile,
    runtime: {
      ...profile.runtime,
      history_recording: recording,
      history_context_sessions: selectedSessions,
    },
  };
}

export function withAuth(profile: WorkspaceProfile, auth: AuthConfig): WorkspaceProfile {
  return { ...profile, auth };
}
