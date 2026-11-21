use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;
use std::time::SystemTime;

use regex::Regex;
use serde_json::{json, Value};
use walkdir::{DirEntry, WalkDir};

use crate::tools::io_util::{is_ignored_os_component, truncate_utf8};
use crate::tools::workspace::{relative_display, tool_ok, Workspace, WorkspaceError};

const DEFAULT_READ_MAX_BYTES: usize = 32_768;
const DEFAULT_LIST_DIR_MAX_ENTRIES: usize = 1_000;
const DEFAULT_LIST_FILES_MAX_RESULTS: usize = 5_000;
const DEFAULT_SEARCH_MAX_RESULTS: usize = 100;
const DEFAULT_SEARCH_MAX_PREVIEW_BYTES: usize = 256;
/// Default per-file cap for `search_text` to avoid loading multi-GB assets.
const DEFAULT_SEARCH_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const READ_PEEK_BYTES: usize = 4096;
const BINARY_PEEK_BYTES: usize = 8192;
const LINE_COUNT_CHUNK_BYTES: usize = 8192;

pub fn read_file(ws: &Workspace, args: &Value) -> Result<Value, WorkspaceError> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkspaceError::invalid_argument("path is required"))?;
    let resolved = ws.resolve_read_path(path)?;
    if resolved.path.is_dir() {
        return Err(WorkspaceError::Tool {
            code: "IS_DIRECTORY",
            message: "Path is a directory.".into(),
            category: "validation",
            retryable: false,
        });
    }
    let max_bytes = args
        .get("max_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_READ_MAX_BYTES as u64) as usize;
    let start_line = args
        .get("start_line")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1) as usize;
    let end_line = args
        .get("end_line")
        .and_then(Value::as_u64)
        .map(|v| v as usize);

    let mut file = File::open(&resolved.path).map_err(file_io_error)?;
    let mut peek = [0u8; READ_PEEK_BYTES];
    let peeked = file.read(&mut peek).map_err(file_io_error)?;
    if peek[..peeked].contains(&0) {
        return Err(WorkspaceError::Tool {
            code: "BINARY_FILE",
            message: "Binary file read blocked for text tool.".into(),
            category: "validation",
            retryable: false,
        });
    }
    let total_bytes = file.metadata().map(|m| m.len() as usize).unwrap_or(peeked);
    let mut reader = BufReader::new((&peek[..peeked]).chain(file));

    let mut line_buf = Vec::new();
    let mut total_lines = 0usize;
    let mut content = String::new();
    let mut truncated = false;
    let mut truncated_by: Option<&'static str> = None;

    loop {
        if truncated || end_line.is_some_and(|limit| total_lines >= limit) {
            total_lines += count_remaining_lines(&mut reader)?;
            break;
        }
        line_buf.clear();
        let read = reader
            .read_until(b'\n', &mut line_buf)
            .map_err(file_io_error)?;
        if read == 0 {
            break;
        }
        let line_str = std::str::from_utf8(&line_buf).map_err(|_| WorkspaceError::Tool {
            code: "UNSUPPORTED_ENCODING",
            message: "File is not valid utf-8.".into(),
            category: "validation",
            retryable: false,
        })?;
        total_lines += 1;

        let in_range =
            total_lines >= start_line && end_line.is_none_or(|limit| total_lines <= limit);
        if in_range && !truncated {
            let remaining = max_bytes.saturating_sub(content.len());
            if line_str.len() <= remaining {
                content.push_str(line_str);
            } else {
                content.push_str(truncate_utf8(line_str, remaining));
                truncated = true;
                truncated_by = Some("bytes");
            }
        }
    }

    let end = end_line.unwrap_or(total_lines).min(total_lines);
    let actual_end = if truncated && !content.is_empty() {
        start_line + content.lines().count().saturating_sub(1)
    } else {
        end
    };
    let mut warnings = Vec::new();
    if truncated {
        warnings.push("content truncated".to_string());
    }
    Ok(tool_ok(json!({
        "path": resolved.display,
        "content": content,
        "encoding": "utf-8",
        "start_line": start_line,
        "end_line": actual_end,
        "next_start_line": truncated.then_some(actual_end.saturating_add(1)),
        "total_lines": total_lines,
        "total_bytes": total_bytes,
        "bytes_read": content.len(),
        "truncated": truncated,
        "truncated_by": truncated_by,
        "warnings": warnings
    })))
}

