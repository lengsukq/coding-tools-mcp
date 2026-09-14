export type RuntimeState = "stopped" | "starting" | "running" | "stopping" | "error";

export const DEFAULT_SERVICE_PORT = 28766;

export interface TunnelConfig {
  type: string;
  public_url: string;
  frp_server: string;
  frp_subdomain: string;
  frp_profile_id?: string;
  frp_server_port?: number;
  cloudflare_mode: string;
  use_proxy?: boolean;
  use_global_gateway?: boolean;
}

export interface AuthConfig {
  type: string;
  oauth_client_id: string;
  use_shared_secrets?: boolean;
}

export interface RuntimeConfig {
  local_port: number;
  tool_profile: string;
  history_recording?: boolean;
  history_context_sessions?: number[];
  permission_mode: string;
  runtime_command?: string;
  allowed_commands?: string;
  executable_paths?: string;
  ai_instructions?: string;
  instruction_sources?: string[];
  skill_sources?: string[];
  custom_instruction_paths?: string;
  custom_skill_paths?: string;
  workspace_local_entries?: boolean;
  workspace_script_extensions?: string;
}

export interface WorkspaceProfile {
  id: string;
  name: string;
  path: string;
  tunnel: TunnelConfig;
  auth: AuthConfig;
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

export function mcpLocalEndpoint(port: number): string {
  return `http://127.0.0.1:${port}/mcp`;
}

export interface FrpProfileSummary {
  id: string;
  name: string;
  server: string;
  serverPort: number;
}

export function frpPublicUrl(
  tunnelType: string,
  frpSubdomain: string,
  frpServer: string,
  frpProfileId: string | undefined,
  profiles: FrpProfileSummary[],
  publicUrl = "",
): string {
  if (tunnelType !== "frp" || !frpSubdomain) {
    return publicUrl.replace(/\/$/, "");
  }
  const server =
    profiles.find((profile) => profile.id === frpProfileId)?.server ?? frpServer;
  if (!server) return publicUrl.replace(/\/$/, "");
  return `https://${frpSubdomain}.${server}`;
}
