import { ref } from "vue";
import type { RuntimeState, WorkspaceProfile } from "$lib/types";

export const workspaces = ref<WorkspaceProfile[]>([]);
export const mcpRuntimeStates = ref<Record<string, RuntimeState>>({});

/** @deprecated use mcpRuntimeStates */
export const runtimeStates = mcpRuntimeStates;

export function overallRuntimeState(mcp: RuntimeState | undefined): RuntimeState {
  return mcp ?? "stopped";
}