fn count_remaining_lines<R: Read>(reader: &mut R) -> Result<usize, WorkspaceError> {
    let mut buf = [0u8; LINE_COUNT_CHUNK_BYTES];
    let mut lines = 0usize;
    let mut saw_any = false;
    let mut last_was_newline = true;
    loop {
        let read = reader.read(&mut buf).map_err(file_io_error)?;
        if read == 0 {
            break;
        }
        saw_any = true;
        lines += buf[..read].iter().filter(|&&byte| byte == b'\n').count();
        last_was_newline = buf[read - 1] == b'\n';
    }
    if saw_any && !last_was_newline {
        lines += 1;
    }
    Ok(lines)
}

fn file_io_error(error: io::Error) -> WorkspaceError {
    match error.kind() {
        io::ErrorKind::NotFound => WorkspaceError::not_found("File not found"),
        io::ErrorKind::PermissionDenied => WorkspaceError::Tool {
            code: "PERMISSION_DENIED",
            message: format!("Permission denied reading file: {error}"),
            category: "permission",
            retryable: false,
        },
        _ => WorkspaceError::Tool {
            code: "IO_ERROR",
            message: format!("Failed to read file: {error}"),
            category: "filesystem",
            retryable: false,
        },
    }
}

pub fn list_dir(ws: &Workspace, args: &Value) -> Result<Value, WorkspaceError> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let resolved = ws.resolve_read_path(path)?;
    if !resolved.path.is_dir() {
        return Err(WorkspaceError::not_a_directory("Path is not a directory"));
    }
    let options = DirectoryTraversalOptions::from_args(args);
    let mut state = DirectoryTraversalState::default();
    collect_dir_entries(
        ws,
        &resolved.path,
        &resolved.display,
        1,
        &options,
        &mut state,
    );
    state
        .entries
        .sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(tool_ok(json!({
        "path": resolved.display,
        "entries": state.entries,
        "truncated": state.truncated,
        "warnings": if state.truncated { vec!["entry limit reached"] } else { vec![] }
    })))
}

pub fn list_files(ws: &Workspace, args: &Value) -> Result<Value, WorkspaceError> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let resolved = ws.resolve_read_path(path)?;
    if !resolved.path.is_dir() {
        return Err(WorkspaceError::not_a_directory("Path is not a directory"));
    }
    let patterns = compile_globs(&list_files_patterns(args));
    let exclude_patterns = compile_globs(&string_list_arg(args, "exclude_patterns"));
    let max_results = args
        .get("max_results")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_LIST_FILES_MAX_RESULTS as u64) as usize;
    let include_hidden = args
        .get("include_hidden")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let include_ignored = args
        .get("include_ignored")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut files = Vec::new();
    let mut truncated = false;
    if !ws.is_ignored_path(&resolved.path, include_hidden, include_ignored) {
        let walker = WalkDir::new(&resolved.path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| should_descend(entry, include_hidden, include_ignored));
        for entry in walker.filter_map(Result::ok) {
            let p = entry.path();
            if p == resolved.path || !ws.is_safe_read_path(p) {
                continue;
            }
            if !entry.file_type().is_file() && !entry.file_type().is_symlink() {
                continue;
            }
            let rel = relative_display(ws.root(), p);
            if !patterns.iter().any(|pat| pat.matches(&rel)) {
                continue;
            }
            if exclude_patterns.iter().any(|pat| pat.matches(&rel)) {
                continue;
            }
            let meta = p.symlink_metadata().ok();
            files.push(json!({
                "path": rel,
                "type": if entry.file_type().is_symlink() { "symlink" } else { "file" },
                "size_bytes": meta.as_ref().map(|m| m.len()).unwrap_or(0),
                "modified": meta.and_then(|m| format_mtime(m.modified().ok()))
            }));
            if files.len() >= max_results {
                truncated = true;
                break;
            }
        }
    }
    files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(tool_ok(json!({
        "path": resolved.display,
        "files": files,
        "truncated": truncated,
        "warnings": if truncated { vec!["result limit reached"] } else { vec![] }
    })))
}

