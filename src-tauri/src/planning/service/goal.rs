use crate::error::{AppError, AppResult};

use super::super::model::{Goal, GoalStatus, SuccessCriterion};
use super::super::store::PlanningStore;
use super::util::{new_id, non_empty, required_text, timestamp};
use super::UpdateGoalRequest;

pub(super) fn create(
    store: &PlanningStore,
    title: &str,
    objective: &str,
    success_criteria: Vec<String>,
    constraints: Vec<String>,
) -> AppResult<Goal> {
    let title = required_text(title, "Goal title")?;
    let objective = required_text(objective, "Goal objective")?;
    store.update(|state| {
        let now = timestamp();
        let goal = Goal {
            id: new_id(),
            title,
            objective,
            status: GoalStatus::Active,
            success_criteria: success_criteria
                .into_iter()
                .filter_map(non_empty)
                .map(|text| SuccessCriterion {
                    id: new_id(),
                    text,
                    completed: false,
                })
                .collect(),
            constraints: constraints.into_iter().filter_map(non_empty).collect(),
            plan_ids: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            archived_at: None,
            review_requested_at: None,
            review_summary: None,
            review_feedback: None,
            execution_checkpoint: None,
            extra: Default::default(),
        };
        if state.focus_goal_id.is_none() {
            state.focus_goal_id = Some(goal.id.clone());
        }
        state.goals.push(goal.clone());
        Ok(goal)
    })
}

pub(super) fn update(store: &PlanningStore, request: UpdateGoalRequest) -> AppResult<Goal> {
    let UpdateGoalRequest {
        goal_id,
        title,
        objective,
        status,
        constraints,
        completed_criteria_ids,
        focus,
    } = request;
    store.update(|state| {
        let goal = state
            .goals
            .iter_mut()
            .find(|goal| goal.id == goal_id)
            .ok_or_else(|| AppError::Message(format!("goal not found: {goal_id}")))?;
        if let Some(value) = title {
            goal.title = required_text(&value, "Goal title")?;
        }
        if let Some(value) = objective {
            goal.objective = required_text(&value, "Goal objective")?;
        }
        if let Some(value) = status {
            goal.status = value;
        }
        if let Some(values) = constraints {
            goal.constraints = values.into_iter().filter_map(non_empty).collect();
        }
        if let Some(ids) = completed_criteria_ids {
            for criterion in &mut goal.success_criteria {
                criterion.completed = ids.iter().any(|id| id == &criterion.id);
            }
        }
        goal.updated_at = timestamp();
        let output = goal.clone();
        if focus == Some(true) {
            state.focus_goal_id = Some(goal_id.clone());
        } else if focus == Some(false) && state.focus_goal_id.as_deref() == Some(goal_id.as_str()) {
            state.focus_goal_id = None;
        }
        Ok(output)
    })
}
