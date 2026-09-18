import { listWorkspaces } from "$lib/api/workspaces";
import type { WorkspaceProfile } from "$lib/types";

export interface RuntimePolicyDraft {
  toolProfile: string;
  permissionMode: string;
  inheritGlobalExecutionPolicy: boolean;
  allowedCommands: string;
  executablePaths: string;
  aiInstructions: string;
  instructionSources: string[];
  skillSources: string[];
  customInstructionPaths: string;
  customSkillPaths: string;
  workspaceLocalEntries: boolean;
  workspaceScriptExtensions: string;
}

export type WorkspaceTab = "overview" | "services" | "planning" | "settings";

export const WORKSPACE_TABS: Array<{ value: WorkspaceTab; label: string }> = [
  { value: "overview", label: "概览" },
  { value: "planning", label: "任务规划" },
  { value: "services", label: "运行与诊断" },
  { value: "settings", label: "设置" },
];

export interface WorkspaceSnapshot {
  items: WorkspaceProfile[];
  profile: WorkspaceProfile | null;
}

export function normalizeWorkspaceProfile(profile: WorkspaceProfile): WorkspaceProfile {
  const runtime = profile.runtime ?? ({} as WorkspaceProfile["runtime"]);

  return {
    ...profile,
    runtime: {
      ...runtime,
      tool_profile: runtime.tool_profile ?? "core",
      permission_mode: runtime.permission_mode ?? "safe",
      inherit_global_execution_policy: runtime.inherit_global_execution_policy ?? false,
    },
  };
}
export async function loadWorkspaceSnapshot(id: string): Promise<WorkspaceSnapshot> {
  const items = (await listWorkspaces()).map(normalizeWorkspaceProfile);
  const profile = items.find((item) => item.id === id) ?? null;
  return { items, profile };
}

export async function refreshWorkspaceSnapshot(id: string) {
  const items = (await listWorkspaces()).map(normalizeWorkspaceProfile);
  return {
    items,
    profile: items.find((item) => item.id === id) ?? null,
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
      inherit_global_execution_policy: draft.inheritGlobalExecutionPolicy,
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