pub fn search_text(ws: &Workspace, args: &Value) -> Result<Value, WorkspaceError> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkspaceError::invalid_argument("query is required"))?;
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let resolved = ws.resolve_read_path(path)?;
    let use_regex = args.get("regex").and_then(Value::as_bool).unwrap_or(false);
    let case_sensitive = args
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let max_results = args
        .get("max_results")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SEARCH_MAX_RESULTS as u64) as usize;
    let max_preview = args
        .get("max_preview_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SEARCH_MAX_PREVIEW_BYTES as u64) as usize;
    let max_file_bytes = args
        .get("max_file_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SEARCH_MAX_FILE_BYTES)
        .max(1);

    let (raw_include_globs, raw_exclude_globs) = search_globs(args);
    let include_globs = compile_globs(&raw_include_globs);
    let exclude_globs = compile_globs(&raw_exclude_globs);
    let context_lines = args
        .get("context_lines")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let matcher = build_matcher(query, use_regex, case_sensitive)?;

    let mut state = SearchState {
        matches: Vec::new(),
        skipped_large: 0,
        skipped_binary: 0,
        truncated: false,
        peek_buf: [0u8; BINARY_PEEK_BYTES],
        line_buf: Vec::with_capacity(512),
    };
    let options = SearchOptions {
        matcher: &matcher,
        include_globs: &include_globs,
        exclude_globs: &exclude_globs,
        context_lines,
        max_preview,
        max_results,
        max_file_bytes,
    };

    if resolved.path.is_file() {
        let _ = consider_search_file(ws, &resolved.path, &options, &mut state);
    } else if !ws.is_ignored_path(&resolved.path, false, false) {
        let walker = WalkDir::new(&resolved.path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| should_descend(entry, false, false));
        for entry in walker.filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            if !consider_search_file(ws, entry.path(), &options, &mut state) {
                break;
            }
        }
    }

    let mut warnings = Vec::new();
    if state.truncated {
        warnings.push("result limit reached; scan stopped early".to_string());
    }
    if state.skipped_large > 0 {
        warnings.push(format!(
            "skipped {} file(s) larger than max_file_bytes ({max_file_bytes})",
            state.skipped_large
        ));
    }
    if state.skipped_binary > 0 {
        warnings.push(format!(
            "skipped {} binary or non-utf8 file(s)",
            state.skipped_binary
        ));
    }

    Ok(tool_ok(json!({
        "query": query,
        "matches": state.matches,
        "total_matches": state.matches.len(),
        "truncated": state.truncated,
        "max_file_bytes": max_file_bytes,
        "skipped_large_files": state.skipped_large,
        "skipped_binary_files": state.skipped_binary,
        "warnings": warnings
    })))
}

struct SearchOptions<'a> {
    matcher: &'a Matcher,
    include_globs: &'a [CompiledGlob],
    exclude_globs: &'a [CompiledGlob],
    context_lines: usize,
    max_preview: usize,
    max_results: usize,
    max_file_bytes: u64,
}

struct SearchState {
    matches: Vec<Value>,
    skipped_large: usize,
    skipped_binary: usize,
    truncated: bool,
    peek_buf: [u8; BINARY_PEEK_BYTES],
    line_buf: Vec<u8>,
}

