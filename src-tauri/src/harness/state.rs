use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::json;
use uuid::Uuid;

use super::baseline::{capture_baseline, git_branch_and_head, timestamp, workspace_id};
use super::model::{
    CapabilityStatus, FileChangeRecord, HarnessEvent, HarnessStatus, OperationRecord,
    ProjectFileState, ProjectState, TaskSession, TaskStatus, SCHEMA_VERSION,
};
use super::store::{HarnessError, HarnessResult, HarnessStore};

#[derive(Debug, Clone)]
pub struct Harness {
    workspace_root: PathBuf,
    workspace_id: String,
    store: HarnessStore,
}

#[derive(Debug, Clone)]
pub struct OperationRecordInput {
    pub operation_id: Option<String>,
    pub task_id: Option<String>,
    pub tool: String,
    pub kind: String,
    pub input_summary: serde_json::Value,
    pub result_summary: serde_json::Value,
}

impl Harness {
    pub fn new(workspace_root: PathBuf, harness_root: PathBuf) -> HarnessResult<Self> {
        let workspace_root = workspace_root
            .canonicalize()
            .map_err(|e| HarnessError::new("WORKSPACE_UNAVAILABLE", e.to_string()))?;
        let workspace_id = workspace_id(&workspace_root);
        Ok(Self {
            workspace_root,
            workspace_id,
            store: HarnessStore::new(harness_root)?,
        })
    }

