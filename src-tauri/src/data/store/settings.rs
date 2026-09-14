use crate::error::AppResult;
use crate::settings::AppSettings;

use super::super::model::AppData;
use super::DataStore;

pub(super) fn apply_settings_update(base: &AppData, latest: &mut AppData, settings: AppSettings) {
    // Global Gateway can persist a newly resolved URL outside the long-lived
    // DataStore snapshot. Unrelated settings saves must preserve that value.
    let gateway_changed_by_caller = settings.global_gateway != base.global_gateway;
    let latest_gateway = latest.global_gateway.clone();
    settings.apply_to(latest);
    if !gateway_changed_by_caller {
        latest.global_gateway = latest_gateway;
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
