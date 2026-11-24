import { invoke } from "@tauri-apps/api/core";

export type UpstreamExposeMode = "direct" | "proxy";

export type UpstreamTransportConfig =
  | {
      type: "stdio";
      command: string;
      args: string[];
      cwd?: string | null;
      env: Record<string, string>;
      secretEnvKeys: string[];
    }
  | {
      type: "streamable_http";
      url: string;
      headers: Record<string, string>;
      secretHeaderKeys: string[];
      hasBearerToken: boolean;
      useProxy: boolean;
    }
  | {
      type: "sse";
      url: string;
      headers: Record<string, string>;
      secretHeaderKeys: string[];
      hasBearerToken: boolean;
      useProxy: boolean;
    };

export interface UpstreamServerConfig {
  id: string;
  alias: string;
  name: string;
  description: string;
  enabled: boolean;
  exposeMode: UpstreamExposeMode;
  transport: UpstreamTransportConfig;
  disabledTools: string[];
  callTimeoutSecs: number;
  idleTimeoutSecs: number;
  source: string;
}

export interface UpstreamSecretPatch {
  setEnvSecrets?: Record<string, string>;
  removeEnvSecrets?: string[];
  setHeaderSecrets?: Record<string, string>;
  removeHeaderSecrets?: string[];
  /** `undefined` = unchanged, `null` = clear, `string` = set */
  bearerToken?: string | null;
}

export interface UpstreamToolItemDto {
  originalName: string;
  exposedName: string;
  title: string | null;
  description: string;
  disabled: boolean;
  inputSchema: Record<string, unknown>;
}

export interface UpstreamServerRuntimeDto {
  status: "idle" | "connecting" | "connected" | "error" | "disabled" | string;
  lastError: string | null;
  lastLatencyMs: number | null;
  lastUsedMs: number | null;
  syncedAtMs: number | null;
  protocolVersion: string | null;
  serverInfoName: string | null;
  serverInfoVersion: string | null;
  stderrTail: string[];
}

export interface UpstreamServerOverviewDto {
  config: UpstreamServerConfig;
  runtime: UpstreamServerRuntimeDto;
  totalToolCount: number;
  enabledToolCount: number;
  tools: UpstreamToolItemDto[];
}

export interface UpstreamMcpOverviewDto {
  enabled: boolean;
  alwaysExposeProxyTools: boolean;
  builtinToolCount: number;
  directExposedToolCount: number;
  proxyMetaToolCount: number;
  totalExposedToolCount: number;
  directToolsWarningThreshold: number;
  servers: UpstreamServerOverviewDto[];
}

export interface UpstreamTestSyncResultDto {
  serverId: string;
  alias: string;
  ok: boolean;
  latencyMs: number;
  protocolVersion: string;
  serverName: string;
  serverVersion: string;
  toolCount: number;
  error: string | null;
  stderrTail: string[];
}

export interface ImportedUpstreamCandidate {
  candidateId: string;
  sourceClient: string;
  sourceLabel: string;
  sourcePath: string;
  originalKey: string;
  suggestedAlias: string;
  name: string;
  transportKind: "stdio" | "streamable_http" | "sse" | string;
  summaryTarget: string;
  config: UpstreamServerConfig;
  secretEnvKeys: string[];
  secretHeaderKeys: string[];
  hasBearerToken: boolean;
  alreadyExistsAlias: boolean;
  blockedReason: string | null;
}

export interface ImportSelectionItem {
  candidateId: string;
  aliasOverride?: string;
  exposeMode?: UpstreamExposeMode;
  enabled?: boolean;
}

export type TakeoverConnectMode = "auto" | "http" | "stdio";

export interface HarnessTakeoverPlanItem {
  sourceClient: string;
  sourceLabel: string;
  displayPath: string;
  effectiveMode: "http" | "stdio";
  removedKeys: string[];
  injectedKey: string;
  alreadyHasCodingTools: boolean;
  entryPreview: string;
  warning: string | null;
}

export interface HarnessTakeoverBackupRecord {
  id: string;
  createdAtMs: number;
  sourceClient: string;
  sourceLabel: string;
  targetPath: string;
  displayPath: string;
  backupFile: string;
  removedKeys: string[];
  injectedKey: string;
  connectMode: "http" | "stdio" | string;
  restoredAtMs?: number | null;
}

export interface HarnessTakeoverAppliedDto {
  backupId: string;
  sourceClient: string;
  sourceLabel: string;
  displayPath: string;
  effectiveMode: "http" | "stdio";
  removedKeys: string[];
  injectedKey: string;
}

