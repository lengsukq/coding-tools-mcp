use std::path::PathBuf;

use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::planning::{
    Goal, GoalStatus, Plan, PlanStatus, PlanStepStatus, PlanningMode, PlanningService,
    PlanningState,
};

#[tauri::command]
pub fn get_planning_state(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<PlanningState> {
    service_for_workspace(&state, &workspace_id)?.state()
}

#[tauri::command]
pub fn set_planning_mode(
    state: State<'_, AppState>,
    workspace_id: String,
    mode: PlanningMode,
) -> AppResult<PlanningState> {
    let path = workspace_path(&state, &workspace_id)?;
    let planning = PlanningService::new(&path).set_mode(mode)?;
    if mode == PlanningMode::Plan {
        crate::tools::session::kill_workspace_sessions(&path);
    }
    Ok(planning)
}

#[tauri::command]
pub fn create_goal(
    state: State<'_, AppState>,
    workspace_id: String,
    title: String,
    objective: String,
    success_criteria: Vec<String>,
    constraints: Vec<String>,
) -> AppResult<Goal> {
    service_for_workspace(&state, &workspace_id)?.create_goal(
        &title,
        &objective,
        success_criteria,
        constraints,
    )
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_goal(
    state: State<'_, AppState>,
    workspace_id: String,
    goal_id: String,
    title: Option<String>,
    objective: Option<String>,
    status: Option<GoalStatus>,
    constraints: Option<Vec<String>>,
    completed_criteria_ids: Option<Vec<String>>,
    focus: Option<bool>,
) -> AppResult<Goal> {
    service_for_workspace(&state, &workspace_id)?.update_goal(
        &goal_id,
        title,
        objective,
        status,
        constraints,
        completed_criteria_ids,
        focus,
    )
}

#[tauri::command]
pub fn create_plan(
    state: State<'_, AppState>,
    workspace_id: String,
    goal_id: Option<String>,
    title: String,
    objective: String,
    steps: Vec<String>,
) -> AppResult<Plan> {
    service_for_workspace(&state, &workspace_id)?.create_plan(
        goal_id,
        &title,
        &objective,
        steps,
    )
}

#[tauri::command]
pub fn update_plan(
    state: State<'_, AppState>,
    workspace_id: String,
    plan_id: String,
    status: Option<PlanStatus>,
    step_updates: Vec<PlanStepUpdate>,
    focus: Option<bool>,
) -> AppResult<Plan> {
    let updates = step_updates
        .into_iter()
        .map(|update| (update.step_id, update.status, update.notes))
        .collect();
    service_for_workspace(&state, &workspace_id)?.update_plan(
        &plan_id,
        status,
        updates,
        focus,
    )
}

#[tauri::command]
pub fn accept_goal_review(
    state: State<'_, AppState>,
    workspace_id: String,
    goal_id: String,
) -> AppResult<Goal> {
    service_for_workspace(&state, &workspace_id)?.accept_goal_review(&goal_id)
}

#[tauri::command]
pub fn reject_goal_review(
    state: State<'_, AppState>,
    workspace_id: String,
    goal_id: String,
    feedback: Option<String>,
) -> AppResult<Goal> {
    service_for_workspace(&state, &workspace_id)?.reject_goal_review(&goal_id, feedback)
}

#[tauri::command]
pub fn accept_plan_review(
    state: State<'_, AppState>,
    workspace_id: String,
    plan_id: String,
) -> AppResult<Plan> {
    service_for_workspace(&state, &workspace_id)?.accept_plan_review(&plan_id)
}

#[tauri::command]
pub fn reject_plan_review(
    state: State<'_, AppState>,
    workspace_id: String,
    plan_id: String,
    feedback: Option<String>,
) -> AppResult<Plan> {
    service_for_workspace(&state, &workspace_id)?.reject_plan_review(&plan_id, feedback)
}

#[derive(Debug, Deserialize)]
pub struct PlanStepUpdate {
    pub step_id: String,
    pub status: PlanStepStatus,
    #[serde(default)]
    pub notes: Option<String>,
}

fn service_for_workspace(state: &AppState, workspace_id: &str) -> AppResult<PlanningService> {
    Ok(PlanningService::new(&workspace_path(state, workspace_id)?))
}

fn workspace_path(state: &AppState, workspace_id: &str) -> AppResult<PathBuf> {
    let path = state.with_workspaces(|store| {
        store
            .get(workspace_id)
            .map(|profile| PathBuf::from(&profile.path))
            .ok_or_else(|| AppError::Message(format!("workspace not found: {workspace_id}")))
    })?;
    path.canonicalize().map_err(AppError::from)
}
