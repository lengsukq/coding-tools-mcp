use serde::{Deserialize, Serialize};

pub const PLANNING_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningMode {
    #[default]
    Direct,
    Plan,
    Goal,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    #[default]
    PendingApproval,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningProposal {
    pub id: String,
    pub source_request: String,
    pub title: String,
    pub objective: String,
    #[serde(default)]
    pub success_criteria: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub plan_steps: Vec<String>,
    #[serde(default)]
    pub approval_status: ProposalStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    #[default]
    Active,
    Paused,
    Completed,
    AwaitingAcceptance,
    Archived,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    #[default]
    Draft,
    Active,
    Paused,
    Completed,
    AwaitingAcceptance,
    Archived,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Blocked,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriterion {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub status: GoalStatus,
    #[serde(default)]
    pub success_criteria: Vec<SuccessCriterion>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub plan_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub archived_at: Option<String>,
    #[serde(default)]
    pub review_requested_at: Option<String>,
    #[serde(default)]
    pub review_summary: Option<String>,
    #[serde(default)]
    pub review_feedback: Option<String>,
    #[serde(default)]
    pub execution_checkpoint: Option<ExecutionCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCheckpoint {
    pub current_step_id: Option<String>,
    #[serde(default)]
    pub completed_step_ids: Vec<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionLedger {
    pub goal_id: Option<String>,
    pub plan_id: Option<String>,
    pub step_id: Option<String>,
    pub task_id: Option<String>,
    pub last_tool: Option<String>,
    #[serde(default)]
    pub state: String,
    pub last_error: Option<String>,
    #[serde(default)]
    pub changed_files: Vec<String>,
    pub history_checkpoint_ref: Option<String>,
    #[serde(default)]
    pub verification: Vec<String>,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub status: PlanStepStatus,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub goal_id: Option<String>,
    pub title: String,
    pub objective: String,
    pub status: PlanStatus,
    #[serde(default)]
    pub steps: Vec<PlanStep>,
    #[serde(default)]
    pub task_ids: Vec<String>,
    pub revision: u32,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub archived_at: Option<String>,
    #[serde(default)]
    pub review_requested_at: Option<String>,
    #[serde(default)]
    pub review_summary: Option<String>,
    #[serde(default)]
    pub review_feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningState {
    pub schema_version: u32,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub mode: PlanningMode,
    pub focus_goal_id: Option<String>,
    pub focus_plan_id: Option<String>,
    #[serde(default)]
    pub proposals: Vec<PlanningProposal>,
    #[serde(default)]
    pub goals: Vec<Goal>,
    #[serde(default)]
    pub plans: Vec<Plan>,
    #[serde(default)]
    pub execution: ExecutionLedger,
}

impl Default for PlanningState {
    fn default() -> Self {
        Self {
            schema_version: PLANNING_SCHEMA_VERSION,
            revision: 0,
            mode: PlanningMode::Direct,
            focus_goal_id: None,
            focus_plan_id: None,
            proposals: Vec::new(),
            goals: Vec::new(),
            plans: Vec::new(),
            execution: ExecutionLedger::default(),
        }
    }
}
