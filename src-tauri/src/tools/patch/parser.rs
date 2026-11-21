use crate::tools::workspace::WorkspaceError;

#[derive(Debug)]
pub(super) struct FilePatch {
    pub(super) path: String,
    pub(super) hunks: Vec<Hunk>,
    pub(super) is_new_file: bool,
    pub(super) is_deleted: bool,
}

#[derive(Debug)]
pub(super) struct Hunk {
    pub(super) lines: Vec<HunkLine>,
}

#[derive(Debug)]
pub(super) enum HunkLine {
    Context(String),
    Add(String),
    Remove(String),
}

pub(super) fn parse_unified_diff(patch: &str) -> Result<Vec<FilePatch>, WorkspaceError> {
    if patch
        .lines()
        .any(|line| line.trim_end_matches('\r') == "*** Begin Patch")
    {
        return parse_codex_patch(patch);
    }

    let mut files = Vec::new();
    let mut current: Option<FilePatch> = None;
    let mut current_hunk: Option<Hunk> = None;

    for line in patch.lines() {
        if line.starts_with("--- ") {
            finish_file(&mut files, &mut current, &mut current_hunk);
            let path = parse_diff_path(line.strip_prefix("--- ").unwrap_or(""));
            current = Some(FilePatch {
                path,
                hunks: Vec::new(),
                is_new_file: line.contains("/dev/null"),
                is_deleted: false,
            });
        } else if line.starts_with("+++ ") {
            if let Some(ref mut file) = current {
                let new_path = parse_diff_path(line.strip_prefix("+++ ").unwrap_or(""));
                if !new_path.is_empty() && new_path != "/dev/null" {
                    file.path = new_path;
                }
                if line.contains("/dev/null") {
                    file.is_deleted = true;
                }
            }
        } else if line.starts_with("@@") {
            push_hunk(&mut current, &mut current_hunk);
            current_hunk = Some(Hunk { lines: Vec::new() });
        } else if let Some(ref mut hunk) = current_hunk {
            push_hunk_line(hunk, line);
        }
    }
    finish_file(&mut files, &mut current, &mut current_hunk);
    Ok(files)
}

fn parse_codex_patch(patch: &str) -> Result<Vec<FilePatch>, WorkspaceError> {
    let mut files = Vec::new();
    let mut current: Option<FilePatch> = None;
    let mut current_hunk: Option<Hunk> = None;

    for raw_line in patch.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line == "*** Begin Patch" {
            continue;
        }
        if line == "*** End Patch" {
            finish_file(&mut files, &mut current, &mut current_hunk);
            continue;
        }

        if let Some((path, is_new_file, is_deleted)) = parse_codex_header(line) {
            finish_file(&mut files, &mut current, &mut current_hunk);
            current = Some(FilePatch {
                path: parse_diff_path(path),
                hunks: Vec::new(),
                is_new_file,
                is_deleted,
            });
            if is_new_file {
                current_hunk = Some(Hunk { lines: Vec::new() });
            }
            continue;
        }

        if line.starts_with("@@") {
            push_hunk(&mut current, &mut current_hunk);
            current_hunk = Some(Hunk { lines: Vec::new() });
            continue;
        }

        let Some(file) = current.as_ref() else {
            continue;
        };
        if file.is_deleted {
            continue;
        }
        let hunk = current_hunk.get_or_insert_with(|| Hunk { lines: Vec::new() });
        push_hunk_line(hunk, line);
    }

    finish_file(&mut files, &mut current, &mut current_hunk);
    Ok(files)
}

fn parse_codex_header(line: &str) -> Option<(&str, bool, bool)> {
    line.strip_prefix("*** Add File: ")
        .map(|path| (path, true, false))
        .or_else(|| {
            line.strip_prefix("*** Update File: ")
                .map(|path| (path, false, false))
        })
        .or_else(|| {
            line.strip_prefix("*** Delete File: ")
                .map(|path| (path, false, true))
        })
}

fn push_hunk_line(hunk: &mut Hunk, line: &str) {
    if let Some(rest) = line.strip_prefix('+') {
        hunk.lines.push(HunkLine::Add(rest.to_string()));
    } else if let Some(rest) = line.strip_prefix('-') {
        hunk.lines.push(HunkLine::Remove(rest.to_string()));
    } else if let Some(rest) = line.strip_prefix(' ') {
        hunk.lines.push(HunkLine::Context(rest.to_string()));
    } else if line.is_empty() {
        hunk.lines.push(HunkLine::Context(String::new()));
    }
}

fn push_hunk(current: &mut Option<FilePatch>, current_hunk: &mut Option<Hunk>) {
    if let Some(hunk) = current_hunk.take() {
        if let Some(file) = current.as_mut() {
            file.hunks.push(hunk);
        }
    }
}

fn finish_file(
    files: &mut Vec<FilePatch>,
    current: &mut Option<FilePatch>,
    current_hunk: &mut Option<Hunk>,
) {
    push_hunk(current, current_hunk);
    if let Some(file) = current.take() {
        files.push(file);
    }
}

fn parse_diff_path(raw: &str) -> String {
    let trimmed = raw.trim();
    let path = trimmed
        .strip_prefix("a/")
        .or_else(|| trimmed.strip_prefix("b/"))
        .unwrap_or(trimmed);
    if path == "/dev/null" {
        return String::new();
    }
    path.replace('\\', "/")
}
