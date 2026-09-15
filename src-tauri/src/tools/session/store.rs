use super::*;

#[derive(Debug, Clone)]
pub(super) struct EvictedCommand {
    pub(super) command_id: String,
    pub(super) evicted_at: String,
    pub(super) termination_reason: String,
    pub(super) exit_code: Option<i32>,
}

fn evicted_command_error(session_id: &str, evicted: EvictedCommand) -> WorkspaceError {
    WorkspaceError::ToolDetails {
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
    }
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
    pub fn insert(&self, mut session: ExecSession) -> Arc<ExecSession> {
        session.attach_output_store(self.output_store.clone());
        let arc = Arc::new(session);
        self.sessions
            .lock()
            .expect("sessions lock")
            .insert(arc.session_id.clone(), arc.clone());
        arc
    }

    pub fn get(&self, session_id: &str) -> Result<Arc<ExecSession>, WorkspaceError> {
        if let Some(session) = self.active_session(session_id) {
            return Ok(session);
        }
        if let Some(evicted) = self.evicted_command(session_id) {
            return Err(evicted_command_error(session_id, evicted));
        }
        Err(WorkspaceError::Tool {
            code: "SESSION_NOT_FOUND",
            message: format!("Command not found: {session_id}"),
            category: "not_found",
            retryable: false,
        })
    }

    pub(super) fn output_read_session(
        &self,
        session_id: &str,
    ) -> Result<Option<Arc<ExecSession>>, WorkspaceError> {
        if let Some(session) = self.active_session(session_id) {
            return Ok(Some(session));
        }
        if let Some(evicted) = self.evicted_command(session_id) {
            if evicted.termination_reason == "exited" {
                return Ok(None);
            }
            return Err(evicted_command_error(session_id, evicted));
        }
        Err(WorkspaceError::Tool {
            code: "SESSION_NOT_FOUND",
            message: format!("Command not found: {session_id}"),
            category: "not_found",
            retryable: false,
        })
    }

    fn active_session(&self, session_id: &str) -> Option<Arc<ExecSession>> {
        self.sessions
            .lock()
            .expect("sessions lock")
            .get(session_id)
            .cloned()
    }

    fn evicted_command(&self, session_id: &str) -> Option<EvictedCommand> {
        self.evicted
            .lock()
            .expect("evicted commands lock")
            .get(session_id)
            .cloned()
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
        self.output_store.cleanup_best_effort();
    }

    pub(super) fn full_output_page(
        &self,
        command_id: &str,
        stream: &str,
        offset: usize,
        limit: usize,
    ) -> std::io::Result<Option<super::output_store::FullOutputPage>> {
        self.output_store
            .read_page(command_id, stream, offset, limit)
    }

    pub(super) fn search_full_output(
        &self,
        command_id: &str,
        stream: &str,
        query: &str,
        regex: bool,
        case_sensitive: bool,
        max_matches: usize,
    ) -> std::io::Result<Option<Value>> {
        self.output_store.search(
            command_id,
            stream,
            query,
            regex,
            case_sensitive,
            max_matches,
        )
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
        .or_insert_with(|| Arc::new(SessionStore::for_workspace(workspace_root)))
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
        .unwrap_or_else(|| Arc::new(SessionStore::for_workspace(workspace_root)));

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
