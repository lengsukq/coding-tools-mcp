import type { PlanningStateDto } from "$lib/api/planning";
import type { WorkspaceGitSummaryDto } from "$lib/api/workspaces";
import type { WorkspaceProfile } from "$lib/types";
import type { taskFocus } from "$lib/ui-model";

export interface DashboardWorkspaceRow {
  workspace: WorkspaceProfile;
  state: PlanningStateDto | null;
  git: WorkspaceGitSummaryDto | null;
  query: {
    value: unknown;
    updatedAt: number;
    loading: boolean;
    error: string | null;
    stale: boolean;
  };
  focus: ReturnType<typeof taskFocus>;
}

export interface DashboardAttentionItem {
  key: string;
  title: string;
  detail: string;
  path: string;
  query?: Record<string, string>;
}
