use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::error::AppResult;
use crate::util::fs::{atomic_write_bytes, lock_file as acquire_file_lock};

use super::migration::normalize_legacy_state;
use super::{PlanningState, PLANNING_RELATIVE_PATH, PLANNING_SCHEMA_VERSION};

const PLANNING_LOCK_FILE: &str = ".state.lock";

fn has_current_schema_version_hint(raw: &str) -> bool {
    raw.match_indices("\"schema_version\"").any(|(idx, key)| {
        let rest = raw[idx + key.len()..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else {
            return false;
        };
        let rest = rest.trim_start();
        let digits_len = rest
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        digits_len > 0 && rest[..digits_len].parse::<u32>().ok() == Some(PLANNING_SCHEMA_VERSION)
    })
}

#[derive(Debug, Clone)]
pub struct PlanningStore {
    path: PathBuf,
}

impl PlanningStore {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            path: workspace_root.join(PLANNING_RELATIVE_PATH),
        }
    }

    fn lock_path(&self) -> PathBuf {
        self.path.with_file_name(PLANNING_LOCK_FILE)
    }

    fn load_unlocked(&self) -> AppResult<PlanningState> {
        let raw = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(PlanningState::default())
            }
            Err(error) => return Err(error.into()),
        };
        // Fast path for current v2 schema avoids constructing an intermediate
        // serde_json::Value tree on every read or update.
        if has_current_schema_version_hint(&raw) {
            if let Ok(state) = serde_json::from_str::<PlanningState>(&raw) {
                if state.schema_version == PLANNING_SCHEMA_VERSION {
                    return Ok(state);
                }
            }
        }
        let mut value: serde_json::Value = serde_json::from_str(&raw)?;
        normalize_legacy_state(&mut value);
        Ok(serde_json::from_value(value)?)
    }

    fn write_unlocked(&self, state: &PlanningState) -> AppResult<()> {
        let mut payload = serde_json::to_vec_pretty(state)?;
        payload.push(b'\n');
        atomic_write_bytes(&self.path, &payload)?;
        Ok(())
    }

    pub fn load(&self) -> AppResult<PlanningState> {
        self.load_unlocked()
    }

    pub fn save(&self, state: &PlanningState) -> AppResult<()> {
        let _lock = acquire_file_lock(&self.lock_path())?;
        self.write_unlocked(state)
    }

    pub fn reset(&self) -> AppResult<PlanningState> {
        let state = PlanningState::default();
        self.save(&state)?;
        Ok(state)
    }

    pub fn update<R>(
        &self,
        mutate: impl FnOnce(&mut PlanningState) -> AppResult<R>,
    ) -> AppResult<R> {
        let _lock = acquire_file_lock(&self.lock_path())?;
        let mut state = self.load_unlocked()?;
        state.revision = state.revision.saturating_add(1);
        let result = mutate(&mut state)?;
        self.write_unlocked(&state)?;
        Ok(result)
    }
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
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&legacy).unwrap()),
        )
        .expect("legacy state");

        let state = store.load().expect("legacy state should remain readable");
        assert_eq!(state.goals[0].success_criteria.len(), 1);
        assert_eq!(
            state.goals[0].success_criteria[0].text,
            "Average content quality score improves by at least 20% against the V1 baseline corpus."
        );
        assert_eq!(state.goals[0].success_criteria[0].id, "legacy-criterion-1");
        assert_eq!(state.plans[0].steps.len(), 2);
        assert_eq!(state.plans[0].steps[0].id, "legacy-step-1");
        assert_eq!(
            state.plans[0].steps[0].status,
            crate::planning::PlanStepStatus::Pending
        );

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
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&legacy).unwrap()),
        )
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
        assert_eq!(
            state.plans[0].objective,
            "Decouple editorial reasoning from UI template filling."
        );
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
        assert_eq!(normalized["plans"][0]["steps"][0]["id"], "a1-eval-baseline");
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

        let reset = store
            .reset()
            .expect("reset must bypass broken state loading");
        assert_eq!(reset.mode, crate::planning::PlanningMode::Direct);
        assert!(reset.goals.is_empty());
        assert!(reset.plans.is_empty());

        let reloaded = store.load().expect("reset state should be readable");
        assert_eq!(reloaded.schema_version, PLANNING_SCHEMA_VERSION);
        assert!(reloaded.goals.is_empty());
        assert!(reloaded.plans.is_empty());
    }

    #[test]
    fn v2_fast_path_loads_directly_and_falls_back_for_incomplete_v2_or_v1() {
        let workspace = tempdir().expect("workspace");
        let store = PlanningStore::new(workspace.path());
        let path = workspace.path().join(PLANNING_RELATIVE_PATH);
        std::fs::create_dir_all(path.parent().expect("planning dir")).expect("planning dir");

        let v2_state = json!({
            "schema_version": 2,
            "revision": 7,
            "mode": "plan",
            "focus_goal_id": "goal-v2",
            "focus_plan_id": "plan-v2",
            "goals": [{
                "id": "goal-v2",
                "title": "Fast path goal",
                "objective": "Deserialize directly without Value migration",
                "status": "active",
                "success_criteria": [{
                    "id": "crit-1",
                    "text": "Loads cleanly",
                    "completed": true
                }],
                "constraints": ["No regression"],
                "plan_ids": ["plan-v2"],
                "created_at": "100",
                "updated_at": "101"
            }],
            "plans": [{
                "id": "plan-v2",
                "goal_id": "goal-v2",
                "title": "Fast path plan",
                "objective": "Verify v2 direct load",
                "status": "active",
                "steps": [{
                    "id": "step-1",
                    "title": "Verify",
                    "status": "completed",
                    "notes": "done"
                }],
                "task_ids": ["task-1"],
                "revision": 3,
                "created_at": "100",
                "updated_at": "101"
            }],
            "execution": {
                "goal_id": "goal-v2",
                "plan_id": "plan-v2",
                "step_id": "step-1",
                "task_id": "task-1",
                "last_tool": "apply_patch",
                "state": "completed",
                "last_error": null,
                "changed_files": ["src/main.rs"],
                "history_checkpoint_ref": null,
                "verification": ["cargo test"],
                "updated_at": "101"
            }
        });
        let raw_v2 = format!("{}\n", serde_json::to_string_pretty(&v2_state).unwrap());
        assert!(has_current_schema_version_hint(&raw_v2));
        std::fs::write(&path, raw_v2).expect("write v2 state");

        let loaded = store.load().expect("v2 fast path load");
        assert_eq!(loaded.schema_version, PLANNING_SCHEMA_VERSION);
        assert_eq!(loaded.revision, 7);
        assert_eq!(loaded.mode, crate::planning::PlanningMode::Plan);
        assert_eq!(loaded.focus_goal_id.as_deref(), Some("goal-v2"));
        assert_eq!(loaded.focus_plan_id.as_deref(), Some("plan-v2"));
        assert_eq!(loaded.goals.len(), 1);
        assert!(loaded.goals[0].success_criteria[0].completed);
        assert_eq!(loaded.plans[0].steps[0].notes.as_deref(), Some("done"));

        // A file declaring schema_version=2 but missing required legacy-migrated fields
        // must still transparently fall back to the Value migration path.
        let incomplete_v2 = json!({
            "schema_version": 2,
            "revision": 2,
            "mode": "direct",
            "focus_goal_id": null,
            "focus_plan_id": null,
            "goals": [{
                "id": "goal-fallback",
                "title": "Fallback Goal",
                "description": "Migrated from description",
                "status": "active"
            }],
            "plans": []
        });
        std::fs::write(&path, serde_json::to_string(&incomplete_v2).unwrap())
            .expect("write incomplete v2");
        let migrated = store.load().expect("fallback migration should succeed");
        assert_eq!(migrated.goals[0].objective, "Migrated from description");

        // Hint helper rejects non-v2 schema versions (e.g. 1 or 20).
        assert!(!has_current_schema_version_hint(r#"{"schema_version": 1}"#));
        assert!(!has_current_schema_version_hint(
            r#"{"schema_version": 20}"#
        ));
    }

    #[test]
    fn concurrent_updates_are_serialized_without_lost_revisions() {
        let workspace = tempdir().expect("workspace");
        let store = PlanningStore::new(workspace.path());
        let workers = (0..4)
            .map(|worker| {
                let store = store.clone();
                std::thread::spawn(move || {
                    for step in 0..8 {
                        store
                            .update(|state| {
                                state
                                    .execution
                                    .verification
                                    .push(format!("w{worker}-{step}"));
                                Ok(())
                            })
                            .expect("concurrent update");
                    }
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker.join().expect("worker join");
        }
        let state = store.load().expect("reload");
        assert_eq!(state.revision, 32);
        assert_eq!(state.execution.verification.len(), 32);
    }
}
