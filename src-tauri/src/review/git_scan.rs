use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::platform::{background_command, platform};
use crate::tools::git::{run_git_bytes, terminate_child};

use super::diff::{review_path_excluded, MAX_FILE_BYTES};

const REVIEW_GIT_TIMEOUT: Duration = Duration::from_secs(10);
const REVIEW_GIT_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const REVIEW_GIT_BATCH_TIMEOUT: Duration = Duration::from_secs(20);
const REVIEW_GIT_MAX_STDOUT_BYTES: usize = 8 * 1024 * 1024;
const REVIEW_GIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub fn direct_git_repositories(workspace_root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(workspace_root) else {
        return Vec::new();
    };
    let mut repositories = entries
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if review_path_excluded(&name) {
                return None;
            }
            let path = entry.path();
            is_git_repository_root(&path).then_some((name, path))
        })
        .collect::<Vec<_>>();
    repositories.sort_by(|left, right| left.0.cmp(&right.0));
    repositories
}

pub(super) fn git_repo_changes(
    repo_root: &Path,
    ignored_prefixes: &[String],
) -> Result<Vec<(String, String, String)>, String> {
    let output = run_git_bytes(
        repo_root,
        &["diff", "--name-status", "-z", "-M", "HEAD", "--", "."],
        REVIEW_GIT_TIMEOUT,
        REVIEW_GIT_MAX_STDOUT_BYTES,
    )?;
    if !output.success {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let fields = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let mut changes = Vec::<(String, String, String)>::new();
    let mut i = 0;
    while i < fields.len() {
        let status = String::from_utf8_lossy(fields[i]).to_string();
        i += 1;
        if status.starts_with('R') || status.starts_with('C') {
            if i + 1 >= fields.len() {
                break;
            }
            let old = String::from_utf8_lossy(fields[i]).to_string();
            let new = String::from_utf8_lossy(fields[i + 1]).to_string();
            i += 2;
            changes.push((status, old, new));
        } else {
            if i >= fields.len() {
                break;
            }
            let path = String::from_utf8_lossy(fields[i]).to_string();
            i += 1;
            changes.push((status, path.clone(), path));
        }
    }
    let untracked = run_git_bytes(
        repo_root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        REVIEW_GIT_TIMEOUT,
        REVIEW_GIT_MAX_STDOUT_BYTES,
    )?;
    if untracked.success {
        for raw in untracked
            .stdout
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
        {
            let path = String::from_utf8_lossy(raw).to_string();
            changes.push(("A".into(), path.clone(), path));
        }
    }
    Ok(changes
        .into_iter()
        .filter(|(_, old_path, new_path)| {
            !review_path_excluded(old_path)
                && !review_path_excluded(new_path)
                && !ignored_prefixes.iter().any(|prefix| {
                    old_path == prefix
                        || old_path.starts_with(&format!("{prefix}/"))
                        || new_path == prefix
                        || new_path.starts_with(&format!("{prefix}/"))
                })
        })
        .collect())
}

pub(super) fn prefixed_review_path(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        path.to_string()
    } else {
        format!("{prefix}/{path}")
    }
}

pub(super) fn is_git_repository_root(path: &Path) -> bool {
    // Most first-level directories are NOT Git repositories. Avoid spawning a
    // Git process for each of them on every workspace status refresh.
    // Linked worktrees use a .git file, and bare repositories have HEAD and
    // objects directly at the root. Both should remain discoverable.
    if !(path.join(".git").exists()
        || (path.join("HEAD").is_file() && path.join("objects").is_dir()))
    {
        return false;
    }
    let Ok(output) = run_git_bytes(
        path,
        &["rev-parse", "--show-toplevel"],
        REVIEW_GIT_PROBE_TIMEOUT,
        64 * 1024,
    ) else {
        return false;
    };
    if !output.success {
        return false;
    }
    let reported = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
    match (reported.canonicalize(), path.canonicalize()) {
        (Ok(reported), Ok(actual)) => reported == actual,
        _ => false,
    }
}

