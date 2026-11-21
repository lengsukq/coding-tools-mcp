use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::data::DataStore;
use crate::error::{AppError, AppResult};
use crate::mcp::gateway::GatewayState;
use crate::runtime::RuntimeSupervisor;
use crate::workspace::WorkspaceProfile;

pub struct AppState {
    pub data: Mutex<DataStore>,
    pub runtime: Mutex<RuntimeSupervisor>,
    pub gateway: Arc<GatewayState>,
    pub startup_restore_attempted: AtomicBool,
}

impl AppState {
    pub fn new() -> AppResult<Self> {
        let mut store = DataStore::load()?;
        store.init_shared_secrets()?;
        crate::tool_audit::initialize(
            store
                .data()
                .tool_audit_retention_days
                .unwrap_or(crate::tool_audit::DEFAULT_RETENTION_DAYS),
        );
        let _ = crate::lan_peer::ensure_local_identity();
        crate::lan_peer::sync_discovery_daemon();
        let gateway = Arc::new(GatewayState::default());
        Ok(Self {
            data: Mutex::new(store),
            runtime: Mutex::new(RuntimeSupervisor::new(gateway.clone())),
            gateway,
            startup_restore_attempted: AtomicBool::new(false),
        })
    }

    pub fn with_data<R>(&self, f: impl FnOnce(&mut DataStore) -> AppResult<R>) -> AppResult<R> {
        let mut guard = self
            .data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }

    pub fn with_workspaces<R>(
        &self,
        f: impl FnOnce(&mut DataStore) -> AppResult<R>,
    ) -> AppResult<R> {
        self.with_data(f)
    }

    pub fn with_settings<R>(&self, f: impl FnOnce(&mut DataStore) -> AppResult<R>) -> AppResult<R> {
        self.with_data(f)
    }

    pub fn workspace_profile(&self, id: &str) -> AppResult<WorkspaceProfile> {
        self.with_workspaces(|store| {
            store
                .get(id)
                .cloned()
                .ok_or_else(|| AppError::Message(format!("workspace not found: {id}")))
        })
    }

    pub fn workspace_path(&self, id: &str) -> AppResult<PathBuf> {
        self.with_workspaces(|store| {
            store
                .get(id)
                .map(|profile| PathBuf::from(&profile.path))
                .ok_or_else(|| AppError::Message(format!("workspace not found: {id}")))
        })
    }

    pub fn with_runtime<R>(
        &self,
        f: impl FnOnce(&mut RuntimeSupervisor) -> AppResult<R>,
    ) -> AppResult<R> {
        let mut guard = self
            .runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new().expect("failed to initialize app state")
    }
}
