use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, AppResult};

use super::model::{
    ExecutionCheckpoint, Goal, GoalStatus, Plan, PlanStatus, PlanStep, PlanStepStatus,
    PlanningMode, PlanningProposal, PlanningState, ProposalStatus, SuccessCriterion,
};
use super::store::PlanningStore;

#[derive(Debug, Clone)]
pub struct PlanningService {
    store: PlanningStore,
}

impl PlanningService {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            store: PlanningStore::new(workspace_root),
        }
    }

    pub fn request_goal_review(&self, goal_id: &str, summary: &str) -> AppResult<Goal> {
        let summary = required_text(summary, "Review summary")?;
        self.store.update(|state| {
            let goal = state
                .goals
                .iter_mut()
                .find(|goal| goal.id == goal_id)
                .ok_or_else(|| AppError::Message(format!("goal not found: {goal_id}")))?;
            if matches!(goal.status, GoalStatus::Archived | GoalStatus::Cancelled) {
                return Err(AppError::Message("Archived or cancelled goals cannot be submitted for review".into()));
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

    pub fn request_plan_review(&self, plan_id: &str, summary: &str) -> AppResult<Plan> {
        let summary = required_text(summary, "Review summary")?;
        self.store.update(|state| {
            let plan = state
                .plans
                .iter_mut()
                .find(|plan| plan.id == plan_id)
                .ok_or_else(|| AppError::Message(format!("plan not found: {plan_id}")))?;
            if matches!(plan.status, PlanStatus::Archived | PlanStatus::Cancelled) {
                return Err(AppError::Message("Archived or cancelled plans cannot be submitted for review".into()));
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

    pub fn accept_goal_review(&self, goal_id: &str) -> AppResult<Goal> {
        self.store.update(|state| {
            let goal_index = state
                .goals
                .iter()
                .position(|goal| goal.id == goal_id)
                .ok_or_else(|| AppError::Message(format!("goal not found: {goal_id}")))?;
            if state.goals[goal_index].status != GoalStatus::AwaitingAcceptance {
                return Err(AppError::Message("Goal is not waiting for human acceptance".into()));
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

    pub fn reject_goal_review(&self, goal_id: &str, feedback: Option<String>) -> AppResult<Goal> {
        self.store.update(|state| {
            let goal_index = state
                .goals
                .iter()
                .position(|goal| goal.id == goal_id)
                .ok_or_else(|| AppError::Message(format!("goal not found: {goal_id}")))?;
            if state.goals[goal_index].status != GoalStatus::AwaitingAcceptance {
                return Err(AppError::Message("Goal is not waiting for human acceptance".into()));
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

    pub fn accept_plan_review(&self, plan_id: &str) -> AppResult<Plan> {
        self.store.update(|state| {
            let plan = state
                .plans
                .iter_mut()
                .find(|plan| plan.id == plan_id)
                .ok_or_else(|| AppError::Message(format!("plan not found: {plan_id}")))?;
            if plan.status != PlanStatus::AwaitingAcceptance {
                return Err(AppError::Message("Plan is not waiting for human acceptance".into()));
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

    pub fn reject_plan_review(&self, plan_id: &str, feedback: Option<String>) -> AppResult<Plan> {
        self.store.update(|state| {
            let plan = state
                .plans
                .iter_mut()
                .find(|plan| plan.id == plan_id)
                .ok_or_else(|| AppError::Message(format!("plan not found: {plan_id}")))?;
            if plan.status != PlanStatus::AwaitingAcceptance {
                return Err(AppError::Message("Plan is not waiting for human acceptance".into()));
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

    pub fn reject_proposal(&self, proposal_id: &str) -> AppResult<PlanningProposal> {
        self.store.update(|state| {
            let proposal = state
                .proposals
                .iter_mut()
                .find(|proposal| proposal.id == proposal_id)
                .ok_or_else(|| AppError::Message(format!("proposal not found: {proposal_id}")))?;
            proposal.approval_status = ProposalStatus::Rejected;
            Ok(proposal.clone())
        })
    }

    pub fn pending_proposals(&self) -> AppResult<Vec<PlanningProposal>> {
        let state = self.store.load()?;
        Ok(state
            .proposals
            .into_iter()
            .filter(|proposal| proposal.approval_status == ProposalStatus::PendingApproval)
            .collect())
    }

    pub fn state(&self) -> AppResult<PlanningState> {
        self.store.load()
    }

    pub fn create_proposal(
        &self,
        source_request: &str,
        title: &str,
        objective: &str,
        success_criteria: Vec<String>,
        constraints: Vec<String>,
        plan_steps: Vec<String>,
    ) -> AppResult<PlanningProposal> {
        let source_request = required_text(source_request, "Proposal request")?;
        let title = required_text(title, "Proposal title")?;
        let objective = required_text(objective, "Proposal objective")?;
        self.store.update(|state| {
            let proposal = PlanningProposal {
                id: new_id(),
                source_request,
                title,
                objective,
                success_criteria,
                constraints,
                plan_steps,
                approval_status: ProposalStatus::PendingApproval,
                created_at: timestamp(),
            };
            state.proposals.push(proposal.clone());
            Ok(proposal)
        })
    }

    pub fn approve_proposal(&self, proposal_id: &str) -> AppResult<(Goal, Plan)> {
        self.store.update(|state| {
            let proposal = state
                .proposals
                .iter_mut()
                .find(|proposal| proposal.id == proposal_id)
                .ok_or_else(|| AppError::Message(format!("proposal not found: {proposal_id}")))?;
            proposal.approval_status = ProposalStatus::Approved;

            let now = timestamp();
            let goal = Goal {
                id: new_id(),
                title: proposal.title.clone(),
                objective: proposal.objective.clone(),
                status: GoalStatus::Active,
                success_criteria: proposal
                    .success_criteria
                    .iter()
                    .cloned()
                    .map(|text| SuccessCriterion { id: new_id(), text, completed: false })
                    .collect(),
                constraints: proposal.constraints.clone(),
                plan_ids: Vec::new(),
                created_at: now.clone(),
                updated_at: now.clone(),
                archived_at: None,
                review_requested_at: None,
                review_summary: None,
                review_feedback: None,
                execution_checkpoint: None,
            };
            let plan = Plan {
                id: new_id(),
                goal_id: Some(goal.id.clone()),
                title: proposal.title.clone(),
                objective: proposal.objective.clone(),
                status: PlanStatus::Active,
                steps: proposal.plan_steps.iter().cloned().map(|title| PlanStep {
                    id: new_id(),
                    title,
                    status: PlanStepStatus::Pending,
                    notes: None,
                }).collect(),
                task_ids: Vec::new(),
                revision: 1,
                created_at: now.clone(),
                updated_at: now,
                archived_at: None,
                review_requested_at: None,
                review_summary: None,
                review_feedback: None,
            };
            state.focus_goal_id = Some(goal.id.clone());
            state.focus_plan_id = Some(plan.id.clone());
            state.goals.push(goal.clone());
            state.plans.push(plan.clone());
            Ok((goal, plan))
        })
    }

    pub fn set_mode(&self, mode: PlanningMode) -> AppResult<PlanningState> {
        self.store.update(|state| {
            state.mode = mode;
            Ok(state.clone())
        })
    }

    pub fn create_goal(
        &self,
        title: &str,
        objective: &str,
        success_criteria: Vec<String>,
        constraints: Vec<String>,
    ) -> AppResult<Goal> {
        let title = required_text(title, "Goal title")?;
        let objective = required_text(objective, "Goal objective")?;
        self.store.update(|state| {
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
            };
            if state.focus_goal_id.is_none() {
                state.focus_goal_id = Some(goal.id.clone());
            }
            state.goals.push(goal.clone());
            Ok(goal)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_goal(
        &self,
        goal_id: &str,
        title: Option<String>,
        objective: Option<String>,
        status: Option<GoalStatus>,
        constraints: Option<Vec<String>>,
        completed_criteria_ids: Option<Vec<String>>,
        focus: Option<bool>,
    ) -> AppResult<Goal> {
        self.store.update(|state| {
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
                state.focus_goal_id = Some(goal_id.to_string());
            } else if focus == Some(false) && state.focus_goal_id.as_deref() == Some(goal_id) {
                state.focus_goal_id = None;
            }
            Ok(output)
        })
    }

    pub fn create_plan(
        &self,
        goal_id: Option<String>,
        title: &str,
        objective: &str,
        steps: Vec<String>,
    ) -> AppResult<Plan> {
        let title = required_text(title, "Plan title")?;
        let objective = required_text(objective, "Plan objective")?;
        self.store.update(|state| {
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

    pub fn update_plan(
        &self,
        plan_id: &str,
        status: Option<PlanStatus>,
        step_updates: Vec<(String, PlanStepStatus, Option<String>)>,
        focus: Option<bool>,
    ) -> AppResult<Plan> {
        self.store.update(|state| {
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
                state.focus_plan_id = Some(plan_id.to_string());
                if let Some(goal_id) = output.goal_id.as_ref() {
                    state.focus_goal_id = Some(goal_id.clone());
                }
            } else if focus == Some(false) && state.focus_plan_id.as_deref() == Some(plan_id) {
                state.focus_plan_id = None;
            }
            Ok(output)
        })
    }

    pub fn update_execution_checkpoint(
        &self,
        goal_id: &str,
        current_step_id: Option<String>,
        completed_step_ids: Vec<String>,
        last_error: Option<String>,
    ) -> AppResult<Goal> {
        self.store.update(|state| {
            let goal = state
                .goals
                .iter_mut()
                .find(|goal| goal.id == goal_id)
                .ok_or_else(|| AppError::Message(format!("goal not found: {goal_id}")))?;
            goal.execution_checkpoint = Some(ExecutionCheckpoint {
                current_step_id,
                completed_step_ids,
                last_error,
                updated_at: timestamp(),
            });
            goal.updated_at = timestamp();
            Ok(goal.clone())
        })
    }

    pub fn resume_context(&self) -> AppResult<Option<(Goal, Plan)>> {
        let state = self.store.load()?;
        let Some(goal_id) = state.focus_goal_id else {
            return Ok(None);
        };
        let Some(goal) = state.goals.iter().find(|goal| goal.id == goal_id) else {
            return Ok(None);
        };
        let plan = state
            .focus_plan_id
            .as_deref()
            .and_then(|id| state.plans.iter().find(|plan| plan.id == id));
        Ok(plan.cloned().map(|plan| (goal.clone(), plan)))
    }
}

fn required_text(value: &str, label: &str) -> AppResult<String> {
    non_empty(value.to_string()).ok_or_else(|| AppError::Message(format!("{label} cannot be empty")))
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn planning_state_is_stored_inside_workspace() {
        let workspace = tempdir().expect("workspace");
        let service = PlanningService::new(workspace.path());

        let goal = service
            .create_goal(
                "Ship Goal mode",
                "Persist Goal state with the project",
                vec!["Goal survives restart".into()],
                vec!["Do not use app data".into()],
            )
            .expect("create goal");
        let plan = service
            .create_plan(
                Some(goal.id.clone()),
                "First plan",
                "Implement project-local planning",
                vec!["Model".into(), "Store".into()],
            )
            .expect("create plan");

        assert!(workspace.path().join(super::super::PLANNING_RELATIVE_PATH).exists());
        let reloaded = PlanningService::new(workspace.path()).state().expect("reload");
        assert_eq!(reloaded.goals.len(), 1);
        assert_eq!(reloaded.plans.len(), 1);
        assert_eq!(reloaded.goals[0].plan_ids, vec![plan.id]);
    }

    #[test]
    fn updating_plan_progress_preserves_goal_link() {
        let workspace = tempdir().expect("workspace");
        let service = PlanningService::new(workspace.path());
        let goal = service
            .create_goal("Goal", "Objective", Vec::new(), Vec::new())
            .expect("goal");
        let plan = service
            .create_plan(Some(goal.id), "Plan", "Objective", vec!["Step".into()])
            .expect("plan");
        let step_id = plan.steps[0].id.clone();

        let updated = service
            .update_plan(
                &plan.id,
                Some(PlanStatus::Active),
                vec![(step_id, PlanStepStatus::Completed, None)],
                Some(true),
            )
            .expect("update");

        assert_eq!(updated.status, PlanStatus::Active);
        assert_eq!(updated.steps[0].status, PlanStepStatus::Completed);
        assert_eq!(service.state().expect("state").focus_plan_id, Some(plan.id));
    }

    #[test]
    fn ai_review_request_waits_for_human_acceptance_and_archives_after_acceptance() {
        let workspace = tempdir().expect("workspace");
        let service = PlanningService::new(workspace.path());
        let goal = service
            .create_goal("AI goal", "Finish the requested work", Vec::new(), Vec::new())
            .expect("goal");
        service
            .update_goal(&goal.id, None, None, None, None, None, Some(true))
            .expect("focus goal");
        let plan = service
            .create_plan(
                Some(goal.id.clone()),
                "AI plan",
                "Execute the implementation",
                vec!["Implement".into(), "Verify".into()],
            )
            .expect("plan");
        service
            .update_plan(&plan.id, Some(PlanStatus::Active), Vec::new(), Some(true))
            .expect("focus plan");

        let reviewed_plan = service
            .request_plan_review(&plan.id, "Implementation and verification completed")
            .expect("request plan review");
        let reviewed_goal = service
            .request_goal_review(&goal.id, "All requested outcomes are ready for acceptance")
            .expect("request goal review");
        assert_eq!(reviewed_plan.status, PlanStatus::AwaitingAcceptance);
        assert_eq!(reviewed_goal.status, GoalStatus::AwaitingAcceptance);

        let archived_goal = service.accept_goal_review(&goal.id).expect("accept goal");
        let state = service.state().expect("state");
        assert_eq!(archived_goal.status, GoalStatus::Archived);
        assert!(archived_goal.archived_at.is_some());
        assert_eq!(state.focus_goal_id, None);
        assert_eq!(state.focus_plan_id, None);
        assert_eq!(
            state.plans.iter().find(|item| item.id == plan.id).unwrap().status,
            PlanStatus::Archived
        );
    }

    #[test]
    fn rejected_review_reactivates_work_and_preserves_human_feedback() {
        let workspace = tempdir().expect("workspace");
        let service = PlanningService::new(workspace.path());
        let goal = service
            .create_goal("Review me", "Need human acceptance", Vec::new(), Vec::new())
            .expect("goal");
        service
            .request_goal_review(&goal.id, "Ready for review")
            .expect("request review");

        let rejected = service
            .reject_goal_review(&goal.id, Some("Add one more regression test".into()))
            .expect("reject review");
        let state = service.state().expect("state");
        assert_eq!(rejected.status, GoalStatus::Active);
        assert_eq!(rejected.review_feedback.as_deref(), Some("Add one more regression test"));
        assert_eq!(state.focus_goal_id.as_deref(), Some(goal.id.as_str()));
    }
}
