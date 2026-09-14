use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PLANNING_SCHEMA_VERSION: u32 = 2;

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

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum SuccessCriterionCompat {
    Structured(SuccessCriterion),
    LegacyText(String),
}

fn deserialize_success_criteria<'de, D>(deserializer: D) -> Result<Vec<SuccessCriterion>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values = Vec::<SuccessCriterionCompat>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .enumerate()
        .map(|(index, value)| match value {
            SuccessCriterionCompat::Structured(value) => value,
            SuccessCriterionCompat::LegacyText(text) => SuccessCriterion {
                // Legacy files never had criterion ids. Keep the generated id
                // deterministic until the normalized state is saved again.
                id: format!("legacy-criterion-{}", index + 1),
                text,
                completed: false,
            },
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub status: GoalStatus,
    #[serde(default, deserialize_with = "deserialize_success_criteria")]
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
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
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
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PlanStepCompat {
    Structured(PlanStep),
    LegacyTitle(String),
}

fn deserialize_plan_steps<'de, D>(deserializer: D) -> Result<Vec<PlanStep>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values = Vec::<PlanStepCompat>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .enumerate()
        .map(|(index, value)| match value {
            PlanStepCompat::Structured(value) => value,
            PlanStepCompat::LegacyTitle(title) => PlanStep {
                id: format!("legacy-step-{}", index + 1),
                title,
                status: PlanStepStatus::Pending,
                notes: None,
            },
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub goal_id: Option<String>,
    pub title: String,
    pub objective: String,
    pub status: PlanStatus,
    #[serde(default, deserialize_with = "deserialize_plan_steps")]
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
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
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
            goals: Vec::new(),
            plans: Vec::new(),
            execution: ExecutionLedger::default(),
        }
    }
}
