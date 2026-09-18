import { invoke } from "@tauri-apps/api/core";

export interface HealthItem {
  label: string;
  ok: boolean;
  detail: string;
  hint: string;
}

export async function runGlobalHealthChecks(): Promise<HealthItem[]> {
  return invoke<HealthItem[]>("run_global_health_checks");
}
