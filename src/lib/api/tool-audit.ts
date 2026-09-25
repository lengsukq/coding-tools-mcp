import { invoke } from "@tauri-apps/api/core";

export interface ToolAuditRecord {
  timestampMs: number;
  durationMs: number;
  requestId: string | null;
  sessionId: string | null;
  workspaceId: string | null;
  toolName: string;
  wrapperToolName: string | null;
  authMethod: "oauth" | "shared_bearer" | "none" | string;
  clientId: string | null;
  outcome: "success" | "error" | string;
  errorCategory: string | null;
  requestBytes: number;
  responseBytes: number;
  changeId: string | null;
  changedFileCount: number | null;
}

export interface ToolAuditFilter {
  workspaceId?: string | null;
  fromMs?: number | null;
  toMs?: number | null;
  sessionId?: string | null;
  toolName?: string | null;
  outcome?: string | null;
  search?: string | null;
  offset?: number;
  limit?: number;
}

export interface ToolAuditPage {
  records: ToolAuditRecord[];
  total: number;
  errorCount: number;
  workspaceCount: number;
  offset: number;
  limit: number;
  retentionDays: number;
  healthMessage: string | null;
}

export async function listToolAuditRecords(filter: ToolAuditFilter = {}): Promise<ToolAuditPage> {
  return invoke<ToolAuditPage>("list_tool_audit_records", { filter });
}

export async function setToolAuditRetentionDays(days: number): Promise<number> {
  return invoke<number>("set_tool_audit_retention_days", { days });
}

export async function clearToolAuditRecords(workspaceId?: string): Promise<void> {
  return invoke<void>("clear_tool_audit_records", { workspaceId: workspaceId ?? null });
}
