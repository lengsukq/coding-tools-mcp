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
export type ProposalStatus = "pending_approval" | "approved" | "rejected";

export interface SuccessCriterionDto {
  id: string;
  text: string;
  completed: boolean;
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

export interface PlanningProposalDto {
  id: string;
  source_request: string;
  title: string;
  objective: string;
  success_criteria: string[];
  constraints: string[];
  plan_steps: string[];
  approval_status: ProposalStatus;
  created_at: string;
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
  proposals: PlanningProposalDto[];
  goals: GoalDto[];
  plans: PlanDto[];
}

export interface GoalUpdate {
  title?: string;
  objective?: string;
  status?: GoalStatus;
  constraints?: string[];
  completedCriteriaIds?: string[];
  focus?: boolean;
}

export interface PlanStepUpdate {
  step_id: string;
  status: PlanStepStatus;
  notes?: string;
}

export async function getPlanningState(workspaceId: string): Promise<PlanningStateDto> {
  return invoke<PlanningStateDto>("get_planning_state", { workspaceId });
}

export async function setPlanningMode(
  workspaceId: string,
  mode: PlanningMode,
): Promise<PlanningStateDto> {
  return invoke<PlanningStateDto>("set_planning_mode", { workspaceId, mode });
}

export async function createGoal(
  workspaceId: string,
  title: string,
  objective: string,
  successCriteria: string[],
  constraints: string[],
): Promise<GoalDto> {
  return invoke<GoalDto>("create_goal", {
    workspaceId,
    title,
    objective,
    successCriteria,
    constraints,
  });
}

export async function updateGoal(
  workspaceId: string,
  goalId: string,
  update: GoalUpdate,
): Promise<GoalDto> {
  return invoke<GoalDto>("update_goal", {
    workspaceId,
    goalId,
    title: update.title ?? null,
    objective: update.objective ?? null,
    status: update.status ?? null,
    constraints: update.constraints ?? null,
    completedCriteriaIds: update.completedCriteriaIds ?? null,
    focus: update.focus ?? null,
  });
}

export async function createPlan(
  workspaceId: string,
  goalId: string | null,
  title: string,
  objective: string,
  steps: string[],
): Promise<PlanDto> {
  return invoke<PlanDto>("create_plan", { workspaceId, goalId, title, objective, steps });
}

export async function updatePlan(
  workspaceId: string,
  planId: string,
  update: { status?: PlanStatus; stepUpdates?: PlanStepUpdate[]; focus?: boolean },
): Promise<PlanDto> {
  return invoke<PlanDto>("update_plan", {
    workspaceId,
    planId,
    status: update.status ?? null,
    stepUpdates: update.stepUpdates ?? [],
    focus: update.focus ?? null,
  });
}
