import { writable } from "svelte/store";
import type { RuntimeState, WorkspaceProfile } from "$lib/types";

export const workspaces = writable<WorkspaceProfile[]>([]);
export const mcpRuntimeStates = writable<Record<string, RuntimeState>>({});

/** @deprecated use mcpRuntimeStates */
export const runtimeStates = mcpRuntimeStates;

export function overallRuntimeState(mcp: RuntimeState | undefined): RuntimeState {
  return mcp ?? "stopped";
}
