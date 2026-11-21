use crate::tools::workspace::WorkspaceError;

use super::parser::{Hunk, HunkLine};
use super::patch_failed;

pub(super) fn apply_hunks(original: &str, hunks: &[Hunk]) -> Result<String, WorkspaceError> {
    let line_ending = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let had_trailing_newline = original.ends_with('\n');
    let mut lines: Vec<String> = if original.is_empty() {
        Vec::new()
    } else {
        original
            .split_terminator('\n')
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect()
    };

    for hunk in hunks {
        let hunk_old: Vec<&str> = hunk
            .lines
            .iter()
            .filter_map(|line| match line {
                HunkLine::Context(text) | HunkLine::Remove(text) => Some(text.as_str()),
                HunkLine::Add(_) => None,
            })
            .collect();

        let pos = find_hunk_position(&lines, &hunk_old, 0)
            .ok_or_else(|| patch_failed("Hunk context did not match file content."))?;

        let mut idx = pos;
        for hunk_line in &hunk.lines {
            match hunk_line {
                HunkLine::Context(_) => idx += 1,
                HunkLine::Remove(_) => {
                    if idx < lines.len() {
                        lines.remove(idx);
                    }
                }
                HunkLine::Add(text) => {
                    lines.insert(idx, text.clone());
                    idx += 1;
                }
            }
        }
    }

    let mut output = lines.join(line_ending);
    if !output.is_empty() && (had_trailing_newline || original.is_empty()) {
        output.push_str(line_ending);
    }
    Ok(output)
}

fn find_hunk_position(lines: &[String], pattern: &[&str], start: usize) -> Option<usize> {
    if pattern.is_empty() {
        return Some(start);
    }
    if start > lines.len() || pattern.len() > lines.len().saturating_sub(start) {
        return None;
    }
    for index in start..=lines.len().saturating_sub(pattern.len()) {
        if lines[index..index + pattern.len()]
            .iter()
            .zip(pattern.iter())
            .all(|(actual, expected)| actual.as_str() == *expected)
        {
            return Some(index);
        }
    }
    None
}