fn consider_search_file(
    ws: &Workspace,
    path: &Path,
    options: &SearchOptions<'_>,
    state: &mut SearchState,
) -> bool {
    if state.matches.len() >= options.max_results {
        state.truncated = true;
        return false;
    }
    if !ws.is_safe_read_path(path) || ws.is_ignored_path(path, false, false) {
        return true;
    }
    let rel = relative_display(ws.root(), path);
    if !passes_glob_filters(&rel, options.include_globs, options.exclude_globs) {
        return true;
    }
    let meta = match path.metadata() {
        Ok(meta) if meta.is_file() => meta,
        _ => return true,
    };
    if meta.len() > options.max_file_bytes {
        state.skipped_large += 1;
        return true;
    }
    match search_file_streaming(path, &rel, options, state) {
        SearchFileOutcome::Binary => {
            state.skipped_binary += 1;
            true
        }
        SearchFileOutcome::Completed => true,
        SearchFileOutcome::LimitReached => {
            state.truncated = true;
            false
        }
    }
}

enum SearchFileOutcome {
    Completed,
    Binary,
    LimitReached,
}

/// Open, binary-check, and stream-search a file using a single `File::open` and
/// caller-owned reusable buffers.
fn search_file_streaming(
    path: &Path,
    rel: &str,
    options: &SearchOptions<'_>,
    state: &mut SearchState,
) -> SearchFileOutcome {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return SearchFileOutcome::Completed,
    };
    let peeked = match file.read(&mut state.peek_buf) {
        Ok(n) => n,
        Err(_) => return SearchFileOutcome::Completed,
    };
    if state.peek_buf[..peeked].contains(&0) {
        return SearchFileOutcome::Binary;
    }

    let mut reader = BufReader::new((&state.peek_buf[..peeked]).chain(file));
    let mut recent: VecDeque<String> = VecDeque::with_capacity(options.context_lines.max(1));
    let mut pending: VecDeque<PendingMatch> = VecDeque::new();
    let mut line_no = 0usize;

    loop {
        state.line_buf.clear();
        let read = match reader.read_until(b'\n', &mut state.line_buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                flush_pending(&mut pending, &mut state.matches, options.max_results);
                return if state.matches.len() >= options.max_results {
                    SearchFileOutcome::LimitReached
                } else {
                    SearchFileOutcome::Completed
                };
            }
        };
        let mut slice = &state.line_buf[..read];
        if slice.ends_with(b"\n") {
            slice = &slice[..slice.len() - 1];
            if slice.ends_with(b"\r") {
                slice = &slice[..slice.len() - 1];
            }
        }
        let Ok(line) = std::str::from_utf8(slice) else {
            // Invalid UTF-8 mid-file: emit already-captured context and stop this file.
            flush_pending(&mut pending, &mut state.matches, options.max_results);
            return if state.matches.len() >= options.max_results {
                SearchFileOutcome::LimitReached
            } else {
                SearchFileOutcome::Completed
            };
        };
        line_no += 1;

        if options.context_lines > 0 {
            for pend in &mut pending {
                if pend.after.len() < options.context_lines {
                    pend.after.push(line.to_string());
                }
            }
            while pending
                .front()
                .is_some_and(|front| front.after.len() >= options.context_lines)
            {
                if let Some(done) = pending.pop_front() {
                    state.matches.push(done.into_value());
                    if state.matches.len() >= options.max_results {
                        return SearchFileOutcome::LimitReached;
                    }
                }
            }
        }

        if options.matcher.is_match(line) {
            let preview = preview_line(line, options.max_preview);
            if options.context_lines == 0 {
                state.matches.push(json!({
                    "path": rel,
                    "line": line_no,
                    "column": 1,
                    "preview": preview
                }));
                if state.matches.len() >= options.max_results {
                    return SearchFileOutcome::LimitReached;
                }
            } else {
                pending.push_back(PendingMatch {
                    path: rel.to_string(),
                    line: line_no,
                    preview,
                    before: recent.iter().cloned().collect(),
                    after: Vec::new(),
                });
            }
        }

        if options.context_lines > 0 {
            recent.push_back(line.to_string());
            while recent.len() > options.context_lines {
                recent.pop_front();
            }
        }
    }

    flush_pending(&mut pending, &mut state.matches, options.max_results);
    if state.matches.len() >= options.max_results {
        SearchFileOutcome::LimitReached
    } else {
        SearchFileOutcome::Completed
    }
}