    pub fn default_root() -> HarnessResult<PathBuf> {
        let root = dirs::data_local_dir()
            .or_else(dirs::data_dir)
            .ok_or_else(|| HarnessError::new("STORE_UNAVAILABLE", "无法确定应用数据目录"))?;
        Ok(root.join("coding-tools-mcp").join("harness"))
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn store_root(&self) -> &Path {
        self.store.root()
    }

    pub fn start_task(&self, objective: &str) -> HarnessResult<TaskSession> {
        if objective.trim().is_empty() {
            return Err(HarnessError::new("INVALID_ARGUMENT", "任务目标不能为空"));
        }
        if let Some(task) = self.current_task()? {
            return Err(HarnessError::new(
                "TASK_ALREADY_ACTIVE",
                format!("工作区已有活动任务 {}", task.id),
            ));
        }
        let baseline = capture_baseline(&self.workspace_root);
        let now = timestamp();
        let task = TaskSession {
            id: Uuid::new_v4().simple().to_string(),
            workspace_id: self.workspace_id.clone(),
            objective: objective.trim().to_string(),
            status: TaskStatus::Active,
            expected_fingerprint: baseline.worktree_fingerprint.clone(),
            baseline,
            completed_steps: Vec::new(),
            pending_steps: Vec::new(),
            latest_change_id: None,
            latest_verification_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.save_task(&task)?;
        self.save_workspace_state(Some(&task.id), Some(&task.id), &task.updated_at)?;
        self.record_event(
            &task.id,
            "task_started",
            None,
            json!({}),
            json!({"ok": true}),
        )?;
        Ok(task)
    }

    pub fn current_task(&self) -> HarnessResult<Option<TaskSession>> {
        if let Some(state) = self.store.load_workspace_state(&self.workspace_id)? {
            let Some(active_id) = state.active_task_id else {
                return Ok(None);
            };
            if let Ok(task) = self.store.load_task(&self.workspace_id, &active_id) {
                return Ok(task.status.is_writable().then_some(task));
            }
        }
        Ok(self
            .store
            .list_tasks(&self.workspace_id)?
            .into_iter()
            .find(|task| task.status.is_writable()))
    }

    pub fn task(&self, task_id: &str) -> HarnessResult<TaskSession> {
        self.store.load_task(&self.workspace_id, task_id)
    }

    pub fn transition(&self, task_id: &str, next: TaskStatus) -> HarnessResult<TaskSession> {
        let mut task = self.task(task_id)?;
        if !task.status.can_transition_to(next) {
            return Err(HarnessError::new(
                "INVALID_TASK_TRANSITION",
                format!("不允许从 {:?} 转换到 {:?}", task.status, next),
            ));
        }
        task.status = next;
        task.updated_at = timestamp();
        self.store.save_task(&task)?;
        let active_id = task.status.is_writable().then_some(task.id.as_str());
        self.save_workspace_state(active_id, Some(&task.id), &task.updated_at)?;
        self.record_event(
            task_id,
            "task_status_changed",
            None,
            json!({"status": next}),
            json!({"ok": true}),
        )?;
        Ok(task)
    }

    pub fn update_steps(
        &self,
        task_id: &str,
        completed_steps: Option<Vec<String>>,
        pending_steps: Option<Vec<String>>,
    ) -> HarnessResult<TaskSession> {
        let mut task = self.task(task_id)?;
        if let Some(steps) = completed_steps {
            task.completed_steps = steps;
        }
        if let Some(steps) = pending_steps {
            task.pending_steps = steps;
        }
        task.updated_at = timestamp();
        self.store.save_task(&task)?;
        self.record_event(
            task_id,
            "task_updated",
            None,
            json!({
                "completed_steps": task.completed_steps,
                "pending_steps": task.pending_steps
            }),
            json!({"ok": true}),
        )?;
        Ok(task)
    }

    pub fn check_baseline(&self, task_id: &str) -> HarnessResult<()> {
        let task = self.task(task_id)?;
        let current = capture_baseline(&self.workspace_root);
        if current.branch != task.baseline.branch || current.head != task.baseline.head {
            return Err(HarnessError::new(
                "BASELINE_STALE",
                "Git 分支或 HEAD 已发生变化",
            ));
        }
        if current.worktree_fingerprint != task.expected_fingerprint {
            return Err(HarnessError::new(
                "FILE_CHANGED_EXTERNALLY",
                "工作区存在 Harness 未记录的外部文件变化",
            ));
        }
        Ok(())
    }

    pub fn refresh_expected_state(&self, task_id: &str) -> HarnessResult<TaskSession> {
        let mut task = self.task(task_id)?;
        task.expected_fingerprint = capture_baseline(&self.workspace_root).worktree_fingerprint;
        task.updated_at = timestamp();
        self.store.save_task(&task)?;
        Ok(task)
    }

    pub fn record_event(
        &self,
        task_id: &str,
        kind: &str,
        tool_name: Option<&str>,
        input_summary: serde_json::Value,
        result_summary: serde_json::Value,
    ) -> HarnessResult<HarnessEvent> {
        let event = HarnessEvent {
            id: Uuid::new_v4().simple().to_string(),
            task_id: task_id.to_string(),
            operation_id: Uuid::new_v4().simple().to_string(),
            kind: kind.to_string(),
            tool_name: tool_name.map(str::to_string),
            input_summary: json!({"workspace_id": self.workspace_id, "payload": input_summary}),
            result_summary,
            reason: None,
            affected_files: Vec::<FileChangeRecord>::new(),
            created_at: timestamp(),
        };
        self.store
            .append_event_for_workspace(&self.workspace_id, &event)?;
        Ok(event)
    }

    pub fn list_events(
        &self,
        task_id: &str,
        offset: usize,
        limit: usize,
    ) -> HarnessResult<Vec<HarnessEvent>> {
        self.store
            .list_events(&self.workspace_id, task_id, offset, limit)
    }

    pub fn record_operation(&self, input: OperationRecordInput) -> HarnessResult<OperationRecord> {
        let reason = input
            .input_summary
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let operation = OperationRecord {
            id: input
                .operation_id
                .unwrap_or_else(|| Uuid::new_v4().simple().to_string()),
            workspace_id: self.workspace_id.clone(),
            task_id: input.task_id,
            tool: input.tool,
            kind: input.kind,
            input_summary: input.input_summary,
            result_summary: input.result_summary,
            reason,
            affected_files: Vec::new(),
            created_at: timestamp(),
        };
        self.store
            .append_operation(&self.workspace_id, &operation)?;
        Ok(operation)
    }

    pub fn list_operations(
        &self,
        offset: usize,
        limit: usize,
    ) -> HarnessResult<Vec<OperationRecord>> {
        self.store
            .list_operations(&self.workspace_id, offset, limit)
    }

    pub fn project_state(&self, max_files: usize) -> HarnessResult<ProjectState> {
        let current = capture_baseline(&self.workspace_root);
        let task = self.current_task()?;
        let baseline_map = task
            .as_ref()
            .map(|t| {
                t.baseline
                    .entries
                    .iter()
                    .map(|e| (e.path.as_str(), e))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let current_map: HashMap<&str, _> = current
            .entries
            .iter()
            .map(|e| (e.path.as_str(), e))
            .collect();
        let mut paths: Vec<&str> = baseline_map
            .keys()
            .chain(current_map.keys())
            .copied()
            .collect();
        paths.sort_unstable();
        paths.dedup();
        let total_files = paths.len();
        let files = paths
            .into_iter()
            .map(|path| {
                let before = baseline_map.get(path).map(|e| e.sha256.as_str());
                let entry = current_map.get(path);
                let status = match (before, entry) {
                    (Some(before), Some(entry)) if before == entry.sha256 => "unchanged",
                    (Some(_), Some(_)) => "modified",
                    (Some(_), None) => "deleted",
                    (None, Some(_)) => "added",
                    (None, None) => "unknown",
                };
                ProjectFileState {
                    path: path.to_string(),
                    status: status.to_string(),
                    sha256: entry.map(|e| e.sha256.clone()).unwrap_or_default(),
                    bytes: entry.map(|e| e.bytes).unwrap_or(0),
                }
            })
            .collect::<Vec<_>>();
        // Compute cleanliness over the full file set before applying max_files truncation.
        let clean = files.iter().all(|f| f.status == "unchanged");
        let max_files = max_files.max(1);
        let truncated = files.len() > max_files;
        let files = files.into_iter().take(max_files).collect::<Vec<_>>();
        let active_task_id = task.as_ref().map(|t| t.id.clone());
        let recent_events = task
            .as_ref()
            .and_then(|t| self.list_events(&t.id, 0, 100).ok())
            .map(|events| events.len())
            .unwrap_or(0);
        Ok(ProjectState {
            schema_version: SCHEMA_VERSION,
            workspace_id: self.workspace_id.clone(),
            branch: current.branch,
            head: current.head,
            clean,
            files,
            total_files,
            truncated,
            active_task_id,
            task,
            recent_events,
        })
    }

    pub fn status(&self) -> HarnessResult<HarnessStatus> {
        let task = self.current_task()?;
        let (
            branch,
            head,
            task_id,
            task_state,
            task_updated_at,
            writable,
            baseline_matches,
            reason,
        ) = match task.as_ref() {
            Some(task) => {
                let current = capture_baseline(&self.workspace_root);
                let matches = task.baseline.branch == current.branch
                    && task.baseline.head == current.head
                    && task.expected_fingerprint == current.worktree_fingerprint;
                let reason = if matches {
                    "任务可继续执行"
                } else {
                    "工作区基线已变化，写入和执行已暂停"
                };
                (
                    current.branch,
                    current.head,
                    Some(task.id.clone()),
                    Some(task.status),
                    Some(task.updated_at.clone()),
                    matches && task.status.is_writable(),
                    Some(matches),
                    reason.to_string(),
                )
            }
            None => {
                // Avoid scanning and hashing the entire repository on status checks
                // when no task is active (especially on every tool error attachment).
                let (branch, head) = git_branch_and_head(&self.workspace_root);
                (
                    branch,
                    head,
                    None,
                    None,
                    None,
                    true,
                    None,
                    "当前没有活动任务，工作区采用无任务模式；修改不会进入任务事件流".to_string(),
                )
            }
        };

        let mut capabilities = HashMap::new();
        capabilities.insert(
            "read".into(),
            CapabilityStatus {
                status: "available".into(),
                reason: "工作区读取不依赖活动任务".into(),
                recoverable: true,
            },
        );
        capabilities.insert(
            "write".into(),
            CapabilityStatus {
                status: if writable { "available" } else { "denied" }.into(),
                reason: if writable {
                    if task_id.is_some() {
                        "活动任务和工作区基线有效"
                    } else {
                        "无任务模式允许直接修改，建议需要长期追踪时调用 start_task"
                    }
                } else {
                    "需要活动任务且工作区基线必须匹配"
                }
                .into(),
                recoverable: true,
            },
        );
        capabilities.insert(
            "exec".into(),
            CapabilityStatus {
                status: if writable { "available" } else { "denied" }.into(),
                reason: if writable {
                    if task_id.is_some() {
                        "活动任务和工作区基线有效"
                    } else {
                        "无任务模式允许直接执行，建议需要长期追踪时调用 start_task"
                    }
                } else {
                    "需要活动任务且工作区基线必须匹配"
                }
                .into(),
                recoverable: true,
            },
        );
        capabilities.insert(
            "git".into(),
            CapabilityStatus {
                status: if branch.is_some() && head.is_some() {
                    "available"
                } else {
                    "degraded"
                }
                .into(),
                reason: if branch.is_some() && head.is_some() {
                    "已读取当前分支和 HEAD"
                } else {
                    "当前工作区不是可读取 Git 状态的仓库"
                }
                .into(),
                recoverable: true,
            },
        );
        capabilities.insert(
            "network".into(),
            CapabilityStatus {
                status: "managed_by_policy".into(),
                reason: "网络权限由工具策略控制，不由 Harness 任务状态决定".into(),
                recoverable: true,
            },
        );

        let mut next_actions = Vec::new();
        if task_id.is_none() {
            next_actions.push("start_task".into());
        } else if baseline_matches == Some(false) {
            next_actions.push("project_state".into());
            next_actions.push("git_diff".into());
        } else if !writable {
            next_actions.push("resume_task".into());
        }
        next_actions.push("read_file".into());
        next_actions.push("git_status".into());

        Ok(HarnessStatus {
            schema_version: SCHEMA_VERSION,
            workspace_id: self.workspace_id.clone(),
            task_id,
            task_state,
            task_updated_at,
            writable,
            reason,
            recoverable: true,
            branch,
            head,
            baseline_matches,
            capabilities,
            next_actions,
        })
    }

    fn save_workspace_state(
        &self,
        active_task_id: Option<&str>,
        touched_task_id: Option<&str>,
        updated_at: &str,
    ) -> HarnessResult<()> {
        self.store
            .update_workspace_state(
                &self.workspace_id,
                active_task_id,
                touched_task_id,
                updated_at,
            )
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn status_keeps_read_available_without_task() {
        let workspace = tempdir().expect("workspace");
        let harness_root = tempdir().expect("harness");
        fs::write(workspace.path().join("main.rs"), "fn main() {}\n").expect("file");
        let harness = Harness::new(
            workspace.path().to_path_buf(),
            harness_root.path().to_path_buf(),
        )
        .expect("harness");

        let status = harness.status().expect("status");
        assert!(status.writable);
        assert_eq!(status.capabilities["read"].status, "available");
        assert_eq!(status.capabilities["write"].status, "available");
        assert!(status.next_actions.contains(&"start_task".to_string()));
    }

    #[test]
    fn starting_task_does_not_create_workspace_copies() {
        let workspace = tempdir().expect("workspace");
        let harness_root = tempdir().expect("harness");
        fs::write(workspace.path().join("main.rs"), "fn main() {}\n").expect("file");
        let harness = Harness::new(
            workspace.path().to_path_buf(),
            harness_root.path().to_path_buf(),
        )
        .expect("harness");

        harness.start_task("测试任务").expect("start task");
        assert!(!harness
            .store_root()
            .join("workspaces")
            .join(harness.workspace_id())
            .join("snapshots")
            .exists());
    }

    #[test]
    fn planning_runtime_state_does_not_invalidate_harness_baseline() {
        let workspace = tempdir().expect("workspace");
        let harness_root = tempdir().expect("harness");
        fs::write(workspace.path().join("main.rs"), "fn main() {}\n").expect("file");
        let harness = Harness::new(
            workspace.path().to_path_buf(),
            harness_root.path().to_path_buf(),
        )
        .expect("harness");
        let task = harness
            .start_task("测试 Planning 运行时状态")
            .expect("start task");

        let planning_dir = workspace.path().join(".coding-tools/planning");
        fs::create_dir_all(&planning_dir).expect("planning dir");
        fs::write(planning_dir.join("state.json"), "{\"revision\":1}\n").expect("planning state");

        harness
            .check_baseline(&task.id)
            .expect("Harness 自己的 Planning 状态不应被视为外部变化");
    }

    #[test]
    fn managed_history_does_not_invalidate_harness_baseline() {
        let workspace = tempdir().expect("workspace");
        let harness_root = tempdir().expect("harness");
        fs::write(workspace.path().join("main.rs"), "fn main() {}\n").expect("file");
        let harness = Harness::new(
            workspace.path().to_path_buf(),
            harness_root.path().to_path_buf(),
        )
        .expect("harness");
        let task = harness
            .start_task("测试 History 运行时状态")
            .expect("start task");

        let history_dir = workspace.path().join("docs/history-session");
        fs::create_dir_all(&history_dir).expect("history dir");
        fs::write(history_dir.join("1.md"), "# managed history\n").expect("history file");

        harness
            .check_baseline(&task.id)
            .expect("MCP 自己的 history archive 不应被视为外部变化");
    }

    #[test]
    fn real_workspace_change_still_invalidates_harness_baseline() {
        let workspace = tempdir().expect("workspace");
        let harness_root = tempdir().expect("harness");
        fs::write(workspace.path().join("main.rs"), "fn main() {}\n").expect("file");
        let harness = Harness::new(
            workspace.path().to_path_buf(),
            harness_root.path().to_path_buf(),
        )
        .expect("harness");
        let task = harness.start_task("测试真实外部变化").expect("start task");

        fs::write(
            workspace.path().join("main.rs"),
            "fn main() { println!(\"changed\"); }\n",
        )
        .expect("external change");

        let error = harness
            .check_baseline(&task.id)
            .expect_err("真实项目文件变化必须使 baseline 失效");
        assert_eq!(error.code(), "FILE_CHANGED_EXTERNALLY");
    }

    #[test]
    fn status_without_active_task_skips_baseline_scan() {
        let workspace = tempdir().expect("workspace");
        let harness_root = tempdir().expect("harness");
        fs::write(workspace.path().join("main.rs"), "fn main() {}\n").expect("file");
        let harness = Harness::new(
            workspace.path().to_path_buf(),
            harness_root.path().to_path_buf(),
        )
        .expect("harness");

        super::super::baseline::reset_baseline_capture_calls();
        let idle_status = harness.status().expect("idle status");
        assert_eq!(super::super::baseline::baseline_capture_calls(), 0);
        assert!(idle_status.writable);
        assert_eq!(idle_status.baseline_matches, None);

        let task = harness.start_task("验证状态扫描").expect("start task");
        super::super::baseline::reset_baseline_capture_calls();
        let active_status = harness.status().expect("active status");
        assert_eq!(super::super::baseline::baseline_capture_calls(), 1);
        assert_eq!(active_status.baseline_matches, Some(true));

        // External modification makes baseline mismatch, and next_actions must not
        // suggest the removed nonexistent "refresh_baseline" tool.
        fs::write(
            workspace.path().join("main.rs"),
            "fn main() { /* dirty */ }\n",
        )
        .expect("dirty");
        let stale_status = harness.status().expect("stale status");
        assert_eq!(stale_status.baseline_matches, Some(false));
        assert!(!stale_status
            .next_actions
            .contains(&"refresh_baseline".to_string()));
        assert_eq!(stale_status.task_id.as_deref(), Some(task.id.as_str()));
    }

    #[test]
    fn project_state_computes_clean_before_truncating_files() {
        let workspace = tempdir().expect("workspace");
        let harness_root = tempdir().expect("harness");
        fs::write(workspace.path().join("a.rs"), "fn a() {}\n").expect("file a");
        fs::write(workspace.path().join("b.rs"), "fn b() {}\n").expect("file b");
        let harness = Harness::new(
            workspace.path().to_path_buf(),
            harness_root.path().to_path_buf(),
        )
        .expect("harness");

        harness
            .start_task("验证 clean 截断前计算")
            .expect("start task");
        fs::write(workspace.path().join("b.rs"), "fn b() { /* changed */ }\n").expect("modify b");

        let state = harness.project_state(1).expect("project state");
        assert_eq!(state.total_files, 2);
        assert_eq!(state.files.len(), 1);
        assert!(state.truncated);
        assert_eq!(state.files[0].path, "a.rs");
        assert_eq!(state.files[0].status, "unchanged");
        assert!(
            !state.clean,
            "truncated modified file must mark state dirty"
        );
    }

    #[test]
    fn current_task_recovers_in_o1_from_state_json_and_falls_back_when_absent() {
        let workspace = tempdir().expect("workspace");
        let harness_root = tempdir().expect("harness");
        fs::write(workspace.path().join("main.rs"), "fn main() {}\n").expect("file");
        let harness = Harness::new(
            workspace.path().to_path_buf(),
            harness_root.path().to_path_buf(),
        )
        .expect("harness");

        let task = harness.start_task("验证 O(1) 恢复").expect("start task");
        let state_json = harness
            .store_root()
            .join("workspaces")
            .join(harness.workspace_id())
            .join("state.json");
        assert!(state_json.exists());

        // Plant an extra corrupt task file in tasks/ to confirm O(1) lookup via state.json
        // succeeds cleanly without scanning the whole directory.
        let tasks_dir = harness
            .store_root()
            .join("workspaces")
            .join(harness.workspace_id())
            .join("tasks");
        fs::write(tasks_dir.join("zzz-corrupt.json"), "{broken").expect("corrupt task");

        let active = harness
            .current_task()
            .expect("current task from state.json")
            .expect("active task");
        assert_eq!(active.id, task.id);

        // Removing state.json falls back to scanning tasks/.
        fs::remove_file(&state_json).expect("remove state.json");
        let recovered = harness
            .current_task()
            .expect("fallback scan")
            .expect("recovered active task");
        assert_eq!(recovered.id, task.id);

        // Finishing the task clears active_task_id in state.json so current_task returns None.
        harness
            .transition(&task.id, TaskStatus::CompletedUnverified)
            .expect("complete task");
        assert!(harness.current_task().expect("no active task").is_none());
        let saved_state: super::super::model::WorkspaceHarnessState =
            serde_json::from_slice(&fs::read(&state_json).expect("read state.json"))
                .expect("parse state.json");
        assert_eq!(saved_state.active_task_id, None);
        assert_eq!(
            saved_state.recent_task_ids.first().map(String::as_str),
            Some(task.id.as_str())
        );
    }
}
