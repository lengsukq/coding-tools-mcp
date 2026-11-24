//! Headless-friendly workspace lifecycle helpers and parent-directory `.git`
//! repository auto-discovery (`workspace_auto_roots`).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::data::DataStore;
use crate::error::{AppError, AppResult};
use crate::util::hash::sha256_hex;
use crate::workspace::{RuntimeConfig, WorkspaceProfile};

use super::model::TunnelConfig;
use super::AuthConfig;

const AUTO_SCAN_CACHE_TTL_MS: u64 = 3_000;
const MAX_CHILDREN_PER_ROOT: usize = 512;

#[derive(Debug, Clone, Default)]
pub struct AutoDiscoverSyncReport {
    pub added: Vec<WorkspaceProfile>,
    pub total_workspaces: usize,
}

pub fn expand_user_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(trimmed));
    }
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

pub fn canonical_dir_path(raw: &str) -> AppResult<PathBuf> {
    let expanded = expand_user_path(raw);
    let resolved = expanded.canonicalize().map_err(|err| {
        AppError::Message(format!(
            "目录不存在或无法访问 ({}): {err}",
            expanded.display()
        ))
    })?;
    if !resolved.is_dir() {
        return Err(AppError::Message(format!(
            "目标路径不是有效目录: {}",
            resolved.display()
        )));
    }
    Ok(resolved)
}

/// Deterministic 32-hex workspace ID derived from canonical path so live auto-discovery
/// and persisted sync always agree on the exact same ID across local & LAN federation.
pub fn deterministic_workspace_id(canonical_path: &Path) -> String {
    let digest = sha256_hex(canonical_path.to_string_lossy().as_bytes());
    digest[..32].to_string()
}

/// Scan configured parent roots (`max_depth = 1`) for immediate child git repositories
/// not yet covered by `existing` profiles.
pub fn discover_unregistered_git_workspaces(
    roots: &[String],
    existing: &[WorkspaceProfile],
) -> Vec<WorkspaceProfile> {
    if roots.is_empty() {
        return Vec::new();
    }
    let mut known_paths: HashSet<PathBuf> = existing
        .iter()
        .map(|p| {
            let pb = expand_user_path(&p.path);
            pb.canonicalize().unwrap_or(pb)
        })
        .collect();
    let mut known_ids: HashSet<String> = existing.iter().map(|p| p.id.clone()).collect();
    let mut discovered = Vec::new();

    for raw_root in roots {
        let root_pb = expand_user_path(raw_root);
        let Ok(root) = root_pb.canonicalize() else {
            continue;
        };
        if !root.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .take(MAX_CHILDREN_PER_ROOT)
            .map(|e| e.path())
            .collect();
        children.sort();

        for child in children {
            let Some(name) = child.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.')
                || matches!(name, "node_modules" | "target" | "dist" | "build" | "venv")
            {
                continue;
            }
            let Ok(canon_child) = child.canonicalize() else {
                continue;
            };
            if !canon_child.is_dir() || !canon_child.join(".git").exists() {
                continue;
            }
            if !known_paths.insert(canon_child.clone()) {
                continue;
            }
            let id = deterministic_workspace_id(&canon_child);
            if !known_ids.insert(id.clone()) {
                continue;
            }
            let display_name = canon_child
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(name)
                .to_string();
            discovered.push(WorkspaceProfile {
                id,
                name: display_name,
                path: canon_child.to_string_lossy().into_owned(),
                tunnel: TunnelConfig::default(),
                auth: AuthConfig::default(),
                runtime: RuntimeConfig::default(),
            });
        }
    }

    discovered
}

/// Merge persisted profiles with a short-TTL cached scan of `workspace_auto_roots`
/// so newly `git clone`d directories become visible to `workspace_list` / `workspace_select`
/// immediately without requiring manual UI registration.
pub fn merge_with_cached_auto_roots(
    mut persisted: Vec<WorkspaceProfile>,
    roots: &[String],
) -> Vec<WorkspaceProfile> {
    if roots.is_empty() {
        return persisted;
    }
    struct ScanCache {
        at_ms: u64,
        roots_key: String,
        existing_len: usize,
        extra: Vec<WorkspaceProfile>,
    }
    static CACHE: OnceLock<Mutex<Option<ScanCache>>> = OnceLock::new();

    let now_ms = crate::util::time::unix_now_ms();
    let roots_key = roots.join("\n");
    let slot = CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = slot.lock() {
        if let Some(cached) = guard.as_ref() {
            if cached.roots_key == roots_key
                && cached.existing_len == persisted.len()
                && now_ms.saturating_sub(cached.at_ms) <= AUTO_SCAN_CACHE_TTL_MS
            {
                persisted.extend(cached.extra.iter().cloned());
                return persisted;
            }
        }
    }

    let extra = discover_unregistered_git_workspaces(roots, &persisted);
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(ScanCache {
            at_ms: now_ms,
            roots_key,
            existing_len: persisted.len(),
            extra: extra.clone(),
        });
    }
    persisted.extend(extra);
    persisted
}

/// Persist any newly discovered git repositories under `workspace_auto_roots` into `profiles.json`.
pub fn sync_auto_discovered_workspaces() -> AppResult<AutoDiscoverSyncReport> {
    DataStore::update_file(|data| {
        let newly_found =
            discover_unregistered_git_workspaces(&data.workspace_auto_roots, &data.profiles);
        for item in &newly_found {
            data.profiles.push(item.clone());
        }
        Ok(AutoDiscoverSyncReport {
            added: newly_found,
            total_workspaces: data.profiles.len(),
        })
    })
}

