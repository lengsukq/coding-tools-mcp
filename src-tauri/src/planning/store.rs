use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppResult;

use super::{PlanningState, PLANNING_RELATIVE_PATH, PLANNING_SCHEMA_VERSION};

fn normalize_legacy_state(value: &mut serde_json::Value) {
    let fallback_updated_at = value
        .get("execution")
        .and_then(|execution| execution.get("updated_at"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();

    let Some(root) = value.as_object_mut() else {
        return;
    };
    root.insert(
        "schema_version".into(),
        serde_json::Value::from(PLANNING_SCHEMA_VERSION),
    );

    if let Some(goals) = root.get_mut("goals").and_then(serde_json::Value::as_array_mut) {
        for goal in goals {
            let Some(goal) = goal.as_object_mut() else {
                continue;
            };
            if !goal.contains_key("objective") {
                let objective = goal
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| goal.get("title").and_then(serde_json::Value::as_str))
                    .unwrap_or("")
                    .to_string();
                goal.insert("objective".into(), serde_json::Value::String(objective));
            }
            goal.entry("created_at")
                .or_insert_with(|| serde_json::Value::String(fallback_updated_at.clone()));
            goal.entry("updated_at")
                .or_insert_with(|| serde_json::Value::String(fallback_updated_at.clone()));
            goal.entry("plan_ids")
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        }
    }

    let mut goal_plan_links = Vec::<(String, String)>::new();
    if let Some(plans) = root.get_mut("plans").and_then(serde_json::Value::as_array_mut) {
        for (plan_index, plan) in plans.iter_mut().enumerate() {
            let Some(plan) = plan.as_object_mut() else {
                continue;
            };
            if !plan.contains_key("objective") {
                let objective = plan
                    .get("strategy")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| plan.get("title").and_then(serde_json::Value::as_str))
                    .unwrap_or("")
                    .to_string();
                plan.insert("objective".into(), serde_json::Value::String(objective));
            }
            plan.entry("revision")
                .or_insert_with(|| serde_json::Value::from(1_u64));
            plan.entry("task_ids")
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));
            plan.entry("created_at")
                .or_insert_with(|| serde_json::Value::String(fallback_updated_at.clone()));
            plan.entry("updated_at")
                .or_insert_with(|| serde_json::Value::String(fallback_updated_at.clone()));

            let needs_steps = plan
                .get("steps")
                .and_then(serde_json::Value::as_array)
                .map(|steps| steps.is_empty())
                .unwrap_or(true);
            if needs_steps {
                let mut steps = Vec::new();
                if let Some(phases) = plan.get("phases").and_then(serde_json::Value::as_array) {
                    for (phase_index, phase) in phases.iter().enumerate() {
                        let Some(phase) = phase.as_object() else {
                            continue;
                        };
                        let phase_title = phase
                            .get("title")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("");
                        let Some(tasks) = phase.get("tasks").and_then(serde_json::Value::as_array)
                        else {
                            continue;
                        };
                        for (task_index, task) in tasks.iter().enumerate() {
                            let Some(task) = task.as_object() else {
                                continue;
                            };
                            let id = task
                                .get("id")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string)
                                .unwrap_or_else(|| {
                                    format!(
                                        "legacy-step-{}-{}-{}",
                                        plan_index + 1,
                                        phase_index + 1,
                                        task_index + 1
                                    )
                                });
                            let title = task
                                .get("title")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or(&id)
                                .to_string();
                            let status = match task
                                .get("status")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("pending")
                            {
                                "completed" | "done" => "completed",
                                "in_progress" | "in-progress" | "inprogress" => "in_progress",
                                "blocked" => "blocked",
                                "skipped" => "skipped",
                                _ => "pending",
                            };
                            steps.push(serde_json::json!({
                                "id": id,
                                "title": title,
                                "status": status,
                                "notes": if phase_title.is_empty() {
                                    serde_json::Value::Null
                                } else {
                                    serde_json::Value::String(format!("Phase: {phase_title}"))
                                }
                            }));
                        }
                    }
                }
                plan.insert("steps".into(), serde_json::Value::Array(steps));
            }

            if let (Some(goal_id), Some(plan_id)) = (
                plan.get("goal_id").and_then(serde_json::Value::as_str),
                plan.get("id").and_then(serde_json::Value::as_str),
            ) {
                goal_plan_links.push((goal_id.to_string(), plan_id.to_string()));
            }
        }
    }

    if let Some(goals) = root.get_mut("goals").and_then(serde_json::Value::as_array_mut) {
        for goal in goals {
            let Some(goal) = goal.as_object_mut() else {
                continue;
            };
            let Some(goal_id) = goal
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            let Some(plan_ids) = goal
                .get_mut("plan_ids")
                .and_then(serde_json::Value::as_array_mut)
            else {
                continue;
            };
            for (_, plan_id) in goal_plan_links.iter().filter(|(id, _)| id == &goal_id) {
                if !plan_ids.iter().any(|value| value.as_str() == Some(plan_id)) {
                    plan_ids.push(serde_json::Value::String(plan_id.clone()));
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanningStore {
    path: PathBuf,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn legacy_string_success_criteria_are_loaded_and_normalized_on_save() {
        let workspace = tempdir().expect("workspace");
        let store = PlanningStore::new(workspace.path());
        let path = workspace.path().join(PLANNING_RELATIVE_PATH);
        std::fs::create_dir_all(path.parent().expect("planning dir")).expect("planning dir");

        let legacy = json!({
            "schema_version": 1,
            "revision": 11,
            "mode": "plan",
            "focus_goal_id": "goal-1",
            "focus_plan_id": "plan-1",
            "proposals": [],
            "goals": [{
                "id": "goal-1",
                "title": "Improve generated articles",
                "objective": "Improve article quality",
                "status": "active",
                "success_criteria": [
                    "Average content quality score improves by at least 20% against the V1 baseline corpus."
                ],
                "constraints": [],
                "plan_ids": ["plan-1"],
                "created_at": "2026-09-14T00:00:00Z",
                "updated_at": "2026-09-14T00:00:00Z"
            }],
            "plans": [{
                "id": "plan-1",
                "goal_id": "goal-1",
                "title": "Improve article generation",
                "objective": "Raise article quality",
                "status": "active",
                "steps": [
                    "Build a V1 baseline corpus",
                    "Improve the generation prompt"
                ],
                "task_ids": [],
                "revision": 1,
                "created_at": "2026-09-14T00:00:00Z",
                "updated_at": "2026-09-14T00:00:00Z"
            }]
        });
        std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&legacy).unwrap()))
            .expect("legacy state");

        let state = store.load().expect("legacy state should remain readable");
        assert_eq!(state.goals[0].success_criteria.len(), 1);
        assert_eq!(
            state.goals[0].success_criteria[0].text,
            "Average content quality score improves by at least 20% against the V1 baseline corpus."
        );
        assert_eq!(
            state.goals[0].success_criteria[0].id,
            "legacy-criterion-1"
        );
        assert_eq!(state.plans[0].steps.len(), 2);
        assert_eq!(state.plans[0].steps[0].id, "legacy-step-1");
        assert_eq!(state.plans[0].steps[0].status, crate::planning::PlanStepStatus::Pending);

        store
            .update(|state| {
                state.mode = crate::planning::PlanningMode::Goal;
                Ok(())
            })
            .expect("mode switch should normalize legacy state");
        let normalized: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("normalized state"))
                .expect("normalized json");
        assert!(normalized["goals"][0]["success_criteria"][0].is_object());
        assert_eq!(
            normalized["goals"][0]["success_criteria"][0]["completed"],
            false
        );
        assert!(normalized["plans"][0]["steps"][0].is_object());
        assert_eq!(normalized["mode"], "goal");
    }

    #[test]
    fn oxpecker_style_legacy_plan_is_migrated_without_losing_details() {
        let workspace = tempdir().expect("workspace");
        let store = PlanningStore::new(workspace.path());
        let path = workspace.path().join(PLANNING_RELATIVE_PATH);
        std::fs::create_dir_all(path.parent().expect("planning dir")).expect("planning dir");

        let legacy = json!({
            "schema_version": 1,
            "revision": 12,
            "mode": "direct",
            "focus_goal_id": "goal-content-quality-v2",
            "focus_plan_id": "plan-content-quality-v2",
            "proposals": [],
            "goals": [{
                "id": "goal-content-quality-v2",
                "title": "Improve article generation quality",
                "description": "Upgrade content generation quality without breaking existing workflows.",
                "status": "active",
                "success_criteria": [
                    "Average content quality score improves by at least 20% against the V1 baseline corpus."
                ]
            }],
            "plans": [{
                "id": "plan-content-quality-v2",
                "goal_id": "goal-content-quality-v2",
                "title": "Article Generation Quality V2",
                "status": "active",
                "strategy": "Decouple editorial reasoning from UI template filling.",
                "architecture": ["Article Planning -> ArticlePlan"],
                "phases": [{
                    "id": "phase-a-foundation",
                    "title": "Phase A - Baseline and low-risk foundation",
                    "priority": "P0",
                    "status": "pending",
                    "tasks": [{
                        "id": "a1-eval-baseline",
                        "title": "Create content generation evaluation corpus and V1 baseline",
                        "status": "pending",
                        "details": ["Add representative evaluation cases."],
                        "files": ["serverless-apis/tests/evals/content-generation/"]
                    }]
                }],
                "verification": ["Run npm test."],
                "rollout_order": ["Phase A first."]
            }],
            "execution": {
                "goal_id": "goal-content-quality-v2",
                "plan_id": "plan-content-quality-v2",
                "state": "planned",
                "changed_files": [".coding-tools/planning/state.json"],
                "verification": [],
                "updated_at": "1789365200"
            }
        });
        std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&legacy).unwrap()))
            .expect("legacy state");

        let state = store.load().expect("Oxpecker legacy plan should load");
        assert_eq!(state.schema_version, PLANNING_SCHEMA_VERSION);
        assert_eq!(
            state.goals[0].objective,
            "Upgrade content generation quality without breaking existing workflows."
        );
        assert_eq!(
            state.goals[0].plan_ids,
            vec!["plan-content-quality-v2".to_string()]
        );
        assert_eq!(state.plans[0].objective, "Decouple editorial reasoning from UI template filling.");
        assert_eq!(state.plans[0].steps.len(), 1);
        assert_eq!(state.plans[0].steps[0].id, "a1-eval-baseline");
        assert_eq!(
            state.plans[0].steps[0].title,
            "Create content generation evaluation corpus and V1 baseline"
        );
        assert!(state.plans[0].extra.contains_key("phases"));
        assert!(state.plans[0].extra.contains_key("architecture"));
        assert!(state.plans[0].extra.contains_key("verification"));
        assert!(state.plans[0].extra.contains_key("rollout_order"));

        store
            .update(|state| {
                state.mode = crate::planning::PlanningMode::Plan;
                Ok(())
            })
            .expect("mode switch should persist migrated state");

        let normalized: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("normalized state"))
                .expect("normalized json");
        assert_eq!(normalized["schema_version"], PLANNING_SCHEMA_VERSION);
        assert_eq!(normalized["mode"], "plan");
        assert!(normalized["goals"][0]["success_criteria"][0].is_object());
        assert_eq!(
            normalized["goals"][0]["objective"],
            "Upgrade content generation quality without breaking existing workflows."
        );
        assert_eq!(
            normalized["plans"][0]["steps"][0]["id"],
            "a1-eval-baseline"
        );
        assert!(normalized["plans"][0]["phases"].is_array());
        assert!(normalized["plans"][0]["architecture"].is_array());
        assert!(normalized["plans"][0]["verification"].is_array());
        assert!(normalized["plans"][0]["rollout_order"].is_array());
    }

    #[test]
    fn reset_recovers_even_when_existing_state_is_unreadable() {
        let workspace = tempdir().expect("workspace");
        let store = PlanningStore::new(workspace.path());
        let path = workspace.path().join(PLANNING_RELATIVE_PATH);
        std::fs::create_dir_all(path.parent().expect("planning dir")).expect("planning dir");
        std::fs::write(&path, "{ definitely not valid planning json\n").expect("broken state");

        assert!(store.load().is_err());

        let reset = store.reset().expect("reset must bypass broken state loading");
        assert_eq!(reset.mode, crate::planning::PlanningMode::Direct);
        assert!(reset.goals.is_empty());
        assert!(reset.plans.is_empty());

        let reloaded = store.load().expect("reset state should be readable");
        assert_eq!(reloaded.schema_version, PLANNING_SCHEMA_VERSION);
        assert!(reloaded.goals.is_empty());
        assert!(reloaded.plans.is_empty());
    }
}

impl PlanningStore {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            path: workspace_root.join(PLANNING_RELATIVE_PATH),
        }
    }

    pub fn load(&self) -> AppResult<PlanningState> {
        if !self.path.exists() {
            return Ok(PlanningState::default());
        }
        let raw = fs::read_to_string(&self.path)?;
        let mut value: serde_json::Value = serde_json::from_str(&raw)?;
        normalize_legacy_state(&mut value);
        Ok(serde_json::from_value(value)?)
    }

    pub fn save(&self, state: &PlanningState) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(state)?;
        fs::write(&self.path, format!("{raw}\n"))?;
        Ok(())
    }

    pub fn reset(&self) -> AppResult<PlanningState> {
        let state = PlanningState::default();
        self.save(&state)?;
        Ok(state)
    }

    pub fn update<R>(&self, mutate: impl FnOnce(&mut PlanningState) -> AppResult<R>) -> AppResult<R> {
        let mut state = self.load()?;
        state.revision = state.revision.saturating_add(1);
        let result = mutate(&mut state)?;
        self.save(&state)?;
        Ok(result)
    }
}