/// Read multiple blob contents at `HEAD` in a single `git cat-file --batch`
/// invocation to avoid spawning one `git show` subprocess per changed file.
pub(super) fn git_blobs_at_head(root: &Path, paths: &[&str]) -> HashMap<String, Vec<u8>> {
    let mut blobs = HashMap::new();
    let mut seen = HashSet::new();
    let mut batch_paths = Vec::with_capacity(paths.len());

    for &path in paths {
        if !seen.insert(path) {
            continue;
        }
        // Paths containing raw line breaks cannot use the line-delimited batch protocol.
        if path.contains('\n') || path.contains('\r') {
            blobs.insert(path.to_string(), git_blob_at_head(root, path));
        } else {
            batch_paths.push(path);
        }
    }

    if batch_paths.is_empty() {
        return blobs;
    }

    let Ok(mut child) = background_command("git")
        .args(["cat-file", "--batch"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return blobs;
    };

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.wait();
        return blobs;
    };
    let Some(stdout) = child.stdout.take() else {
        drop(stdin);
        let _ = child.wait();
        return blobs;
    };

    // Feed stdin concurrently while draining stdout to avoid OS pipe buffer deadlocks.
    // A bounded watchdog terminates stalled `git cat-file` processes so review generation never hangs.
    let child_pid = child.id();
    let batch_done = Arc::new(AtomicBool::new(false));
    let watchdog_done = Arc::clone(&batch_done);
    let watchdog = thread::spawn(move || {
        let deadline = Instant::now() + REVIEW_GIT_BATCH_TIMEOUT;
        while !watchdog_done.load(Ordering::Acquire) {
            if Instant::now() >= deadline {
                let _ = platform().terminate_process_tree(child_pid);
                break;
            }
            thread::sleep(
                REVIEW_GIT_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    });

    let batch_paths_ref = batch_paths.as_slice();
    thread::scope(|scope| {
        let writer = scope.spawn(move || {
            for path in batch_paths_ref {
                if writeln!(stdin, "HEAD:{path}").is_err() {
                    break;
                }
            }
            let _ = stdin.flush();
        });

        let mut reader = BufReader::new(stdout);
        let mut header_buf = Vec::new();
        for &path in batch_paths_ref {
            header_buf.clear();
            let Ok(read) = reader.read_until(b'\n', &mut header_buf) else {
                break;
            };
            if read == 0 {
                break;
            }
            let header = String::from_utf8_lossy(&header_buf);
            let Some((obj_type, size)) =
                parse_cat_file_header(header.trim_end_matches(['\r', '\n']))
            else {
                continue;
            };
            let Ok(content) = read_cat_file_body(&mut reader, size) else {
                break;
            };
            if obj_type == "blob" {
                blobs.insert(path.to_string(), content);
            }
        }

        drop(reader);
        let _ = writer.join();
    });

    batch_done.store(true, Ordering::Release);
    let _ = watchdog.join();
    if child.try_wait().ok().flatten().is_none() {
        terminate_child(&mut child);
    }
    blobs
}

fn parse_cat_file_header(header: &str) -> Option<(&str, u64)> {
    let mut parts = header.split(' ');
    let oid = parts.next()?;
    let obj_type = parts.next()?;
    let size = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if oid.len() < 40 || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    if !matches!(obj_type, "blob" | "tree" | "commit" | "tag") {
        return None;
    }
    let size = size.parse::<u64>().ok()?;
    Some((obj_type, size))
}

fn read_cat_file_body<R: Read>(reader: &mut R, size: u64) -> io::Result<Vec<u8>> {
    // Retain at most MAX_FILE_BYTES + 1 bytes so oversized blobs still trigger
    // "File too large to render" redaction without allocating unbounded memory.
    let capture_limit = (MAX_FILE_BYTES + 1) as u64;
    let capture_len = size.min(capture_limit) as usize;
    let mut content = vec![0u8; capture_len];
    reader.read_exact(&mut content)?;

    let remaining = size.saturating_sub(capture_len as u64);
    if remaining > 0 {
        io::copy(&mut reader.by_ref().take(remaining), &mut io::sink())?;
    }

    // Consume the single LF separator emitted by `git cat-file --batch` after each object.
    let mut separator = [0u8; 1];
    reader.read_exact(&mut separator)?;
    Ok(content)
}

fn git_blob_at_head(root: &Path, path: &str) -> Vec<u8> {
    let spec = format!("HEAD:{path}");
    run_git_bytes(
        root,
        &["show", spec.as_str()],
        REVIEW_GIT_TIMEOUT,
        MAX_FILE_BYTES + 1,
    )
    .ok()
    .filter(|output| output.success)
    .map(|output| output.stdout)
    .unwrap_or_default()
}

pub(super) fn git_head(root: &Path) -> Option<String> {
    run_git_bytes(
        root,
        &["rev-parse", "HEAD"],
        REVIEW_GIT_PROBE_TIMEOUT,
        64 * 1024,
    )
    .ok()
    .filter(|out| out.success)
    .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
}
