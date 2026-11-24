use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::error::AppResult;
use crate::platform::platform;

use super::migrate::{
    data_file_path, load_for_store, load_or_migrate_at, maybe_backup_legacy_files, write_data,
};
use super::model::AppData;

mod secrets;
mod settings;
mod workspaces;

static DATA_FILE_LOCK: Mutex<()> = Mutex::new(());
/// Path-keyed process cache so concurrent tests with isolated temp paths never pollute each other,
/// while production hot-path reads (`read_file`, `AppSettings::load_or_default`, `SecretStore`)
/// hit `Arc<AppData>` with zero JSON re-parsing and automatically invalidate when another
/// process (`coding-tools-mcp` CLI vs desktop/daemon) atomically updates `profiles.json`.
static CACHED_DATA: OnceLock<RwLock<HashMap<PathBuf, CachedDataEntry>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    ino: u64,
}

impl FileStamp {
    fn from_path(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            len: meta.len(),
            modified: meta.modified().ok(),
            #[cfg(unix)]
            ino: {
                use std::os::unix::fs::MetadataExt;
                meta.ino()
            },
        })
    }
}

#[derive(Debug, Clone)]
struct CachedDataEntry {
    stamp: Option<FileStamp>,
    data: Arc<AppData>,
}

pub(crate) const SHARED_KEYS: &[&str] = &[
    "oauth_client_id",
    "bearer_token",
    "oauth_client_secret",
    "oauth_password",
    "oauth_token_secret",
];

#[derive(Debug)]
pub struct DataStore {
    path: PathBuf,
    data: AppData,
}

fn data_cache() -> &'static RwLock<HashMap<PathBuf, CachedDataEntry>> {
    CACHED_DATA.get_or_init(|| RwLock::new(HashMap::new()))
}

fn cache_get(path: &Path) -> Option<Arc<AppData>> {
    let guard = data_cache()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = guard.get(path)?;
    match (entry.stamp, FileStamp::from_path(path)) {
        (Some(cached), Some(disk)) if cached != disk => None,
        (None, Some(_)) => None,
        _ => Some(Arc::clone(&entry.data)),
    }
}

fn cache_put(path: &Path, data: Arc<AppData>) {
    let stamp = FileStamp::from_path(path);
    let mut guard = data_cache()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.insert(path.to_path_buf(), CachedDataEntry { stamp, data });
}

fn lock_data_file() -> std::sync::MutexGuard<'static, ()> {
    DATA_FILE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Acquire a cross-process advisory file lock beside `profiles.json` so desktop and
/// headless CLI processes serialize read-modify-write updates without lost writes.
fn lock_cross_process(path: &Path) -> AppResult<crate::util::fs::FileLock> {
    let lock_path = path.with_extension("lock");
    Ok(crate::util::fs::lock_file(&lock_path)?)
}

fn app_root_for_path(path: &Path) -> AppResult<PathBuf> {
    if let Some(root) = path.parent().and_then(Path::parent) {
        if !root.as_os_str().is_empty() {
            return Ok(root.to_path_buf());
        }
    }
    platform().app_config_dir()
}

impl DataStore {
    pub fn load() -> AppResult<Self> {
        let _guard = lock_data_file();
        let path = data_file_path()?;
        let _file_lock = lock_cross_process(&path)?;
        let (path, loaded) = load_for_store()?;
        if !loaded.existed_before || loaded.needs_persist {
            write_data(&path, &loaded.data)?;
        }
        if !loaded.existed_before {
            maybe_backup_legacy_files(&path)?;
        }
        cache_put(&path, Arc::new(loaded.data.clone()));
        Ok(Self {
            path,
            data: loaded.data,
        })
    }

    pub fn read_file<R>(f: impl FnOnce(&AppData) -> AppResult<R>) -> AppResult<R> {
        let path = data_file_path()?;
        Self::read_file_at(&path, true, f)
    }

    pub fn update_file<R>(f: impl FnOnce(&mut AppData) -> AppResult<R>) -> AppResult<R> {
        Self::update_file_if_changed(|data| f(data).map(|result| (true, result)))
    }

    /// Mutate `AppData` under the file lock and write to disk only when `f` reports `changed == true`.
    pub fn update_file_if_changed<R>(
        f: impl FnOnce(&mut AppData) -> AppResult<(bool, R)>,
    ) -> AppResult<R> {
        let path = data_file_path()?;
        Self::update_file_if_changed_at(&path, true, f)
    }

    pub fn data(&self) -> &AppData {
        &self.data
    }

