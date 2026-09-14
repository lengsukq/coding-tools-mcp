use super::*;

#[derive(Debug, Clone)]
pub(super) struct EvictedCommand {
    command_id: String,
    evicted_at: String,
    termination_reason: String,
    exit_code: Option<i32>,
}

pub(super) fn command_id_arg(args: &Value) -> Result<&str, WorkspaceError> {
    args.get("command_id")
        .and_then(Value::as_str)
        .or_else(|| args.get("session_id").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            WorkspaceError::invalid_argument(
                "command_id is required (legacy session_id is also accepted)",
            )
        })
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, session: ExecSession) -> Arc<ExecSession> {
        let arc = Arc::new(session);
        self.sessions
            .lock()
            .expect("sessions lock")
            .insert(arc.session_id.clone(), arc.clone());
        arc
    }

    pub fn get(&self, session_id: &str) -> Result<Arc<ExecSession>, WorkspaceError> {
        if let Some(session) = self
            .sessions
            .lock()
            .expect("sessions lock")
            .get(session_id)
            .cloned()
        {
            return Ok(session);
        }
        if let Some(evicted) = self
            .evicted
            .lock()
            .expect("evicted commands lock")
            .get(session_id)
            .cloned()
        {
            return Err(WorkspaceError::ToolDetails {
                code: "COMMAND_EVICTED",
                message: format!("Command output is no longer retained: {session_id}"),
                category: "runtime",
                retryable: false,
                details: json!({
                    "command_id": evicted.command_id,
                    "command_state": "evicted",
                    "evicted_at": evicted.evicted_at,
                    "termination_reason": evicted.termination_reason,
                    "exit_code": evicted.exit_code
                }),
            });
        }
        Err(WorkspaceError::Tool {
            code: "SESSION_NOT_FOUND",
            message: format!("Command not found: {session_id}"),
            category: "not_found",
            retryable: false,
        })
    }

    pub fn remove(&self, session_id: &str) {
        let removed = self
            .sessions
            .lock()
            .expect("sessions lock")
            .remove(session_id);
        if let Some(session) = removed {
            let mut evicted = self.evicted.lock().expect("evicted commands lock");
            if evicted.len() >= MAX_EVICTED_TOMBSTONES {
                if let Some(oldest) = evicted.keys().next().cloned() {
                    evicted.remove(&oldest);
                }
            }
            evicted.insert(
                session_id.to_string(),
                EvictedCommand {
                    command_id: session_id.to_string(),
                    evicted_at: unix_timestamp(),
                    termination_reason: session
                        .termination_reason
                        .lock()
                        .expect("termination lock")
                        .clone()
                        .unwrap_or_else(|| "evicted".into()),
                    exit_code: *session.exit_code.lock().expect("exit_code lock"),
                },
            );
        }
    }

    fn session_ids(&self) -> Vec<String> {
        self.sessions
            .lock()
            .expect("sessions lock")
            .keys()
            .cloned()
            .collect()
    }
}

pub fn workspace_session_store(workspace_root: &Path) -> Arc<SessionStore> {
    let registry = WORKSPACE_SESSION_STORES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry.lock().expect("workspace session registry lock");
    registry
        .entry(workspace_root.to_path_buf())
        .or_insert_with(|| Arc::new(SessionStore::new()))
        .clone()
}

pub fn kill_workspace_sessions(workspace_root: &Path) -> usize {
    let store = WORKSPACE_SESSION_STORES
        .get()
        .and_then(|registry| {
            registry
                .lock()
                .expect("workspace session registry lock")
                .get(workspace_root)
                .cloned()
        })
        .unwrap_or_else(|| Arc::new(SessionStore::new()));

    store
        .session_ids()
        .into_iter()
        .filter(|command_id| {
            kill_session(
                &store,
                &json!({
                    "command_id": command_id,
                    "signal": "TERM",
                    "wait_ms": 1500,
                    "max_output_bytes": 1024
                }),
            )
            .is_ok()
        })
        .count()
}
