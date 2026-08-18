use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppResult;

use super::{PlanningState, PLANNING_RELATIVE_PATH};

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

    pub fn load(&self) -> AppResult<PlanningState> {
        if !self.path.exists() {
            return Ok(PlanningState::default());
        }
        let raw = fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self, state: &PlanningState) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(state)?;
        fs::write(&self.path, format!("{raw}\n"))?;
        Ok(())
    }

    pub fn update<R>(&self, mutate: impl FnOnce(&mut PlanningState) -> AppResult<R>) -> AppResult<R> {
        let mut state = self.load()?;
        state.revision = state.revision.saturating_add(1);
        let result = mutate(&mut state)?;
        self.save(&state)?;
        Ok(result)
    }
}