    fn read_file_at<R>(
        path: &Path,
        allow_legacy_home_import: bool,
        f: impl FnOnce(&AppData) -> AppResult<R>,
    ) -> AppResult<R> {
        if let Some(cached) = cache_get(path) {
            return f(cached.as_ref());
        }
        let snapshot = {
            let _guard = lock_data_file();
            if let Some(cached) = cache_get(path) {
                cached
            } else {
                let app_root = app_root_for_path(path)?;
                let loaded = load_or_migrate_at(&app_root, path, allow_legacy_home_import)?;
                let arc = Arc::new(loaded);
                cache_put(path, Arc::clone(&arc));
                arc
            }
        };
        f(snapshot.as_ref())
    }

    fn update_file_if_changed_at<R>(
        path: &Path,
        allow_legacy_home_import: bool,
        f: impl FnOnce(&mut AppData) -> AppResult<(bool, R)>,
    ) -> AppResult<R> {
        let _guard = lock_data_file();
        let _file_lock = lock_cross_process(path)?;
        let (mut data, was_cached) = if let Some(cached) = cache_get(path) {
            ((*cached).clone(), true)
        } else {
            let app_root = app_root_for_path(path)?;
            (
                load_or_migrate_at(&app_root, path, allow_legacy_home_import)?,
                false,
            )
        };
        let (changed, result) = f(&mut data)?;
        if changed {
            write_data(path, &data)?;
            cache_put(path, Arc::new(data));
        } else if !was_cached {
            cache_put(path, Arc::new(data));
        }
        Ok(result)
    }

    fn update_latest<R>(&mut self, f: impl FnOnce(&mut AppData) -> AppResult<R>) -> AppResult<R> {
        self.update_latest_if_changed(|data| f(data).map(|result| (true, result)))
    }

    fn update_latest_if_changed<R>(
        &mut self,
        f: impl FnOnce(&mut AppData) -> AppResult<(bool, R)>,
    ) -> AppResult<R> {
        let _guard = lock_data_file();
        let _file_lock = lock_cross_process(&self.path)?;
        let (mut data, was_cached) = if let Some(cached) = cache_get(&self.path) {
            ((*cached).clone(), true)
        } else {
            let app_root = app_root_for_path(&self.path)?;
            (load_or_migrate_at(&app_root, &self.path, true)?, false)
        };
        let (changed, result) = f(&mut data)?;
        if changed {
            write_data(&self.path, &data)?;
            cache_put(&self.path, Arc::new(data.clone()));
        } else if !was_cached {
            cache_put(&self.path, Arc::new(data.clone()));
        }
        self.data = data;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::super::migrate::{contains_obsolete_actions_json, strip_obsolete_actions_secrets};
    use super::secrets::shared_value_for_key;
    use super::settings::apply_settings_update;
    use super::*;
    use crate::settings::AppSettings;

    #[test]
    fn cache_serves_reads_without_re_reading_disk_and_refreshes_on_write() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("data").join("profiles.json");
        let initial = AppData {
            last_workspace_id: "ws-initial".into(),
            ..AppData::default()
        };
        write_data(&path, &initial).expect("seed file");

        let first = DataStore::read_file_at(&path, false, |d| Ok(d.last_workspace_id.clone()))
            .expect("initial read");
        assert_eq!(first, "ws-initial");

        // Remove the underlying file: a cached read must succeed purely from RAM.
        std::fs::remove_file(&path).expect("remove underlying file");
        let cached = DataStore::read_file_at(&path, false, |d| Ok(d.last_workspace_id.clone()))
            .expect("cached read should not hit disk");
        assert_eq!(cached, "ws-initial");

        // Conditional write with changed == false must not recreate the deleted file.
        DataStore::update_file_if_changed_at(&path, false, |_| Ok((false, ())))
            .expect("no-op update");
        assert!(!path.exists(), "unchanged update must skip disk write");

        // Conditional write with changed == true writes to disk and refreshes cache.
        DataStore::update_file_if_changed_at(&path, false, |data| {
            data.last_workspace_id = "ws-updated".into();
            Ok((true, ()))
        })
        .expect("changed update");
        assert!(path.exists(), "changed update must persist to disk");
        let after = DataStore::read_file_at(&path, false, |d| Ok(d.last_workspace_id.clone()))
            .expect("read after write");
        assert_eq!(after, "ws-updated");
    }

    #[test]
    fn external_disk_write_invalidates_stale_process_cache() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("data").join("profiles.json");
        let initial = AppData {
            last_workspace_id: "ws-v1".into(),
            ..AppData::default()
        };
        write_data(&path, &initial).expect("seed v1");
        let v1 = DataStore::read_file_at(&path, false, |d| Ok(d.last_workspace_id.clone()))
            .expect("read v1");
        assert_eq!(v1, "ws-v1");