export interface UpstreamImportResultDto {
  importedCount: number;
  skipped: string[];
  replacedFiles: HarnessTakeoverAppliedDto[];
  replaceErrors: string[];
  overview: UpstreamMcpOverviewDto;
}

export function getUpstreamMcpOverview(): Promise<UpstreamMcpOverviewDto> {
  return invoke<UpstreamMcpOverviewDto>("get_upstream_mcp_overview");
}

export function setUpstreamMcpGlobalSettings(
  enabled: boolean,
  alwaysExposeProxyTools: boolean,
): Promise<UpstreamMcpOverviewDto> {
  return invoke<UpstreamMcpOverviewDto>("set_upstream_mcp_global_settings", {
    enabled,
    alwaysExposeProxyTools,
  });
}

export function saveUpstreamMcpServer(
  server: UpstreamServerConfig,
  secretPatch?: UpstreamSecretPatch,
): Promise<UpstreamMcpOverviewDto> {
  return invoke<UpstreamMcpOverviewDto>("save_upstream_mcp_server", {
    server,
    secretPatch: secretPatch ?? null,
  });
}

export function deleteUpstreamMcpServer(serverId: string): Promise<UpstreamMcpOverviewDto> {
  return invoke<UpstreamMcpOverviewDto>("delete_upstream_mcp_server", { serverId });
}

export function setUpstreamMcpServerEnabled(
  serverId: string,
  enabled: boolean,
): Promise<UpstreamMcpOverviewDto> {
  return invoke<UpstreamMcpOverviewDto>("set_upstream_mcp_server_enabled", {
    serverId,
    enabled,
  });
}

export function setUpstreamMcpServerMode(
  serverId: string,
  exposeMode: UpstreamExposeMode,
): Promise<UpstreamMcpOverviewDto> {
  return invoke<UpstreamMcpOverviewDto>("set_upstream_mcp_server_mode", {
    serverId,
    exposeMode,
  });
}

export function updateUpstreamMcpDisabledTools(
  serverId: string,
  disabledTools: string[],
): Promise<UpstreamMcpOverviewDto> {
  return invoke<UpstreamMcpOverviewDto>("update_upstream_mcp_disabled_tools", {
    serverId,
    disabledTools,
  });
}

export function testAndSyncUpstreamMcpServer(serverId: string): Promise<UpstreamTestSyncResultDto> {
  return invoke<UpstreamTestSyncResultDto>("test_and_sync_upstream_mcp_server", { serverId });
}

export function syncAllUpstreamMcpServers(): Promise<UpstreamTestSyncResultDto[]> {
  return invoke<UpstreamTestSyncResultDto[]>("sync_all_upstream_mcp_servers");
}

export function scanLocalUpstreamMcpClients(): Promise<ImportedUpstreamCandidate[]> {
  return invoke<ImportedUpstreamCandidate[]>("scan_local_upstream_mcp_clients");
}

export function parseUpstreamMcpJson(rawJson: string): Promise<ImportedUpstreamCandidate[]> {
  return invoke<ImportedUpstreamCandidate[]>("parse_upstream_mcp_json", { rawJson });
}

export function previewUpstreamMcpTakeover(
  selections: ImportSelectionItem[],
  connectMode: TakeoverConnectMode = "auto",
): Promise<HarnessTakeoverPlanItem[]> {
  return invoke<HarnessTakeoverPlanItem[]>("preview_upstream_mcp_takeover", {
    selections,
    connectMode,
  });
}

export function listUpstreamMcpTakeoverBackups(): Promise<HarnessTakeoverBackupRecord[]> {
  return invoke<HarnessTakeoverBackupRecord[]>("list_upstream_mcp_takeover_backups");
}

export function restoreUpstreamMcpTakeoverBackup(
  backupId: string,
): Promise<HarnessTakeoverBackupRecord> {
  return invoke<HarnessTakeoverBackupRecord>("restore_upstream_mcp_takeover_backup", {
    backupId,
  });
}

export function importUpstreamMcpCandidates(
  rawJson: string | null,
  selections: ImportSelectionItem[],
  options?: {
    replaceSourceConfigs?: boolean;
    connectMode?: TakeoverConnectMode;
  },
): Promise<UpstreamImportResultDto> {
  return invoke<UpstreamImportResultDto>("import_upstream_mcp_candidates", {
    rawJson,
    selections,
    replaceSourceConfigs: options?.replaceSourceConfigs ?? false,
    connectMode: options?.connectMode ?? "auto",
  });
}
