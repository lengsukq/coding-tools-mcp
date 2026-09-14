import { invoke } from "@tauri-apps/api/core";

export type PlanningMode = "direct" | "plan" | "goal";
export type GoalStatus =
  | "active"
  | "paused"
  | "completed"
  | "awaiting_acceptance"
  | "archived"
  | "cancelled";
export type PlanStatus =
  | "draft"
  | "active"
  | "paused"
  | "completed"
  | "awaiting_acceptance"
  | "archived"
  | "cancelled";
export type PlanStepStatus = "pending" | "in_progress" | "completed" | "blocked" | "skipped";
export interface SuccessCriterionDto {
  id: string;
  text: string;
  completed: boolean;
}

export interface ExecutionCheckpointDto {
  current_step_id: string | null;
  completed_step_ids: string[];
  last_error: string | null;
  updated_at: string;
}

export interface ExecutionLedgerDto {
  goal_id: string | null;
  plan_id: string | null;
  step_id: string | null;
  task_id: string | null;
  last_tool: string | null;
  state: string;
  last_error: string | null;
  changed_files: string[];
  history_checkpoint_ref: string | null;
  verification: string[];
  updated_at: string;
}

export async function acceptGoalReview(
  workspaceId: string,
  goalId: string,
): Promise<GoalDto> {
  return invoke<GoalDto>("accept_goal_review", { workspaceId, goalId });
}

export async function rejectGoalReview(
  workspaceId: string,
  goalId: string,
  feedback?: string,
): Promise<GoalDto> {
  return invoke<GoalDto>("reject_goal_review", {
    workspaceId,
    goalId,
    feedback: feedback?.trim() || null,
  });
}

export async function acceptPlanReview(
  workspaceId: string,
  planId: string,
): Promise<PlanDto> {
  return invoke<PlanDto>("accept_plan_review", { workspaceId, planId });
}

export async function rejectPlanReview(
  workspaceId: string,
  planId: string,
  feedback?: string,
): Promise<PlanDto> {
  return invoke<PlanDto>("reject_plan_review", {
    workspaceId,
    planId,
    feedback: feedback?.trim() || null,
  });
}

export interface GoalDto {
  id: string;
  title: string;
  objective: string;
  status: GoalStatus;
  success_criteria: SuccessCriterionDto[];
  constraints: string[];
  plan_ids: string[];
  created_at: string;
  updated_at: string;
  archived_at: string | null;
  review_requested_at: string | null;
  review_summary: string | null;
  review_feedback: string | null;
  execution_checkpoint: ExecutionCheckpointDto | null;
}

export interface PlanStepDto {
  id: string;
  title: string;
  status: PlanStepStatus;
  notes: string | null;
}

export interface PlanDto {
  id: string;
  goal_id: string | null;
  title: string;
  objective: string;
  status: PlanStatus;
  steps: PlanStepDto[];
  task_ids: string[];
  revision: number;
  created_at: string;
  updated_at: string;
  archived_at: string | null;
  review_requested_at: string | null;
  review_summary: string | null;
  review_feedback: string | null;
}

export interface PlanningStateDto {
  schema_version: number;
  revision: number;
  mode: PlanningMode;
  focus_goal_id: string | null;
  focus_plan_id: string | null;
  goals: GoalDto[];
  plans: PlanDto[];
  execution: ExecutionLedgerDto;
}

export async function getPlanningState(workspaceId: string): Promise<PlanningStateDto> {
  return invoke<PlanningStateDto>("get_planning_state", { workspaceId });
}

export async function resetPlanningState(workspaceId: string): Promise<PlanningStateDto> {
  return invoke<PlanningStateDto>("reset_planning_state", { workspaceId });
}

export async function setPlanningMode(
  workspaceId: string,
  mode: PlanningMode,
): Promise<PlanningStateDto> {
  return invoke<PlanningStateDto>("set_planning_mode", { workspaceId, mode });
}
