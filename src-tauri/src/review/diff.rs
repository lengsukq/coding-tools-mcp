use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use similar::{ChangeTag, TextDiff};

use super::store::ChangeFile;

pub(super) const MAX_FILE_BYTES: usize = 512 * 1024;

fn review_snapshot_entry_allowed(entry: &walkdir::DirEntry) -> bool {
    entry.depth() == 0
        || (!review_path_excluded(&entry.file_name().to_string_lossy())
            && !sensitive_path(&entry.file_name().to_string_lossy()))
}

pub fn snapshot_text_files(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    // Do not descend into trees that can never contribute a review file.
    // Filtering only at the leaf used to traverse every node_modules/target
    // entry before discarding it, making even a read-only command expensive.
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(review_snapshot_entry_allowed)
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        let path = rel.to_string_lossy().replace('\\', "/");
        if review_path_excluded(&path) || sensitive_path(&path) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.len() as usize > MAX_FILE_BYTES {
            continue;
        }
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
    let walker = walkdir::WalkDir::new(root).into_iter();
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
    let patch = unified_patch(path, before, after, false);
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
    let normalized = path.replace('\\', "/");
    normalized.split('/').any(|part| {
        matches!(
            part,
            ".git"
                | ".coding-tools"
                | ".local-build"
                | ".scratch"
                | "node_modules"
                | "target"
                | "build"
                | "dist"
                | ".venv"
        )
    })
}

pub(super) fn sensitive_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.split('/').any(|part| {
        part == ".env"
            || part.starts_with(".env.")
            || part.contains("credential")
            || part.contains("secret")
    }) || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with("id_rsa")
        || lower.ends_with("id_ed25519")
}

pub(super) fn unified_patch(
    path: &str,
    before: &str,
    after: &str,
    ignore_whitespace: bool,
) -> String {
    if before == after {
        return String::new();
    }
    let (old, new);
    let (before, after) = if ignore_whitespace {
        old = before
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n");
        new = after
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n");
        (old.as_str(), new.as_str())
    } else {
        (before, after)
    };
    let diff = TextDiff::from_lines(before, after);
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    for group in diff.grouped_ops(3) {
        let first = group.first().unwrap();
        let last = group.last().unwrap();
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
