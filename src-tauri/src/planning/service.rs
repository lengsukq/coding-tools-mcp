use std::path::Path;

use crate::error::AppResult;

use super::model::{
    Goal, GoalStatus, Plan, PlanStatus, PlanStepStatus, PlanningMode, PlanningState,
};
use super::store::PlanningStore;

mod execution;
mod goal;
mod plan;
mod review;
mod util;

#[derive(Debug, Clone)]
pub struct PlanningService {
    store: PlanningStore,
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionLedgerUpdate {
    pub task_id: Option<String>,
    pub last_tool: Option<String>,
    pub state: Option<String>,
    pub last_error: Option<String>,
    pub changed_files: Vec<String>,
    pub history_checkpoint_ref: Option<String>,
    pub verification: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateGoalRequest {
    pub goal_id: String,
    pub title: Option<String>,
    pub objective: Option<String>,
    pub status: Option<GoalStatus>,
    pub constraints: Option<Vec<String>>,
    pub completed_criteria_ids: Option<Vec<String>>,
    pub focus: Option<bool>,
}

impl UpdateGoalRequest {
    pub fn focus(goal_id: impl Into<String>, focus: bool) -> Self {
        Self {
            goal_id: goal_id.into(),
            title: None,
            objective: None,
            status: None,
            constraints: None,
            completed_criteria_ids: None,
            focus: Some(focus),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdatePlanRequest {
    pub plan_id: String,
    pub status: Option<PlanStatus>,
    pub step_updates: Vec<(String, PlanStepStatus, Option<String>)>,
    pub focus: Option<bool>,
}

impl UpdatePlanRequest {
    pub fn activate(plan_id: impl Into<String>) -> Self {
        Self {
            plan_id: plan_id.into(),
            status: Some(PlanStatus::Active),
            step_updates: Vec::new(),
            focus: Some(true),
        }
    }
}

impl PlanningService {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            store: PlanningStore::new(workspace_root),
        }
    }

    /// Replace the project-local planning file with a clean default state.
    /// This intentionally does not call `load()` first, so it remains usable
    /// even when an older or manually edited state file can no longer deserialize.
    pub fn reset_state(&self) -> AppResult<PlanningState> {
        self.store.reset()
    }

    pub fn request_goal_review(&self, goal_id: &str, summary: &str) -> AppResult<Goal> {
        review::request_goal(&self.store, goal_id, summary)
    }

    pub fn request_plan_review(&self, plan_id: &str, summary: &str) -> AppResult<Plan> {
        review::request_plan(&self.store, plan_id, summary)
    }

    pub fn accept_goal_review(&self, goal_id: &str) -> AppResult<Goal> {
        review::accept_goal(&self.store, goal_id)
    }

    pub fn reject_goal_review(&self, goal_id: &str, feedback: Option<String>) -> AppResult<Goal> {
        review::reject_goal(&self.store, goal_id, feedback)
    }

    pub fn accept_plan_review(&self, plan_id: &str) -> AppResult<Plan> {
        review::accept_plan(&self.store, plan_id)
    }

    pub fn reject_plan_review(&self, plan_id: &str, feedback: Option<String>) -> AppResult<Plan> {
        review::reject_plan(&self.store, plan_id, feedback)
    }

    pub fn state(&self) -> AppResult<PlanningState> {
        self.store.load()
    }

    pub fn record_execution(&self, update: ExecutionLedgerUpdate) -> AppResult<PlanningState> {
        execution::record(&self.store, update)
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
        goal::create(&self.store, title, objective, success_criteria, constraints)
    }

    pub fn update_goal(&self, request: UpdateGoalRequest) -> AppResult<Goal> {
        goal::update(&self.store, request)
    }

    pub fn create_plan(
        &self,
        goal_id: Option<String>,
        title: &str,
        objective: &str,
        steps: Vec<String>,
    ) -> AppResult<Plan> {
        plan::create(&self.store, goal_id, title, objective, steps)
    }

    pub fn update_plan(&self, request: UpdatePlanRequest) -> AppResult<Plan> {
        plan::update(&self.store, request)
    }

    pub fn delete_plan(&self, plan_id: &str) -> AppResult<PlanningState> {
        plan::delete(&self.store, plan_id)
    }
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

        assert!(workspace
            .path()
            .join(super::super::PLANNING_RELATIVE_PATH)
            .exists());
        let reloaded = PlanningService::new(workspace.path())
            .state()
            .expect("reload");
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
            .update_plan(UpdatePlanRequest {
                plan_id: plan.id.clone(),
                status: Some(PlanStatus::Active),
                step_updates: vec![(step_id, PlanStepStatus::Completed, None)],
                focus: Some(true),
            })
            .expect("update");

        assert_eq!(updated.status, PlanStatus::Active);
        assert_eq!(updated.steps[0].status, PlanStepStatus::Completed);
        assert_eq!(service.state().expect("state").focus_plan_id, Some(plan.id));
    }

    #[test]
    fn execution_ledger_converges_goal_plan_task_and_history_state() {
        let workspace = tempdir().expect("workspace");
        let service = PlanningService::new(workspace.path());
        let goal = service
            .create_goal("Goal", "Objective", Vec::new(), Vec::new())
            .expect("goal");
        service
            .update_goal(UpdateGoalRequest::focus(&goal.id, true))
            .expect("focus goal");
        let plan = service
            .create_plan(
                Some(goal.id.clone()),
                "Plan",
                "Objective",
                vec!["Step".into()],
            )
            .expect("plan");
        let step_id = plan.steps[0].id.clone();
        service
            .update_plan(UpdatePlanRequest {
                plan_id: plan.id.clone(),
                status: Some(PlanStatus::Active),
                step_updates: vec![(step_id.clone(), PlanStepStatus::InProgress, None)],
                focus: Some(true),
            })
            .expect("focus plan");

        let state = service
            .record_execution(ExecutionLedgerUpdate {
                task_id: Some("task-1".into()),
                last_tool: Some("history_manage:checkpoint".into()),
                state: Some("completed".into()),
                history_checkpoint_ref: Some("docs/history-session/6.md".into()),
                verification: vec!["cargo test".into()],
                ..ExecutionLedgerUpdate::default()
            })
            .expect("record execution");

        assert_eq!(state.execution.goal_id.as_deref(), Some(goal.id.as_str()));
        assert_eq!(state.execution.plan_id.as_deref(), Some(plan.id.as_str()));
        assert_eq!(state.execution.step_id.as_deref(), Some(step_id.as_str()));
        assert_eq!(state.execution.task_id.as_deref(), Some("task-1"));
        assert_eq!(
            state.execution.history_checkpoint_ref.as_deref(),
            Some("docs/history-session/6.md")
        );
        assert_eq!(
            state.goals[0]
                .execution_checkpoint
                .as_ref()
                .unwrap()
                .current_step_id
                .as_deref(),
            Some(step_id.as_str())
        );
    }

    #[test]
    fn ai_review_request_waits_for_human_acceptance_and_archives_after_acceptance() {
        let workspace = tempdir().expect("workspace");
        let service = PlanningService::new(workspace.path());
        let goal = service
            .create_goal(
                "AI goal",
                "Finish the requested work",
                Vec::new(),
                Vec::new(),
            )
            .expect("goal");
        service
            .update_goal(UpdateGoalRequest::focus(&goal.id, true))
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
            .update_plan(UpdatePlanRequest::activate(&plan.id))
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
            state
                .plans
                .iter()
                .find(|item| item.id == plan.id)
                .unwrap()
                .status,
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
        assert_eq!(
            rejected.review_feedback.as_deref(),
            Some("Add one more regression test")
        );
        assert_eq!(state.focus_goal_id.as_deref(), Some(goal.id.as_str()));
    }

    #[test]
    fn deleting_plan_cleans_focus_goal_links_and_execution_references() {
        let workspace = tempdir().expect("workspace");
        let service = PlanningService::new(workspace.path());
        let goal = service
            .create_goal("Goal", "Objective", Vec::new(), Vec::new())
            .expect("goal");
        service
            .update_goal(UpdateGoalRequest::focus(&goal.id, true))
            .expect("focus goal");
        let plan = service
            .create_plan(
                Some(goal.id.clone()),
                "Disposable plan",
                "Temporary work",
                vec!["First step".into()],
            )
            .expect("plan");
        let step_id = plan.steps[0].id.clone();
        service
            .update_plan(UpdatePlanRequest {
                plan_id: plan.id.clone(),
                status: Some(PlanStatus::Active),
                step_updates: vec![(step_id.clone(), PlanStepStatus::InProgress, None)],
                focus: Some(true),
            })
            .expect("focus plan");
        service
            .record_execution(ExecutionLedgerUpdate {
                state: Some("running".into()),
                ..ExecutionLedgerUpdate::default()
            })
            .expect("execution");

        let state = service.delete_plan(&plan.id).expect("delete plan");

        assert!(state.plans.is_empty());
        assert!(state.goals[0].plan_ids.is_empty());
        assert_eq!(state.focus_plan_id, None);
        assert_eq!(state.execution.plan_id, None);
        assert_eq!(state.execution.step_id, None);
        assert_eq!(state.focus_goal_id.as_deref(), Some(goal.id.as_str()));
    }
}
