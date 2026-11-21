use std::collections::VecDeque;

use super::*;

#[derive(Debug, Clone)]
pub(super) struct EvictedCommand {
    pub(super) command_id: String,
    pub(super) evicted_at: String,
    pub(super) termination_reason: String,
    pub(super) exit_code: Option<i32>,
}

#[derive(Debug, Default)]
pub(super) struct EvictedTombstones {
    entries: HashMap<String, EvictedCommand>,
    order: VecDeque<String>,
}

impl EvictedTombstones {
    pub(super) fn insert_with_limit(
        &mut self,
        session_id: String,
        record: EvictedCommand,
        max_entries: usize,
    ) {
        if self.entries.contains_key(&session_id) {
            self.order.retain(|existing| existing != &session_id);
        } else {
            while self.entries.len() >= max_entries {
                let Some(oldest) = self.order.pop_front() else {
                    break;
                };
                self.entries.remove(&oldest);
            }
        }
        self.order.push_back(session_id.clone());
        self.entries.insert(session_id, record);
    }

    pub(super) fn get(&self, session_id: &str) -> Option<&EvictedCommand> {
        self.entries.get(session_id)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
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
        lock_unpoisoned(&self.sessions).insert(arc.session_id.clone(), arc.clone());
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
        let session = lock_unpoisoned(&self.sessions).get(session_id).cloned();
        if let Some(session) = session.as_ref() {
            session.touch_access();
        }
        session
    }

    pub(super) async fn reap_idle_completed(&self) -> usize {
        self.reap_idle_completed_with_ttl(FINISHED_COMMAND_IDLE_TTL)
            .await
    }

    pub(super) async fn reap_idle_completed_with_ttl(&self, idle_ttl: Duration) -> usize {
        // Snapshot references without locking the store during process I/O.
        let sessions = lock_unpoisoned(&self.sessions)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut to_remove = Vec::new();
        for session in &sessions {
            session.refresh_status().await;
            if session.completed_idle_for(idle_ttl) && Arc::strong_count(session) == 2 {
                to_remove.push(session.session_id.clone());
            }
        }
        let mut reaped = 0;
        for id in to_remove {
            // Recheck under lock: a concurrent reader may have touched or
            // claimed an Arc since we took our snapshot.
            let removed = {
                let mut registry = lock_unpoisoned(&self.sessions);
                let removable = registry.get(&id).is_some_and(|entry| {
                    entry.completed_idle_for(idle_ttl) && Arc::strong_count(entry) == 2
                });
                removable.then(|| registry.remove(&id)).flatten()
            };
            if let Some(session) = removed {
                self.remember_evicted(&id, &session);
                reaped += 1;
            }
        }
        reaped
    }

    fn evicted_command(&self, session_id: &str) -> Option<EvictedCommand> {
        lock_unpoisoned(&self.evicted).get(session_id).cloned()
    }

    pub fn remove(&self, session_id: &str) {
        let removed = lock_unpoisoned(&self.sessions).remove(session_id);
        if let Some(session) = removed {
            self.remember_evicted(session_id, &session);
        }
        self.output_store.cleanup_best_effort();
    }

    fn remember_evicted(&self, session_id: &str, session: &ExecSession) {
        let record = EvictedCommand {
            command_id: session_id.to_string(),
            evicted_at: unix_timestamp(),
            termination_reason: lock_unpoisoned(&session.termination_reason)
                .clone()
                .unwrap_or_else(|| "evicted".into()),
            exit_code: *lock_unpoisoned(&session.exit_code),
        };
        lock_unpoisoned(&self.evicted).insert_with_limit(
            session_id.to_string(),
            record,
            MAX_EVICTED_TOMBSTONES,
        );
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
        lock_unpoisoned(&self.sessions).keys().cloned().collect()
    }
}

pub fn workspace_session_store(workspace_root: &Path) -> Arc<SessionStore> {
    let registry = WORKSPACE_SESSION_STORES.get_or_init(|| Mutex::new(HashMap::new()));
    let (store, created) = {
        let mut guard = lock_unpoisoned(registry);
        if let Some(existing) = guard.get(workspace_root) {
            (Arc::clone(existing), false)
        } else {
            let created_store = Arc::new(SessionStore::for_workspace(workspace_root));
            guard.insert(workspace_root.to_path_buf(), Arc::clone(&created_store));
            (created_store, true)
        }
    };
    if created {
        store.output_store.cleanup_best_effort();
    }
    store
}

pub fn kill_workspace_sessions(workspace_root: &Path) -> usize {
    let Some(store) = WORKSPACE_SESSION_STORES
        .get()
        .and_then(|registry| lock_unpoisoned(registry).get(workspace_root).cloned())
    else {
        return 0;
    };

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

pub fn drop_workspace_session_store(workspace_root: &Path) {
    if let Some(registry) = WORKSPACE_SESSION_STORES.get() {
        let mut guard = lock_unpoisoned(registry);
        guard.remove(workspace_root);
        if guard.capacity() > 32 && guard.len() * 4 < guard.capacity() {
            guard.shrink_to_fit();
        }
    }
}
