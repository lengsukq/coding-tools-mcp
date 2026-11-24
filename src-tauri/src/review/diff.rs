use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use similar::{ChangeTag, TextDiff};

use crate::util::fs::is_default_excluded_os;
use crate::util::text::contains_ascii_ignore_case;

use super::store::ChangeFile;

pub(super) const MAX_FILE_BYTES: usize = 512 * 1024;

fn is_review_extra_excluded_os(name: &OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(".coding-tools" | ".local-build" | ".scratch")
    )
}

fn is_review_excluded_os(name: &OsStr) -> bool {
    is_default_excluded_os(name) || is_review_extra_excluded_os(name)
}

fn starts_with_ascii_ignore_case(value: &str, prefix: &str) -> bool {
    value.len() >= prefix.len()
        && value.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn ends_with_ascii_ignore_case(value: &str, suffix: &str) -> bool {
    value.len() >= suffix.len()
        && value.as_bytes()[value.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes())
}

fn is_sensitive_component(part: &str) -> bool {
    part.eq_ignore_ascii_case(".env")
        || starts_with_ascii_ignore_case(part, ".env.")
        || contains_ascii_ignore_case(part, "credential")
        || contains_ascii_ignore_case(part, "secret")
        || ends_with_ascii_ignore_case(part, ".pem")
        || ends_with_ascii_ignore_case(part, ".key")
        || ends_with_ascii_ignore_case(part, "id_rsa")
        || ends_with_ascii_ignore_case(part, "id_ed25519")
}

fn is_sensitive_os_component(name: &OsStr) -> bool {
    match name.to_str() {
        Some(text) => is_sensitive_component(text),
        None => is_sensitive_component(&name.to_string_lossy()),
    }
}

fn review_snapshot_entry_allowed(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name();
    !is_review_excluded_os(name) && !is_sensitive_os_component(name)
}

pub fn snapshot_text_files(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    // Do not descend into trees that can never contribute a review file.
    // Zero-allocation OsStr pruning keeps directory walks cheap on large repos.
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(review_snapshot_entry_allowed)
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        let Ok(meta) = entry.metadata() else { continue };
        if meta.len() as usize > MAX_FILE_BYTES {
            continue;
        }
        let path = rel.to_string_lossy().replace('\\', "/");
        if let Ok(bytes) = fs::read(entry.path()) {
            if !bytes.contains(&0) {
                if let Ok(text) = String::from_utf8(bytes) {
                    out.insert(path, text);
                }
            }
        }
    }
    out
}

#[cfg(test)]
pub(super) fn snapshot_walk_count(root: &Path, prune_excluded: bool) -> usize {
    let walker = walkdir::WalkDir::new(root).follow_links(false).into_iter();
    if prune_excluded {
        walker
            .filter_entry(review_snapshot_entry_allowed)
            .filter_map(Result::ok)
            .count()
    } else {
        walker.filter_map(Result::ok).count()
    }
}

pub(super) fn change_file_from_bytes(
    path: &str,
    status: &str,
    before: &[u8],
    after: &[u8],
) -> ChangeFile {
    if sensitive_path(path) {
        return ChangeFile::redacted(path, status, "Sensitive file content hidden");
    }
    if before.len() > MAX_FILE_BYTES || after.len() > MAX_FILE_BYTES {
        return ChangeFile::redacted(path, status, "File too large to render");
    }
    if before.contains(&0) || after.contains(&0) {
        return ChangeFile::redacted(path, status, "Binary file — content not rendered");
    }
    let (Ok(before), Ok(after)) = (std::str::from_utf8(before), std::str::from_utf8(after)) else {
        return ChangeFile::redacted(path, status, "Binary file — content not rendered");
    };
    let patch = unified_patch(path, before, after);
    let additions = patch
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .count();
    let deletions = patch
        .lines()
        .filter(|line| line.starts_with('-') && !line.starts_with("---"))
        .count();
    ChangeFile {
        path: path.into(),
        status: status.into(),
        additions,
        deletions,
        patch,
        redacted: false,
    }
}

pub(super) fn review_path_excluded(path: &str) -> bool {
    path.split(['/', '\\'])
        .any(|part| is_review_excluded_os(OsStr::new(part)))
}

pub(super) fn sensitive_path(path: &str) -> bool {
    path.split(['/', '\\']).any(is_sensitive_component)
}

pub(super) fn unified_patch(path: &str, before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }
    let diff = TextDiff::from_lines(before, after);
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    for group in diff.grouped_ops(3) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            first.old_range().start + 1,
            last.old_range().end - first.old_range().start,
            first.new_range().start + 1,
            last.new_range().end - first.new_range().start
        ));
        for op in group {
            for change in diff.iter_changes(&op) {
                out.push(match change.tag() {
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                    ChangeTag::Equal => ' ',
                });
                out.push_str(change.value());
                if !change.value().ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }
    out
}
