use crate::error::AppResult;
use crate::settings::AppSettings;

use super::super::model::AppData;
use super::DataStore;

pub(super) fn apply_settings_update(base: &AppData, latest: &mut AppData, settings: AppSettings) {
    // Global Gateway and LAN Peer Pairing can persist runtime-negotiated state
    // outside a long-lived DataStore snapshot. Unrelated settings saves must
    // preserve those newer values instead of overwriting them.
    let gateway_changed_by_caller = settings.global_gateway != base.global_gateway;
    let lan_changed_by_caller = settings.lan_federation != base.lan_federation;
    let latest_gateway = latest.global_gateway.clone();
    let latest_lan = latest.lan_federation.clone();
    settings.apply_to(latest);
    if !gateway_changed_by_caller {
        latest.global_gateway = latest_gateway;
    }
    if !lan_changed_by_caller {
        latest.lan_federation = latest_lan;
    }
}

impl DataStore {
    pub fn settings(&self) -> AppSettings {
        AppSettings::from_data(&self.data)
    }

    pub fn update_settings(&mut self, settings: AppSettings) -> AppResult<()> {
        let base = self.data.clone();
        self.update_latest(move |data| {
            apply_settings_update(&base, data, settings);
            Ok(())
        })
    }
}