struct PendingMatch {
    path: String,
    line: usize,
    preview: String,
    before: Vec<String>,
    after: Vec<String>,
}

impl PendingMatch {
    fn into_value(self) -> Value {
        json!({
            "path": self.path,
            "line": self.line,
            "column": 1,
            "preview": self.preview,
            "before": self.before,
            "after": self.after
        })
    }
}

fn preview_line(line: &str, max_preview: usize) -> String {
    if line.len() <= max_preview {
        return line.to_string();
    }
    format!("{}...", truncate_utf8(line, max_preview))
}

fn flush_pending(
    pending: &mut VecDeque<PendingMatch>,
    matches: &mut Vec<Value>,
    max_results: usize,
) {
    while let Some(pend) = pending.pop_front() {
        if matches.len() >= max_results {
            break;
        }
        matches.push(pend.into_value());
    }
}

fn should_descend(entry: &DirEntry, include_hidden: bool, include_ignored: bool) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    !is_ignored_os_component(entry.file_name(), include_hidden, include_ignored)
}

fn build_matcher(
    query: &str,
    use_regex: bool,
    case_sensitive: bool,
) -> Result<Matcher, WorkspaceError> {
    if use_regex {
        let pattern = if case_sensitive {
            Regex::new(query)
        } else {
            Regex::new(&format!("(?i:{query})"))
        }
        .map_err(|e| WorkspaceError::invalid_argument(format!("Invalid regex: {e}")))?;
        return Ok(Matcher::Regex(pattern));
    }
    if case_sensitive {
        return Ok(Matcher::Literal {
            needle: query.to_string(),
            case_sensitive: true,
            ascii_lower: None,
        });
    }
    let ascii_lower = query.is_ascii().then(|| {
        query
            .as_bytes()
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect()
    });
    Ok(Matcher::Literal {
        needle: query.to_lowercase(),
        case_sensitive: false,
        ascii_lower,
    })
}

enum Matcher {
    Regex(Regex),
    Literal {
        needle: String,
        case_sensitive: bool,
        ascii_lower: Option<Vec<u8>>,
    },
}

impl Matcher {
    fn is_match(&self, line: &str) -> bool {
        match self {
            Matcher::Regex(re) => re.is_match(line),
            Matcher::Literal {
                needle,
                case_sensitive: true,
                ..
            } => line.contains(needle.as_str()),
            Matcher::Literal {
                case_sensitive: false,
                ascii_lower: Some(needle_lower),
                ..
            } => contains_ascii_case_insensitive(line.as_bytes(), needle_lower),
            Matcher::Literal {
                needle,
                case_sensitive: false,
                ascii_lower: None,
            } => {
                if line.is_ascii() && !needle.is_ascii() {
                    return false;
                }
                line.to_lowercase().contains(needle.as_str())
            }
        }
    }
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle_lower: &[u8]) -> bool {
    let Some((&first, rest)) = needle_lower.split_first() else {
        return true;
    };
    if haystack.len() < needle_lower.len() {
        return false;
    }
    let limit = haystack.len() - needle_lower.len();
    for idx in 0..=limit {
        if haystack[idx].to_ascii_lowercase() == first
            && haystack[idx + 1..idx + needle_lower.len()].eq_ignore_ascii_case(rest)
        {
            return true;
        }
    }
    false
}

struct DirectoryTraversalOptions {
    max_depth: usize,
    recursive: bool,
    include_hidden: bool,
    include_ignored: bool,
    max_entries: usize,
}

