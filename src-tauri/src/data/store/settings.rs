use crate::error::AppResult;
use crate::settings::{AppSettings, FrpProfile};

use super::super::model::AppData;
use super::DataStore;

const FRP_PROFILE_TOKEN_SCOPE: &str = "frp_profile_token";

#[cfg(test)]
pub(super) fn apply_settings_update(base: &AppData, latest: &mut AppData, settings: AppSettings) {
    let gateway_changed = settings.global_gateway != base.global_gateway;
    let lan_changed = settings.lan_federation != base.lan_federation;
    apply_settings_update_with_flags(latest, settings, gateway_changed, lan_changed);
}

fn apply_settings_update_with_flags(
    latest: &mut AppData,
    settings: AppSettings,
    gateway_changed_by_caller: bool,
    lan_changed_by_caller: bool,
) {
    // Global Gateway and LAN Peer Pairing can persist runtime-negotiated state
    // outside a long-lived DataStore snapshot. Unrelated settings saves must
    // preserve those newer values instead of overwriting them.
    let latest_gateway = (!gateway_changed_by_caller).then(|| latest.global_gateway.clone());
    let latest_lan = (!lan_changed_by_caller).then(|| latest.lan_federation.clone());
    settings.apply_to(latest);
    if let Some(gateway) = latest_gateway {
        latest.global_gateway = gateway;
    }
    if let Some(lan) = latest_lan {
        latest.lan_federation = lan;
    }
}

impl DataStore {
    pub fn settings(&self) -> AppSettings {
        AppSettings::from_data(&self.data)
    }

    pub fn update_settings(&mut self, settings: AppSettings) -> AppResult<()> {
        let gateway_changed = settings.global_gateway != self.data.global_gateway;
        let lan_changed = settings.lan_federation != self.data.lan_federation;
        self.update_latest(move |data| {
            apply_settings_update_with_flags(data, settings, gateway_changed, lan_changed);
            Ok(())
        })
    }

    /// Persist an FRP profile and its optional token in a single atomic write.
    /// Returns whether a non-empty token is stored for this profile after saving.
    pub fn save_frp_profile(
        &mut self,
        profile: FrpProfile,
        token: Option<&str>,
    ) -> AppResult<bool> {
        let trimmed_token = token
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        self.update_latest(move |data| {
            let profile_id = profile.id.clone();
            if let Some(existing) = data
                .frp_profiles
                .iter_mut()
                .find(|item| item.id == profile_id)
            {
                *existing = profile;
            } else {
                data.frp_profiles.push(profile);
            }
            if let Some(secret) = trimmed_token {
                data.app_secrets
                    .entry(FRP_PROFILE_TOKEN_SCOPE.to_string())
                    .or_default()
                    .insert(profile_id.clone(), secret);
            }
            let has_token = data
                .app_secrets
                .get(FRP_PROFILE_TOKEN_SCOPE)
                .and_then(|items| items.get(&profile_id))
                .is_some_and(|value| !value.trim().is_empty());
            Ok(has_token)
        })
    }

    /// Remove an FRP profile and its associated token in a single atomic write.
    pub fn delete_frp_profile(&mut self, id: &str) -> AppResult<()> {
        self.update_latest_if_changed(|data| {
            let before_len = data.frp_profiles.len();
            data.frp_profiles.retain(|profile| profile.id != id);
            let profile_removed = data.frp_profiles.len() != before_len;

            let mut secret_removed = false;
            if let Some(items) = data.app_secrets.get_mut(FRP_PROFILE_TOKEN_SCOPE) {
                secret_removed = items.remove(id).is_some();
                if items.is_empty() {
                    data.app_secrets.remove(FRP_PROFILE_TOKEN_SCOPE);
                }
            }
            Ok((profile_removed || secret_removed, ()))
        })
    }
}