        // Simulate an external CLI process (`coding-tools-mcp lan pair` / `workspace add`)
        // atomically replacing `profiles.json` on disk without touching this process's cache_put.
        let external = AppData {
            last_workspace_id: "ws-v2".into(),
            ..AppData::default()
        };
        write_data(&path, &external).expect("external write v2");
        let v2 = DataStore::read_file_at(&path, false, |d| Ok(d.last_workspace_id.clone()))
            .expect("read v2 after external atomic replace");
        assert_eq!(v2, "ws-v2");
    }

    #[test]
    fn init_shared_secrets_writes_only_when_keys_are_missing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("data").join("profiles.json");
        let initial = AppData::default();
        write_data(&path, &initial).expect("seed file");
        cache_put(&path, Arc::new(initial.clone()));

        let mut store = DataStore {
            path: path.clone(),
            data: initial,
        };
        store.init_shared_secrets().expect("populate missing keys");
        for key in SHARED_KEYS {
            assert!(store.get_shared_secret(key).is_some());
        }

        // Remove the file on disk to prove the second call performs zero disk writes.
        std::fs::remove_file(&path).expect("remove file");
        store
            .init_shared_secrets()
            .expect("idempotent init should succeed without writing");
        assert!(
            !path.exists(),
            "init_shared_secrets must skip disk write when all keys already exist"
        );
    }

    #[test]
    fn settings_update_preserves_newer_global_oauth_registry() {
        let base = AppData::default();
        let marker = "{\"client\":\"preserved\"}";
        let mut latest = base.clone();
        latest
            .app_secrets
            .entry("global_mcp".into())
            .or_default()
            .insert("oauth_dynamic_clients".into(), marker.into());

        let mut settings = AppSettings::from_data(&base);
        settings.last_workspace_id = "workspace-2".into();
        apply_settings_update(&base, &mut latest, settings);

        assert_eq!(latest.last_workspace_id, "workspace-2");
        assert_eq!(
            latest
                .app_secrets
                .get("global_mcp")
                .and_then(|items| items.get("oauth_dynamic_clients"))
                .map(String::as_str),
            Some(marker)
        );
    }

    #[test]
    fn unrelated_settings_update_preserves_newer_global_gateway_state() {
        let base = AppData::default();
        let mut latest = base.clone();
        latest.global_gateway.public_url = "https://runtime.example.test".into();

        let mut settings = AppSettings::from_data(&base);
        settings.last_workspace_id = "workspace-2".into();
        apply_settings_update(&base, &mut latest, settings);

        assert_eq!(latest.last_workspace_id, "workspace-2");
        assert_eq!(
            latest.global_gateway.public_url,
            "https://runtime.example.test"
        );
    }

    #[test]
    fn explicit_global_gateway_update_still_wins_over_runtime_snapshot() {
        let base = AppData::default();
        let mut latest = base.clone();
        latest.global_gateway.public_url = "https://runtime.example.test".into();

        let mut settings = AppSettings::from_data(&base);
        settings.global_gateway.enabled = true;
        settings.global_gateway.public_url = "https://configured.example.test".into();
        apply_settings_update(&base, &mut latest, settings);

        assert!(latest.global_gateway.enabled);
        assert_eq!(
            latest.global_gateway.public_url,
            "https://configured.example.test"
        );
    }

    #[test]
    fn shared_oauth_client_id_uses_client_id_format() {
        let value = shared_value_for_key("oauth_client_id");
        assert!(value.starts_with("chatgpt-client-"));
        assert_eq!(value.len(), "chatgpt-client-".len() + 12);
    }

    #[test]
    fn obsolete_actions_data_is_detected_and_removed_without_touching_mcp_secrets() {
        let legacy = serde_json::json!({
            "restore_actions_workspace_ids": ["workspace-1"],
            "profiles": [{"id": "workspace-1", "actions": {"local_port": 8787}}],
            "shared_secrets": {
                "bearer_token": "keep-shared",
                "actions_api_key": "remove-shared"
            },
            "workspace_secrets": {
                "workspace-1": {
                    "oauth_password": "keep-workspace",
                    "actions_oauth_dynamic_clients": "remove-workspace"
                }
            }
        });
        assert!(contains_obsolete_actions_json(&legacy));

        let mut data = AppData::default();
        data.shared_secrets
            .insert("bearer_token".into(), "keep-shared".into());
        data.shared_secrets
            .insert("actions_api_key".into(), "remove-shared".into());
        data.workspace_secrets.insert(
            "workspace-1".into(),
            std::collections::HashMap::from([
                ("oauth_password".into(), "keep-workspace".into()),
                (
                    "actions_oauth_dynamic_clients".into(),
                    "remove-workspace".into(),
                ),
            ]),
        );

        assert!(strip_obsolete_actions_secrets(&mut data));
        assert_eq!(
            data.shared_secrets.get("bearer_token").map(String::as_str),
            Some("keep-shared")
        );
        assert!(!data.shared_secrets.contains_key("actions_api_key"));
        let workspace = data
            .workspace_secrets
            .get("workspace-1")
            .expect("workspace secrets");
        assert_eq!(
            workspace.get("oauth_password").map(String::as_str),
            Some("keep-workspace")
        );
        assert!(!workspace.contains_key("actions_oauth_dynamic_clients"));
    }

    #[test]
    fn unrelated_settings_update_preserves_newer_upstream_mcp_config() {
        use crate::upstream_mcp::{
            UpstreamExposeMode, UpstreamServerConfig, UpstreamTransportConfig,
        };
        let base = AppData::default();
        let mut latest = base.clone();
        latest.upstream_mcp.servers.push(UpstreamServerConfig {
            id: "up-1".into(),
            alias: "demo".into(),
            name: "Demo".into(),
            description: String::new(),
            enabled: true,
            expose_mode: UpstreamExposeMode::Proxy,
            transport: UpstreamTransportConfig::StreamableHttp {
                url: "https://demo.example.com/mcp".into(),
                headers: Default::default(),
                secret_header_keys: Vec::new(),
                has_bearer_token: false,
                use_proxy: false,
            },
            disabled_tools: Vec::new(),
            call_timeout_secs: 60,
            idle_timeout_secs: 900,
            source: "manual".into(),
        });

        let mut settings = AppSettings::from_data(&base);
        settings.last_workspace_id = "workspace-9".into();
        apply_settings_update(&base, &mut latest, settings);

        assert_eq!(latest.last_workspace_id, "workspace-9");
        assert_eq!(latest.upstream_mcp.servers.len(), 1);
        assert_eq!(latest.upstream_mcp.servers[0].alias, "demo");
    }

    #[test]
    fn save_and_delete_upstream_server_atomically_manage_secrets() {
        use crate::upstream_mcp::{
            env_secret_item_id, UpstreamExposeMode, UpstreamSecretPatch, UpstreamServerConfig,
            UpstreamTransportConfig, UPSTREAM_ENV_SECRETS_SCOPE,
        };
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("data").join("profiles.json");
        let initial = AppData::default();
        write_data(&path, &initial).expect("seed file");
        cache_put(&path, Arc::new(initial.clone()));

        let mut store = DataStore {
            path: path.clone(),
            data: initial,
        };
        let server = UpstreamServerConfig {
            id: "up-secret-test".into(),
            alias: "gh".into(),
            name: "GitHub".into(),
            description: String::new(),
            enabled: true,
            expose_mode: UpstreamExposeMode::Direct,
            transport: UpstreamTransportConfig::Stdio {
                command: "npx".into(),
                args: vec!["-y".into(), "demo".into()],
                cwd: None,
                env: std::collections::BTreeMap::from([
                    ("PLAIN".into(), "1".into()),
                    ("GITHUB_TOKEN".into(), "must-be-stripped-from-plain".into()),
                ]),
                secret_env_keys: Vec::new(),
            },
            disabled_tools: Vec::new(),
            call_timeout_secs: 60,
            idle_timeout_secs: 900,
            source: "manual".into(),
        };
        let mut patch = UpstreamSecretPatch::default();
        patch
            .set_env_secrets
            .insert("GITHUB_TOKEN".into(), "ghp_real_secret".into());

        let saved = store.save_upstream_server(server, patch).expect("save");
        match &saved.transport {
            UpstreamTransportConfig::Stdio {
                env,
                secret_env_keys,
                ..
            } => {
                assert_eq!(env.get("PLAIN").map(String::as_str), Some("1"));
                assert!(
                    !env.contains_key("GITHUB_TOKEN"),
                    "secret key must never duplicate in plaintext env"
                );
                assert_eq!(secret_env_keys, &vec!["GITHUB_TOKEN".to_string()]);
            }
            _ => panic!("expected stdio"),
        }
        assert_eq!(
            store
                .get_app_secret(
                    UPSTREAM_ENV_SECRETS_SCOPE,
                    &env_secret_item_id("up-secret-test", "GITHUB_TOKEN")
                )
                .as_deref(),
            Some("ghp_real_secret")
        );

        assert!(store
            .delete_upstream_server("up-secret-test")
            .expect("delete"));
        assert!(store.data.upstream_mcp.servers.is_empty());
        assert!(store
            .get_app_secret(
                UPSTREAM_ENV_SECRETS_SCOPE,
                &env_secret_item_id("up-secret-test", "GITHUB_TOKEN")
            )
            .is_none());
    }
}