/// Add or return an existing workspace by filesystem path (idempotent on canonical path).
pub fn add_workspace_by_path(
    raw_path: &str,
    custom_name: Option<&str>,
) -> AppResult<(WorkspaceProfile, bool)> {
    let canon = canonical_dir_path(raw_path)?;
    let canon_str = canon.to_string_lossy().into_owned();
    let name_opt = custom_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    DataStore::update_file(|data| {
        if let Some(existing) = data.profiles.iter().find(|p| {
            let pb = expand_user_path(&p.path);
            pb.canonicalize().unwrap_or(pb) == canon
        }) {
            return Ok((existing.clone(), false));
        }
        let mut profile = WorkspaceProfile::new(canon_str.clone(), name_opt);
        // Use deterministic ID so CLI add and auto-discovery never collide on IDs.
        let det_id = deterministic_workspace_id(&canon);
        if !data.profiles.iter().any(|p| p.id == det_id) {
            profile.id = det_id;
        }
        data.profiles.push(profile.clone());
        Ok((profile, true))
    })
}

/// Remove a workspace by ID, exact name (when unique), or filesystem path.
pub fn remove_workspace_by_target(target: &str) -> AppResult<WorkspaceProfile> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err(AppError::Message(
            "请指定要移除的工作区 ID、名称或目录路径".into(),
        ));
    }
    let target_canon = expand_user_path(trimmed).canonicalize().ok();

    let removed = DataStore::update_file(|data| {
        let index = if let Some(idx) = data.profiles.iter().position(|p| p.id == trimmed) {
            Some(idx)
        } else if let Some(ref canon) = target_canon {
            data.profiles.iter().position(|p| {
                let pb = expand_user_path(&p.path);
                pb.canonicalize().ok().as_ref() == Some(canon)
            })
        } else {
            let matches: Vec<usize> = data
                .profiles
                .iter()
                .enumerate()
                .filter(|(_, p)| p.name == trimmed)
                .map(|(i, _)| i)
                .collect();
            if matches.len() > 1 {
                return Err(AppError::Message(format!(
                    "存在多个名为 `{trimmed}` 的工作区，请使用具体工作区 ID 或路径移除"
                )));
            }
            matches.into_iter().next()
        };

        let Some(idx) = index else {
            return Err(AppError::Message(format!("未找到匹配的工作区: {trimmed}")));
        };
        let removed = data.profiles.remove(idx);
        data.workspace_secrets.remove(&removed.id);
        Ok(removed)
    })?;

    let ws_path = PathBuf::from(&removed.path);
    let canon_ws = ws_path.canonicalize().unwrap_or(ws_path);
    crate::tools::session::kill_workspace_sessions(&canon_ws);
    crate::tools::session::drop_workspace_session_store(&canon_ws);
    Ok(removed)
}

pub fn add_auto_root(raw_path: &str) -> AppResult<(String, AutoDiscoverSyncReport)> {
    let canon = canonical_dir_path(raw_path)?;
    let canon_str = canon.to_string_lossy().into_owned();
    let report = DataStore::update_file(|data| {
        if !data.workspace_auto_roots.iter().any(|r| {
            let pb = expand_user_path(r);
            pb.canonicalize().unwrap_or(pb) == canon
        }) {
            data.workspace_auto_roots.push(canon_str.clone());
        }
        let newly_found =
            discover_unregistered_git_workspaces(&data.workspace_auto_roots, &data.profiles);
        for item in &newly_found {
            data.profiles.push(item.clone());
        }
        Ok(AutoDiscoverSyncReport {
            added: newly_found,
            total_workspaces: data.profiles.len(),
        })
    })?;
    Ok((canon_str, report))
}

pub fn remove_auto_root(raw_path: &str) -> AppResult<bool> {
    let trimmed = raw_path.trim();
    let target_canon = expand_user_path(trimmed).canonicalize().ok();
    DataStore::update_file(|data| {
        let before = data.workspace_auto_roots.len();
        data.workspace_auto_roots.retain(|entry| {
            if entry.trim() == trimmed {
                return false;
            }
            if let Some(ref tc) = target_canon {
                let pb = expand_user_path(entry);
                if pb.canonicalize().ok().as_ref() == Some(tc) {
                    return false;
                }
            }
            true
        });
        Ok(data.workspace_auto_roots.len() != before)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_immediate_git_repos_idempotently_and_preserves_manual_profiles() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_a = temp.path().join("alpha-service");
        let repo_b = temp.path().join("beta-worker");
        let plain_dir = temp.path().join("notes-not-git");
        let hidden_repo = temp.path().join(".hidden-repo");

        fs::create_dir_all(repo_a.join(".git")).expect("repo_a .git");
        fs::create_dir_all(&repo_b).expect("repo_b");
        // Simulate git worktree where `.git` is a file
        fs::write(repo_b.join(".git"), "gitdir: /tmp/somewhere").expect("worktree .git");
        fs::create_dir_all(&plain_dir).expect("plain dir");
        fs::create_dir_all(hidden_repo.join(".git")).expect("hidden .git");

        let manual_alpha = WorkspaceProfile::new(
            repo_a
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            Some("Custom Alpha".into()),
        );
        let roots = vec![temp.path().to_string_lossy().into_owned()];
        let discovered =
            discover_unregistered_git_workspaces(&roots, std::slice::from_ref(&manual_alpha));

        assert_eq!(
            discovered.len(),
            1,
            "only beta-worker should be newly discovered"
        );
        assert_eq!(discovered[0].name, "beta-worker");
        assert_eq!(discovered[0].id.len(), 32);

        // Second pass with beta included must yield zero duplicates
        let mut combined = vec![manual_alpha];
        combined.extend(discovered);
        assert!(discover_unregistered_git_workspaces(&roots, &combined).is_empty());
    }
}
