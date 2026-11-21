use std::path::PathBuf;

use tauri::State;

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::planning::{Goal, Plan, PlanningMode, PlanningService, PlanningState};

async fn run_planning_blocking<T, F>(
    state: &AppState,
    workspace_id: &str,
    label: &'static str,
    f: F,
) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(PathBuf, PlanningService) -> AppResult<T> + Send + 'static,
{
    let raw_path = state.workspace_path(workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let canonical = raw_path.canonicalize().map_err(AppError::from)?;
        let service = PlanningService::new(&canonical);
        f(canonical, service)
    })
    .await
    .map_err(|err| AppError::Message(format!("{label}任务失败: {err}")))?
}

#[tauri::command]
pub async fn get_planning_state(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<PlanningState> {
    run_planning_blocking(&state, &workspace_id, "读取规划状态", |_, service| {
        service.state()
    })
    .await
}

#[tauri::command]
pub async fn reset_planning_state(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<PlanningState> {
    run_planning_blocking(&state, &workspace_id, "重置规划状态", |_, service| {
        service.reset_state()
    })
    .await
}

#[tauri::command]
pub async fn delete_plan(
    state: State<'_, AppState>,
    workspace_id: String,
    plan_id: String,
) -> AppResult<PlanningState> {
    run_planning_blocking(&state, &workspace_id, "删除计划", move |_, service| {
        service.delete_plan(&plan_id)
    })
    .await
}

#[tauri::command]
pub async fn set_planning_mode(
    state: State<'_, AppState>,
    workspace_id: String,
    mode: PlanningMode,
) -> AppResult<PlanningState> {
    run_planning_blocking(
        &state,
        &workspace_id,
        "切换规划模式",
        move |path, service| {
            let planning = service.set_mode(mode)?;
            if mode == PlanningMode::Plan {
                crate::tools::session::kill_workspace_sessions(&path);
            }
            Ok(planning)
        },
    )
    .await
}

#[tauri::command]
pub async fn accept_goal_review(
    state: State<'_, AppState>,
    workspace_id: String,
    goal_id: String,
) -> AppResult<Goal> {
    run_planning_blocking(&state, &workspace_id, "验收目标", move |_, service| {
        service.accept_goal_review(&goal_id)
    })
    .await
}

#[tauri::command]
pub async fn reject_goal_review(
    state: State<'_, AppState>,
    workspace_id: String,
    goal_id: String,
    feedback: Option<String>,
) -> AppResult<Goal> {
    run_planning_blocking(
        &state,
        &workspace_id,
        "退回目标验收",
        move |_, service| service.reject_goal_review(&goal_id, feedback),
    )
    .await
}

#[tauri::command]
pub async fn accept_plan_review(
    state: State<'_, AppState>,
    workspace_id: String,
    plan_id: String,
) -> AppResult<Plan> {
    run_planning_blocking(&state, &workspace_id, "验收计划", move |_, service| {
        service.accept_plan_review(&plan_id)
    })
    .await
}

#[tauri::command]
pub async fn reject_plan_review(
    state: State<'_, AppState>,
    workspace_id: String,
    plan_id: String,
    feedback: Option<String>,
) -> AppResult<Plan> {
    run_planning_blocking(
        &state,
        &workspace_id,
        "退回计划验收",
        move |_, service| service.reject_plan_review(&plan_id, feedback),
    )
    .await
}
