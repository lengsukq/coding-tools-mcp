import { ref } from "vue";
import type { RuntimeState, WorkspaceProfile } from "$lib/types";

export const workspaces = ref<WorkspaceProfile[]>([]);
export const globalMcpRuntimeState = ref<RuntimeState>("stopped");
