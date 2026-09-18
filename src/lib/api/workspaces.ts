import { invoke } from "@tauri-apps/api/core";
import type { RuntimeState, RuntimeStatus, WorkspaceProfile } from "$lib/types";

export async function listWorkspaces(): Promise<WorkspaceProfile[]> {
  return invoke<WorkspaceProfile[]>("list_workspaces");
}
export async function issueWorkspaceReviewUrl(workspaceId: string, reviewId: string): Promise<string> {
  return invoke<string>("issue_workspace_review_url", { workspaceId, reviewId });
}
export interface ReviewLaunchDto {
  changeId: string;
  url: string;
  scope: "workspace" | "session" | string;
  files: number;
  additions: number;
  deletions: number;
}
export async function createWorkspaceReviewUrl(workspaceId: string): Promise<ReviewLaunchDto | null> {
  return invoke<ReviewLaunchDto | null>("create_workspace_review_url", { workspaceId });
}
export async function createSessionReviewUrl(workspaceId: string, sessionId: string): Promise<ReviewLaunchDto | null> {
  return invoke<ReviewLaunchDto | null>("create_session_review_url", { workspaceId, sessionId });
}
export async function deleteWorkspaceReview(workspaceId: string, reviewId: string): Promise<void> {
  return invoke("delete_workspace_review", { workspaceId, reviewId });
}

export interface WorkspaceReviewSummaryDto {
  id: string; createdAt: number; expiresAt: number; summary: string; files: number;
  additions: number; deletions: number; operationIds: string[];
  storageBytes: number;
  sessionId?: string | null;
  scope: "operation" | "workspace" | "session" | string;
}
export async function listWorkspaceReviews(workspaceId: string): Promise<WorkspaceReviewSummaryDto[]> {
  return invoke<WorkspaceReviewSummaryDto[]>("list_workspace_reviews", { workspaceId });
}

export interface WorkspaceActivityEventDto {
  createdAt: number;
  operations: number;
  files: number;
  additions: number;
  deletions: number;
}
export interface WorkspaceActivityMetricsDto {
  events: WorkspaceActivityEventDto[];
  totalOperations: number;
  totalFiles: number;
  totalAdditions: number;
  totalDeletions: number;
}
export async function getWorkspaceActivityMetrics(workspaceId: string): Promise<WorkspaceActivityMetricsDto> {
  return invoke<WorkspaceActivityMetricsDto>("get_workspace_activity_metrics", { workspaceId });
}

export async function createWorkspace(
  path: string,
  name?: string,
): Promise<WorkspaceProfile> {
  return invoke<WorkspaceProfile>("create_workspace", { path, name });
}

export async function updateWorkspace(profile: WorkspaceProfile): Promise<void> {
  return invoke("update_workspace", { profile });
}

export async function openWorkspaceDirectory(path: string): Promise<void> {
  return invoke("open_workspace_directory", { path });
}

export interface WorkspaceGitSummaryDto {
  available: boolean;
  branch: string | null;
  changedFiles: number;
  ahead: number;
  behind: number;
  lastCommit: string | null;
  subRepositories: WorkspaceSubGitSummaryDto[];
}

export interface WorkspaceSubGitSummaryDto {
  name: string;
  path: string;
  branch: string | null;
  changedFiles: number;
  ahead: number;
  behind: number;
  lastCommit: string | null;
}

export async function getWorkspaceGitSummary(workspaceId: string): Promise<WorkspaceGitSummaryDto> {
  return invoke<WorkspaceGitSummaryDto>("get_workspace_git_summary", { workspaceId });
}

export interface DetectedIdeDto {
  id: string;
  name: string;
}

export async function detectInstalledIdes(): Promise<DetectedIdeDto[]> {
  return invoke<DetectedIdeDto[]>("detect_installed_ides");
}

export async function openWorkspaceInIde(workspaceId: string, ideId: string): Promise<void> {
  return invoke("open_workspace_in_ide", { workspaceId, ideId });
}

export async function deleteWorkspace(id: string): Promise<void> {
  return invoke("delete_workspace", { id });
}

export async function startRuntime(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("start_runtime");
}

export async function stopRuntime(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("stop_runtime");
}

export async function getRuntimeStatus(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("get_runtime_status");
}

export interface GlobalMcpSessionDto {
  sessionId: string;
  workspaceId: string;
  workspaceName: string;
  lastSeenAt: number;
}

export interface GlobalMcpOverviewDto {
  state: RuntimeState;
  localEndpoint: string;
  publicEndpoint: string;
  workspaceCount: number;
  sessionCount: number;
  registryRevision: number;
  sessions: GlobalMcpSessionDto[];
}

export async function getGlobalMcpOverview(): Promise<GlobalMcpOverviewDto> {
  return invoke<GlobalMcpOverviewDto>("get_global_mcp_overview");
}

export async function restartRuntime(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("restart_runtime");
}

export async function restoreRuntimeState(): Promise<void> {
  return invoke("restore_runtime_state");
}