impl DirectoryTraversalOptions {
    fn from_args(args: &Value) -> Self {
        Self {
            recursive: args
                .get("recursive")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            max_depth: args
                .get("max_depth")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .max(1) as usize,
            max_entries: args
                .get("max_entries")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_LIST_DIR_MAX_ENTRIES as u64) as usize,
            include_hidden: args
                .get("include_hidden")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            include_ignored: args
                .get("include_ignored")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

#[derive(Default)]
struct DirectoryTraversalState {
    entries: Vec<Value>,
    truncated: bool,
}

fn collect_dir_entries(
    ws: &Workspace,
    dir: &Path,
    display: &str,
    depth: usize,
    options: &DirectoryTraversalOptions,
    state: &mut DirectoryTraversalState,
) {
    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for item in read_dir.flatten() {
        if state.truncated {
            return;
        }
        let p = item.path();
        if ws.is_ignored_path(&p, options.include_hidden, options.include_ignored) {
            continue;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        let rel = if display == "." {
            name.clone()
        } else {
            format!("{display}/{name}")
        };
        let normalized_rel = rel.replace('\\', "/");
        let ft = item.file_type().ok();
        let entry_type = if ft.as_ref().is_some_and(|t| t.is_symlink()) {
            "symlink"
        } else if ft.as_ref().is_some_and(|t| t.is_dir()) {
            "directory"
        } else if ft.as_ref().is_some_and(|t| t.is_file()) {
            "file"
        } else {
            "other"
        };
        let meta = item.metadata().ok();
        state.entries.push(json!({
            "name": name,
            "path": normalized_rel,
            "type": entry_type,
            "size_bytes": meta.as_ref().map(|m| m.len()).unwrap_or(0),
            "modified": meta.and_then(|m| format_mtime(m.modified().ok())),
            "is_hidden": name.starts_with('.'),
            "is_ignored": false
        }));
        if state.entries.len() >= options.max_entries {
            state.truncated = true;
            return;
        }
        if options.recursive
            && depth < options.max_depth
            && entry_type == "directory"
            && !p.is_symlink()
        {
            collect_dir_entries(ws, &p, &normalized_rel, depth + 1, options, state);
        }
    }
}

fn string_list_arg(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn list_files_patterns(args: &Value) -> Vec<String> {
    let patterns = string_list_arg(args, "patterns");
    if !patterns.is_empty() {
        return patterns;
    }
    if let Some(glob) = args.get("glob").and_then(Value::as_str) {
        if !glob.is_empty() {
            return vec![glob.to_string()];
        }
    }
    vec!["**/*".to_string()]
}

fn search_globs(args: &Value) -> (Vec<String>, Vec<String>) {
    let mut include = string_list_arg(args, "include_globs");
    if let Some(glob) = args.get("glob").and_then(Value::as_str) {
        if !glob.is_empty() {
            include.push(glob.to_string());
        }
    }
    (include, string_list_arg(args, "exclude_globs"))
}

enum CompiledGlob {
    Any,
    Suffix(Option<glob::Pattern>),
    Exact(Option<glob::Pattern>),
}

impl CompiledGlob {
    fn compile(pattern: &str) -> Self {
        let normalized = pattern.replace('\\', "/");
        if normalized == "**/*" || normalized == "*" {
            return Self::Any;
        }
        if let Some(suffix) = normalized.strip_prefix("**/") {
            return Self::Suffix(glob::Pattern::new(suffix).ok());
        }
        Self::Exact(glob::Pattern::new(&normalized).ok())
    }

    fn matches(&self, path: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Suffix(Some(pattern)) => {
                pattern.matches(path) || path.split('/').any(|part| pattern.matches(part))
            }
            Self::Exact(Some(pattern)) => pattern.matches(path),
            _ => false,
        }
    }
}

fn compile_globs(patterns: &[String]) -> Vec<CompiledGlob> {
    patterns
        .iter()
        .map(|pattern| CompiledGlob::compile(pattern))
        .collect()
}

fn passes_glob_filters(rel: &str, include: &[CompiledGlob], exclude: &[CompiledGlob]) -> bool {
    if !include.is_empty() && !include.iter().any(|pat| pat.matches(rel)) {
        return false;
    }
    !exclude.iter().any(|pat| pat.matches(rel))
}

fn format_mtime(st: Option<SystemTime>) -> Option<String> {
    st.map(|t| {
        let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        format!("{}.{:03}Z", d.as_secs(), d.subsec_millis())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn literal_matcher_respects_case_sensitive_lowercase_query() {
        let matcher = build_matcher("token", false, true).expect("matcher");
        assert!(matcher.is_match("let token = 1;"));
        assert!(!matcher.is_match("let TOKEN = 1;"));
        assert!(!matcher.is_match("let Token = 1;"));
    }

    #[test]
    fn literal_matcher_case_insensitive_matches_ascii_and_unicode() {
        let ascii = build_matcher("ToKeN", false, false).expect("ascii");
        assert!(ascii.is_match("let token = 1;"));
        assert!(ascii.is_match("let TOKEN = 1;"));
        assert!(!ascii.is_match("let other = 1;"));

        let unicode = build_matcher("ÜBER", false, false).expect("unicode");
        assert!(unicode.is_match("über alles"));
        assert!(!unicode.is_match("plain ascii line"));
    }

    #[test]
    fn search_text_case_sensitive_lowercase_does_not_match_uppercase() {
        let dir = tempdir().expect("dir");
        fs::write(dir.path().join("sample.txt"), "ALPHA\nalpha\nAlpha\n").expect("write");
        let ws = Workspace::new(dir.path().to_path_buf()).expect("ws");

        let sensitive = search_text(&ws, &json!({"query": "alpha", "case_sensitive": true}))
            .expect("sensitive");
        assert_eq!(sensitive["total_matches"], 1);
        assert_eq!(sensitive["matches"][0]["line"], 2);

        let insensitive = search_text(&ws, &json!({"query": "alpha", "case_sensitive": false}))
            .expect("insensitive");
        assert_eq!(insensitive["total_matches"], 3);
    }

    #[test]
    fn read_file_counts_total_lines_without_decoding_past_end_line() {
        let dir = tempdir().expect("dir");
        fs::write(dir.path().join("lines.txt"), "one\ntwo\nthree\nfour").expect("write");
        let ws = Workspace::new(dir.path().to_path_buf()).expect("ws");

        let slice = read_file(
            &ws,
            &json!({"path": "lines.txt", "start_line": 1, "end_line": 2}),
        )
        .expect("slice");
        assert_eq!(slice["content"], "one\ntwo\n");
        assert_eq!(slice["end_line"], 2);
        assert_eq!(slice["total_lines"], 4);
    }

    #[test]
    fn list_files_and_search_text_prune_excluded_directories() {
        let dir = tempdir().expect("dir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::create_dir_all(dir.path().join("node_modules/pkg")).expect("nm");
        fs::write(dir.path().join("src/app.rs"), "needle\n").expect("app");
        fs::write(dir.path().join("node_modules/pkg/dep.js"), "needle\n").expect("dep");
        let ws = Workspace::new(dir.path().to_path_buf()).expect("ws");

        let listed = list_files(&ws, &json!({})).expect("list_files");
        let paths: Vec<_> = listed["files"]
            .as_array()
            .expect("files")
            .iter()
            .filter_map(|item| item["path"].as_str())
            .collect();
        assert_eq!(paths, vec!["src/app.rs"]);

        let searched = search_text(&ws, &json!({"query": "needle"})).expect("search");
        assert_eq!(searched["total_matches"], 1);
        assert_eq!(searched["matches"][0]["path"], "src/app.rs");
    }

    #[test]
    fn file_io_error_distinguishes_not_found_and_permission_denied() {
        let nf = file_io_error(io::Error::new(io::ErrorKind::NotFound, "missing"));
        assert_eq!(nf.to_error_value()["code"], "NOT_FOUND");
        let pd = file_io_error(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        assert_eq!(pd.to_error_value()["code"], "PERMISSION_DENIED");
    }
}
