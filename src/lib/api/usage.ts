import { invoke } from "@tauri-apps/api/core";

export type UsageService = "mcp";

export interface ServiceUsageStats {
  workspaceId: string;
  service: UsageService;
  requestCount: number;
  toolCallCount: number;
  errorCount: number;
  inputBytes: number;
  outputBytes: number;
  toolCallInputBytes: number;
  toolCallOutputBytes: number;
  estimatedInputTokens: number;
  estimatedOutputTokens: number;
  estimatedTokens: number;
  estimatedToolCallInputTokens: number;
  estimatedToolCallOutputTokens: number;
  estimatedToolCallTokens: number;
}

export function getServiceUsageStats(id: string): Promise<ServiceUsageStats[]> {
  return invoke<ServiceUsageStats[]>("get_service_usage_stats", { id });
}
