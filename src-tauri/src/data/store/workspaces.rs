use crate::error::{AppError, AppResult};
use crate::workspace::WorkspaceProfile;

use super::DataStore;

impl DataStore {
    pub fn list(&self) -> &[WorkspaceProfile] {
        &self.data.profiles
    }

    pub fn get(&self, id: &str) -> Option<&WorkspaceProfile> {
        self.data.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn add(&mut self, profile: WorkspaceProfile) -> AppResult<()> {
        self.update_latest(move |data| {
            data.profiles.push(profile);
            Ok(())
        })
    }

    pub fn update(&mut self, profile: WorkspaceProfile) -> AppResult<()> {
        self.update_latest(move |data| {
            let Some(index) = data.profiles.iter().position(|item| item.id == profile.id) else {
                return Err(AppError::Message(format!(
                    "workspace not found: {}",
                    profile.id
                )));
            };
            data.profiles[index] = profile;
            Ok(())
        })
    }

    pub fn remove(&mut self, id: &str) -> AppResult<Option<WorkspaceProfile>> {
        self.update_latest(|data| {
            let Some(index) = data.profiles.iter().position(|item| item.id == id) else {
                return Ok(None);
            };
            let removed = data.profiles.remove(index);
            data.workspace_secrets.remove(id);
            Ok(Some(removed))
        })
    }
}
