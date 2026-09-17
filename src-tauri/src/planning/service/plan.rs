use crate::error::{AppError, AppResult};

use super::super::model::{Plan, PlanStatus, PlanStep, PlanStepStatus, PlanningState};
use super::super::store::PlanningStore;
use super::util::{new_id, non_empty, required_text, timestamp};
use super::UpdatePlanRequest;

pub(super) fn create(
    store: &PlanningStore,
    goal_id: Option<String>,
    title: &str,
    objective: &str,
    steps: Vec<String>,
) -> AppResult<Plan> {
    let title = required_text(title, "Plan title")?;
    let objective = required_text(objective, "Plan objective")?;
    store.update(|state| {
        if let Some(id) = goal_id.as_deref() {
            if !state.goals.iter().any(|goal| goal.id == id) {
                return Err(AppError::Message(format!("goal not found: {id}")));
            }
        }
        let now = timestamp();
        let plan = Plan {
            id: new_id(),
            goal_id: goal_id.clone(),
            title,
            objective,
            status: PlanStatus::Draft,
            steps: steps
                .into_iter()
                .filter_map(non_empty)
                .map(|title| PlanStep {
                    id: new_id(),
                    title,
                    status: PlanStepStatus::Pending,
                    notes: None,
                })
                .collect(),
            task_ids: Vec::new(),
            revision: 1,
            created_at: now.clone(),
            updated_at: now,
            archived_at: None,
            review_requested_at: None,
            review_summary: None,
            review_feedback: None,
            extra: Default::default(),
        };
        if let Some(id) = goal_id.as_deref() {
            if let Some(goal) = state.goals.iter_mut().find(|goal| goal.id == id) {
                goal.plan_ids.push(plan.id.clone());
                goal.updated_at = timestamp();
            }
        }
        state.plans.push(plan.clone());
        Ok(plan)
    })
}

pub(super) fn delete(store: &PlanningStore, plan_id: &str) -> AppResult<PlanningState> {
    store.update(|state| {
        let index = state
            .plans
            .iter()
            .position(|plan| plan.id == plan_id)
            .ok_or_else(|| AppError::Message(format!("plan not found: {plan_id}")))?;
        let removed = state.plans.remove(index);
        let removed_step_ids = removed
            .steps
            .iter()
            .map(|step| step.id.as_str())
            .collect::<Vec<_>>();

        for goal in &mut state.goals {
            let linked = goal.plan_ids.iter().any(|id| id == plan_id);
            goal.plan_ids.retain(|id| id != plan_id);

            let mut checkpoint_changed = false;
            if let Some(checkpoint) = goal.execution_checkpoint.as_mut() {
                if checkpoint
                    .current_step_id
                    .as_deref()
                    .is_some_and(|id| removed_step_ids.contains(&id))
                {
                    checkpoint.current_step_id = None;
                    checkpoint_changed = true;
                }
                let before = checkpoint.completed_step_ids.len();
                checkpoint
                    .completed_step_ids
                    .retain(|id| !removed_step_ids.contains(&id.as_str()));
                if checkpoint.completed_step_ids.len() != before {
                    checkpoint_changed = true;
                }
                if checkpoint_changed {
                    checkpoint.updated_at = timestamp();
                }
            }

            if linked || checkpoint_changed {
                goal.updated_at = timestamp();
            }
        }

        if state.focus_plan_id.as_deref() == Some(plan_id) {
            state.focus_plan_id = None;
        }

        let execution_references_plan = state.execution.plan_id.as_deref() == Some(plan_id);
        let execution_references_step = state
            .execution
            .step_id
            .as_deref()
            .is_some_and(|id| removed_step_ids.contains(&id));
        if execution_references_plan || execution_references_step {
            state.execution.plan_id = None;
            state.execution.step_id = None;
            if state
                .execution
                .task_id
                .as_deref()
                .is_some_and(|id| removed.task_ids.iter().any(|task_id| task_id == id))
            {
                state.execution.task_id = None;
            }
            state.execution.updated_at = timestamp();
        }

        Ok(state.clone())
    })
}

pub(super) fn update(store: &PlanningStore, request: UpdatePlanRequest) -> AppResult<Plan> {
    let UpdatePlanRequest {
        plan_id,
        status,
        step_updates,
        focus,
    } = request;
    store.update(|state| {
        let plan = state
            .plans
            .iter_mut()
            .find(|plan| plan.id == plan_id)
            .ok_or_else(|| AppError::Message(format!("plan not found: {plan_id}")))?;
        if let Some(value) = status {
            plan.status = value;
        }
        for (step_id, step_status, notes) in step_updates {
            let step = plan
                .steps
                .iter_mut()
                .find(|step| step.id == step_id)
                .ok_or_else(|| AppError::Message(format!("plan step not found: {step_id}")))?;
            step.status = step_status;
            if notes.is_some() {
                step.notes = notes;
            }
        }
        plan.revision = plan.revision.saturating_add(1);
        plan.updated_at = timestamp();
        let output = plan.clone();
        if focus == Some(true) {
            state.focus_plan_id = Some(plan_id.clone());
            if let Some(goal_id) = output.goal_id.as_ref() {
                state.focus_goal_id = Some(goal_id.clone());
            }
        } else if focus == Some(false) && state.focus_plan_id.as_deref() == Some(plan_id.as_str()) {
            state.focus_plan_id = None;
        }
        Ok(output)
    })
}
