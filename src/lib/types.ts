export type RuntimeState = "stopped" | "starting" | "running" | "stopping" | "error";

export interface RuntimeConfig {
  tool_profile: string;
  history_recording?: boolean;
  history_context_sessions?: number[];
  permission_mode: string;
  inherit_global_execution_policy?: boolean;
  allowed_commands?: string;
  executable_paths?: string;
  ai_instructions?: string;
  instruction_sources?: string[];
  skill_sources?: string[];
  custom_instruction_paths?: string;
  custom_skill_paths?: string;
  workspace_local_entries?: boolean;
  workspace_script_extensions?: string;
  allow_high_risk_writes?: boolean;
}

export interface WorkspaceProfile {
  id: string;
  name: string;
  path: string;
  runtime: RuntimeConfig;
}

export interface RuntimeStatus {
  state: RuntimeState;
  pid: number | null;
  localMessage: string;
  publicMessage: string;
  localEndpoint: string;
  publicEndpoint: string;
}
