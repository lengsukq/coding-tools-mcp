use std::hash::{Hash, Hasher};
use std::sync::Arc;

use serde::Serialize;

use crate::data::DataStore;
use crate::lan_peer::PeerDeviceMetaDto;
use crate::workspace::WorkspaceProfile;

use super::GatewayError;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDescriptor {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<PeerDeviceMetaDto>,
}

impl From<&WorkspaceProfile> for WorkspaceDescriptor {
    fn from(profile: &WorkspaceProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            path: profile.path.clone(),
            qualified_name: None,
            device: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceRegistry {
    profiles_override: Option<Arc<Vec<WorkspaceProfile>>>,
}

impl WorkspaceRegistry {
    pub fn list(&self) -> Result<Vec<WorkspaceProfile>, String> {
        if let Some(profiles) = &self.profiles_override {
            return Ok(profiles.as_ref().clone());
        }
        let (profiles, auto_roots) = DataStore::read_file(|data| {
            Ok((data.profiles.clone(), data.workspace_auto_roots.clone()))
        })
        .map_err(|error| error.to_string())?;
        Ok(crate::workspace::auto_discover::merge_with_cached_auto_roots(profiles, &auto_roots))
    }

    pub fn descriptors(&self) -> Result<Vec<WorkspaceDescriptor>, String> {
        let mut items = self
            .list()?
            .iter()
            .map(WorkspaceDescriptor::from)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        Ok(items)
    }

    pub fn resolve(&self, workspace_id: &str) -> Result<WorkspaceProfile, GatewayError> {
        self.list()
            .map_err(|message| GatewayError::internal("WORKSPACE_REGISTRY_UNAVAILABLE", message))?
            .into_iter()
            .find(|profile| profile.id == workspace_id)
            .ok_or_else(|| {
                GatewayError::not_found(
                    "WORKSPACE_NOT_FOUND",
                    format!("Workspace 不存在或已被删除: {workspace_id}"),
                )
            })
    }

    /// Compute a deterministic revision hash directly from an already-fetched
    /// descriptor slice without triggering an extra `DataStore::read_file`.
    pub fn revision_for(descriptors: &[WorkspaceDescriptor]) -> u64 {
        let mut ordered: Vec<&WorkspaceDescriptor> = descriptors.iter().collect();
        ordered.sort_by(|left, right| left.id.cmp(&right.id));
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for item in ordered {
            item.id.hash(&mut hasher);
            item.name.hash(&mut hasher);
            item.path.hash(&mut hasher);
        }
        hasher.finish()
    }

    pub fn revision(&self) -> Result<u64, String> {
        let items = self.descriptors()?;
        Ok(Self::revision_for(&items))
    }

    #[cfg(test)]
    pub(crate) fn from_profiles(profiles: Vec<WorkspaceProfile>) -> Self {
        Self {
            profiles_override: Some(Arc::new(profiles)),
        }
    }
}
