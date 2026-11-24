use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::util::fs::{atomic_write_bytes, lock_file as acquire_file_lock, FileLock};

use super::model::{
    HarnessEvent, OperationRecord, TaskSession, WorkspaceHarnessState, SCHEMA_VERSION,
};

const STORE_LOCK_FILE: &str = ".store.lock";
const MAX_RECENT_TASK_IDS: usize = 20;

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct HarnessError {
    code: &'static str,
    message: String,
}

impl HarnessError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }
}

pub type HarnessResult<T> = Result<T, HarnessError>;

#[derive(Debug, Clone)]
pub struct HarnessStore {
    root: PathBuf,
}

impl HarnessStore {
    pub fn new(root: PathBuf) -> HarnessResult<Self> {
        fs::create_dir_all(&root)
            .map_err(|e| HarnessError::new("STORE_UNAVAILABLE", e.to_string()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn workspace_dir(&self, workspace_id: &str) -> PathBuf {
        self.root.join("workspaces").join(workspace_id)
    }

    fn workspace_lock_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_dir(workspace_id).join(STORE_LOCK_FILE)
    }

    fn lock_workspace(&self, workspace_id: &str) -> HarnessResult<FileLock> {
        acquire_file_lock(&self.workspace_lock_path(workspace_id)).map_err(io_error)
    }

    fn workspace_state_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_dir(workspace_id).join("state.json")
    }

    fn tasks_dir(&self, workspace_id: &str) -> PathBuf {
        self.workspace_dir(workspace_id).join("tasks")
    }

    fn events_dir(&self, workspace_id: &str) -> PathBuf {
        self.workspace_dir(workspace_id).join("events")
    }

    fn operations_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_dir(workspace_id).join("operations.jsonl")
    }

    pub fn save_task(&self, task: &TaskSession) -> HarnessResult<()> {
        validate_task_id(&task.id)?;
        let _lock = self.lock_workspace(&task.workspace_id)?;
        let path = self
            .tasks_dir(&task.workspace_id)
            .join(format!("{}.json", task.id));
        atomic_write_json(&path, task)
    }

    pub fn load_task(&self, workspace_id: &str, task_id: &str) -> HarnessResult<TaskSession> {
        validate_task_id(task_id)?;
        read_json(&self.tasks_dir(workspace_id).join(format!("{task_id}.json")))
    }

    pub fn list_tasks(&self, workspace_id: &str) -> HarnessResult<Vec<TaskSession>> {
        let dir = self.tasks_dir(workspace_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut tasks: Vec<TaskSession> = Vec::new();
        for entry in fs::read_dir(dir).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Ok(task) = read_json(&path) {
                tasks.push(task);
            }
        }
        tasks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(tasks)
    }

    pub(super) fn load_workspace_state(
        &self,
        workspace_id: &str,
    ) -> HarnessResult<Option<WorkspaceHarnessState>> {
        self.load_workspace_state_unlocked(workspace_id)
    }

    fn load_workspace_state_unlocked(
        &self,
        workspace_id: &str,
    ) -> HarnessResult<Option<WorkspaceHarnessState>> {
        let path = self.workspace_state_path(workspace_id);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
        };
        let state = serde_json::from_slice(&bytes)
            .map_err(|e| HarnessError::new("STORE_CORRUPT", format!("{}: {e}", path.display())))?;
        Ok(Some(state))
    }

    pub(super) fn update_workspace_state(
        &self,
        workspace_id: &str,
        active_task_id: Option<&str>,
        touched_task_id: Option<&str>,
        updated_at: &str,
    ) -> HarnessResult<WorkspaceHarnessState> {
        let _lock = self.lock_workspace(workspace_id)?;
        let mut recent_task_ids = self
            .load_workspace_state_unlocked(workspace_id)
            .ok()
            .flatten()
            .map(|existing| existing.recent_task_ids)
            .unwrap_or_default();
        if let Some(task_id) = touched_task_id.or(active_task_id) {
            recent_task_ids.retain(|existing| existing != task_id);
            recent_task_ids.insert(0, task_id.to_string());
            recent_task_ids.truncate(MAX_RECENT_TASK_IDS);
        }
        let next = WorkspaceHarnessState {
            schema_version: SCHEMA_VERSION,
            active_task_id: active_task_id.map(str::to_string),
            recent_task_ids,
            updated_at: updated_at.to_string(),
        };
        atomic_write_json(&self.workspace_state_path(workspace_id), &next)?;
        Ok(next)
    }

    pub fn append_event_for_workspace(
        &self,
        workspace_id: &str,
        event: &HarnessEvent,
    ) -> HarnessResult<()> {
        validate_task_id(&event.task_id)?;
        let _lock = self.lock_workspace(workspace_id)?;
        let path = self
            .events_dir(workspace_id)
            .join(format!("{}.jsonl", event.task_id));
        append_jsonl_unlocked(&path, event)
    }

    pub fn append_operation(
        &self,
        workspace_id: &str,
        operation: &OperationRecord,
    ) -> HarnessResult<()> {
        let _lock = self.lock_workspace(workspace_id)?;
        append_jsonl_unlocked(&self.operations_path(workspace_id), operation)
    }

    pub fn list_operations(
        &self,
        workspace_id: &str,
        offset: usize,
        limit: usize,
    ) -> HarnessResult<Vec<OperationRecord>> {
        let path = self.operations_path(workspace_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(path).map_err(io_error)?;
        let mut operations = Vec::new();
        for line in BufReader::new(file).lines().skip(offset).take(limit.max(1)) {
            let line = line.map_err(io_error)?;
            match serde_json::from_str(&line) {
                Ok(operation) => operations.push(operation),
                Err(_) => break,
            }
        }
        Ok(operations)
    }

    pub fn list_events(
        &self,
        workspace_id: &str,
        task_id: &str,
        offset: usize,
        limit: usize,
    ) -> HarnessResult<Vec<HarnessEvent>> {
        validate_task_id(task_id)?;
        let path = self
            .events_dir(workspace_id)
            .join(format!("{task_id}.jsonl"));
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(path).map_err(io_error)?;
        let mut events = Vec::new();
        for line in BufReader::new(file).lines().skip(offset).take(limit.max(1)) {
            let line = line.map_err(io_error)?;
            match serde_json::from_str(&line) {
                Ok(event) => events.push(event),
                Err(_) => break,
            }
        }
        Ok(events)
    }
}

fn validate_task_id(task_id: &str) -> HarnessResult<()> {
    if !crate::util::text::is_safe_identifier(task_id) {
        return Err(HarnessError::new(
            "INVALID_ARGUMENT",
            format!("invalid task_id: {task_id}"),
        ));
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> HarnessError {
    HarnessError::new("STORE_IO_FAILED", error.to_string())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> HarnessResult<T> {
    let bytes = fs::read(path).map_err(io_error)?;
    serde_json::from_slice(&bytes)
        .map_err(|e| HarnessError::new("STORE_CORRUPT", format!("{}: {e}", path.display())))
}

fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> HarnessResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| HarnessError::new("STORE_SERIALIZE_FAILED", e.to_string()))?;
    atomic_write_bytes(path, &bytes).map_err(io_error)
}

fn append_jsonl_unlocked<T: serde::Serialize>(path: &Path, value: &T) -> HarnessResult<()> {
    let mut payload = serde_json::to_vec(value)
        .map_err(|e| HarnessError::new("STORE_SERIALIZE_FAILED", e.to_string()))?;
    payload.push(b'\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(&payload).map_err(io_error)
}
