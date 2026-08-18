import { invoke } from "@tauri-apps/api/core";

export type UsageService = "mcp" | "actions";

export interface ServiceUsageStats {
  workspaceId: string;
  service: UsageService;
  requestCount: number;
  toolCallCount: number;
  errorCount: number;
  inputBytes: number;
  outputBytes: number;
  estimatedInputTokens: number;
  estimatedOutputTokens: number;
  estimatedTokens: number;
}

export function getServiceUsageStats(id: string): Promise<ServiceUsageStats[]> {
  return invoke<ServiceUsageStats[]>("get_service_usage_stats", { id });
}
