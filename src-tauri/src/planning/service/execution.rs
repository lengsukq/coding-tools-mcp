use crate::error::AppResult;

use super::super::model::{ExecutionCheckpoint, PlanStepStatus, PlanningState};
use super::super::store::PlanningStore;
use super::util::timestamp;
use super::ExecutionLedgerUpdate;

pub(super) fn record(
    store: &PlanningStore,
    update: ExecutionLedgerUpdate,
) -> AppResult<PlanningState> {
    store.update(|state| {
        let focused_plan = state
            .focus_plan_id
            .as_deref()
            .and_then(|id| state.plans.iter().find(|plan| plan.id == id));
        let current_step_id = focused_plan.and_then(|plan| {
            plan.steps
                .iter()
                .find(|step| step.status == PlanStepStatus::InProgress)
                .or_else(|| {
                    plan.steps
                        .iter()
                        .find(|step| step.status == PlanStepStatus::Pending)
                })
                .map(|step| step.id.clone())
        });
        let completed_step_ids = focused_plan
            .map(|plan| {
                plan.steps
                    .iter()
                    .filter(|step| step.status == PlanStepStatus::Completed)
                    .map(|step| step.id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        state.execution.goal_id = state.focus_goal_id.clone();
        state.execution.plan_id = state.focus_plan_id.clone();
        state.execution.step_id = current_step_id.clone();
        if update.task_id.is_some() {
            state.execution.task_id = update.task_id;
        }
        if update.last_tool.is_some() {
            state.execution.last_tool = update.last_tool;
        }
        if let Some(value) = update.state {
            state.execution.state = value;
        }
        state.execution.last_error = update.last_error.clone();
        if !update.changed_files.is_empty() {
            state.execution.changed_files = update.changed_files;
        }
        if update.history_checkpoint_ref.is_some() {
            state.execution.history_checkpoint_ref = update.history_checkpoint_ref;
        }
        if !update.verification.is_empty() {
            state.execution.verification = update.verification;
        }
        state.execution.updated_at = timestamp();

        if let Some(goal_id) = state.focus_goal_id.as_deref() {
            if let Some(goal) = state.goals.iter_mut().find(|goal| goal.id == goal_id) {
                goal.execution_checkpoint = Some(ExecutionCheckpoint {
                    current_step_id,
                    completed_step_ids,
                    last_error: update.last_error,
                    updated_at: state.execution.updated_at.clone(),
                });
                goal.updated_at = state.execution.updated_at.clone();
            }
        }
        Ok(state.clone())
    })
}
