use crate::error::{AppError, AppResult};

use super::super::model::{Goal, GoalStatus, Plan, PlanStatus};
use super::super::store::PlanningStore;
use super::util::{non_empty, required_text, timestamp};

pub(super) fn request_goal(store: &PlanningStore, goal_id: &str, summary: &str) -> AppResult<Goal> {
    let summary = required_text(summary, "Review summary")?;
    store.update(|state| {
        let goal = state
            .goals
            .iter_mut()
            .find(|goal| goal.id == goal_id)
            .ok_or_else(|| AppError::Message(format!("goal not found: {goal_id}")))?;
        if matches!(goal.status, GoalStatus::Archived | GoalStatus::Cancelled) {
            return Err(AppError::Message(
                "Archived or cancelled goals cannot be submitted for review".into(),
            ));
        }
        let now = timestamp();
        goal.status = GoalStatus::AwaitingAcceptance;
        goal.review_requested_at = Some(now.clone());
        goal.review_summary = Some(summary);
        goal.review_feedback = None;
        goal.updated_at = now;
        Ok(goal.clone())
    })
}

pub(super) fn request_plan(store: &PlanningStore, plan_id: &str, summary: &str) -> AppResult<Plan> {
    let summary = required_text(summary, "Review summary")?;
    store.update(|state| {
        let plan = state
            .plans
            .iter_mut()
            .find(|plan| plan.id == plan_id)
            .ok_or_else(|| AppError::Message(format!("plan not found: {plan_id}")))?;
        if matches!(plan.status, PlanStatus::Archived | PlanStatus::Cancelled) {
            return Err(AppError::Message(
                "Archived or cancelled plans cannot be submitted for review".into(),
            ));
        }
        let now = timestamp();
        plan.status = PlanStatus::AwaitingAcceptance;
        plan.review_requested_at = Some(now.clone());
        plan.review_summary = Some(summary);
        plan.review_feedback = None;
        plan.updated_at = now;
        plan.revision = plan.revision.saturating_add(1);
        Ok(plan.clone())
    })
}

pub(super) fn accept_goal(store: &PlanningStore, goal_id: &str) -> AppResult<Goal> {
    store.update(|state| {
        let goal_index = state
            .goals
            .iter()
            .position(|goal| goal.id == goal_id)
            .ok_or_else(|| AppError::Message(format!("goal not found: {goal_id}")))?;
        if state.goals[goal_index].status != GoalStatus::AwaitingAcceptance {
            return Err(AppError::Message(
                "Goal is not waiting for human acceptance".into(),
            ));
        }
        let now = timestamp();
        let linked_plan_ids = state.goals[goal_index].plan_ids.clone();
        {
            let goal = &mut state.goals[goal_index];
            goal.status = GoalStatus::Archived;
            goal.archived_at = Some(now.clone());
            goal.review_feedback = None;
            goal.updated_at = now.clone();
        }
        for plan in &mut state.plans {
            if linked_plan_ids.iter().any(|id| id == &plan.id)
                && !matches!(plan.status, PlanStatus::Archived | PlanStatus::Cancelled)
            {
                plan.status = PlanStatus::Archived;
                plan.archived_at = Some(now.clone());
                plan.review_feedback = None;
                plan.updated_at = now.clone();
                plan.revision = plan.revision.saturating_add(1);
            }
        }
        if state.focus_goal_id.as_deref() == Some(goal_id) {
            state.focus_goal_id = None;
            if state
                .focus_plan_id
                .as_ref()
                .is_some_and(|id| linked_plan_ids.iter().any(|plan_id| plan_id == id))
            {
                state.focus_plan_id = None;
            }
        }
        Ok(state.goals[goal_index].clone())
    })
}

pub(super) fn reject_goal(
    store: &PlanningStore,
    goal_id: &str,
    feedback: Option<String>,
) -> AppResult<Goal> {
    store.update(|state| {
        let goal_index = state
            .goals
            .iter()
            .position(|goal| goal.id == goal_id)
            .ok_or_else(|| AppError::Message(format!("goal not found: {goal_id}")))?;
        if state.goals[goal_index].status != GoalStatus::AwaitingAcceptance {
            return Err(AppError::Message(
                "Goal is not waiting for human acceptance".into(),
            ));
        }
        let linked_plan_ids = state.goals[goal_index].plan_ids.clone();
        let now = timestamp();
        {
            let goal = &mut state.goals[goal_index];
            goal.status = GoalStatus::Active;
            goal.review_requested_at = None;
            goal.review_feedback = feedback.and_then(non_empty);
            goal.updated_at = now.clone();
        }
        for plan in &mut state.plans {
            if linked_plan_ids.iter().any(|id| id == &plan.id)
                && plan.status == PlanStatus::AwaitingAcceptance
            {
                plan.status = PlanStatus::Active;
                plan.review_requested_at = None;
                plan.updated_at = now.clone();
                plan.revision = plan.revision.saturating_add(1);
            }
        }
        state.focus_goal_id = Some(goal_id.to_string());
        Ok(state.goals[goal_index].clone())
    })
}

pub(super) fn accept_plan(store: &PlanningStore, plan_id: &str) -> AppResult<Plan> {
    store.update(|state| {
        let plan = state
            .plans
            .iter_mut()
            .find(|plan| plan.id == plan_id)
            .ok_or_else(|| AppError::Message(format!("plan not found: {plan_id}")))?;
        if plan.status != PlanStatus::AwaitingAcceptance {
            return Err(AppError::Message(
                "Plan is not waiting for human acceptance".into(),
            ));
        }
        let now = timestamp();
        plan.status = PlanStatus::Archived;
        plan.archived_at = Some(now.clone());
        plan.review_feedback = None;
        plan.updated_at = now;
        plan.revision = plan.revision.saturating_add(1);
        let output = plan.clone();
        if state.focus_plan_id.as_deref() == Some(plan_id) {
            state.focus_plan_id = None;
        }
        Ok(output)
    })
}

pub(super) fn reject_plan(
    store: &PlanningStore,
    plan_id: &str,
    feedback: Option<String>,
) -> AppResult<Plan> {
    store.update(|state| {
        let plan = state
            .plans
            .iter_mut()
            .find(|plan| plan.id == plan_id)
            .ok_or_else(|| AppError::Message(format!("plan not found: {plan_id}")))?;
        if plan.status != PlanStatus::AwaitingAcceptance {
            return Err(AppError::Message(
                "Plan is not waiting for human acceptance".into(),
            ));
        }
        let now = timestamp();
        plan.status = PlanStatus::Active;
        plan.review_requested_at = None;
        plan.review_feedback = feedback.and_then(non_empty);
        plan.updated_at = now;
        plan.revision = plan.revision.saturating_add(1);
        let output = plan.clone();
        state.focus_plan_id = Some(plan_id.to_string());
        if let Some(goal_id) = output.goal_id.as_ref() {
            state.focus_goal_id = Some(goal_id.clone());
        }
        Ok(output)
    })
}
