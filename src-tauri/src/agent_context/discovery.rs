use super::*;

pub(super) fn effective_sources(sources: &[String]) -> (Vec<String>, bool) {
    let normalized = normalized_source_list(sources);

    let auto_enabled =
        normalized.is_empty() || normalized.iter().any(|source| source == AUTO_SOURCE);
    if auto_enabled {
        return (
            DISCOVERABLE_SOURCES
                .iter()
                .map(|source| (*source).to_string())
                .collect(),
            true,
        );
    }
    (normalized, false)
}

pub(super) fn normalized_source_list(sources: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for source in sources {
        let source = normalize_provider(source);
        if !source.is_empty() && !normalized.iter().any(|item| item == &source) {
            normalized.push(source);
        }
    }
    normalized
}

pub(super) fn add_auto_instruction_candidates(
    candidates: &mut Vec<(String, PathBuf, &'static str)>,
    workspace_root: &Path,
) {
    for (provider, relative) in [("auto", "GEMINI.md"), ("auto", ".windsurfrules")] {
        candidates.push((provider.into(), workspace_root.join(relative), "workspace"));
    }
}

pub(super) fn collect_skill_candidates(
    candidates: &mut Vec<(String, PathBuf, &'static str)>,
    provider: &str,
    root: &Path,
    scope: &'static str,
    max_depth: usize,
) {
    if !root.is_dir() {
        return;
    }
    for entry in WalkDir::new(root)
        .min_depth(1)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(|value| value.to_str()) == Some("SKILL.md") {
            candidates.push((provider.to_string(), path.to_path_buf(), scope));
        }
    }
}

pub(super) fn collect_auto_workspace_skills(
    candidates: &mut Vec<(String, PathBuf, &'static str)>,
    workspace_root: &Path,
) {
    let walker = WalkDir::new(workspace_root)
        .min_depth(1)
        .max_depth(8)
        .into_iter()
        .filter_entry(auto_scan_entry_allowed);
    for entry in walker.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(|value| value.to_str()) == Some("SKILL.md") {
            candidates.push((
                infer_skill_provider(workspace_root, path),
                path.to_path_buf(),
                "workspace",
            ));
        }
    }
}

pub(super) fn auto_scan_entry_allowed(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !matches!(
        name,
        ".git"
            | ".coding-tools"
            | "node_modules"
            | "target"
            | "build"
            | "dist"
            | ".next"
            | ".svelte-kit"
            | "coverage"
            | "vendor"
            | ".venv"
            | "venv"
    )
}

pub(super) fn infer_skill_provider(workspace_root: &Path, path: &Path) -> String {
    let relative = path
        .strip_prefix(workspace_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    if relative.starts_with(".codex/") {
        return "codex".into();
    }
    if relative.starts_with(".claude/") {
        return "claude".into();
    }
    if relative.starts_with(".cursor/") {
        return "cursor".into();
    }
    if relative.starts_with(".github/") {
        return "copilot".into();
    }
    if relative.starts_with(".opencode/") {
        return "opencode".into();
    }
    if relative.starts_with(".zcode/") {
        return "zcode".into();
    }
    if relative.starts_with(".reasonix/") {
        return "reasonix".into();
    }
    if relative.starts_with(".agents/") || relative.starts_with(".agent/") {
        return "shared".into();
    }
    "auto".into()
}

pub(super) fn normalize_provider(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "github" | "github-copilot" | "github_copilot" => "copilot".into(),
        "open-code" | "open_code" => "opencode".into(),
        "z-code" | "z_code" => "zcode".into(),
        value => value.to_string(),
    }
}

pub(super) fn add_home_candidate(
    candidates: &mut Vec<(String, PathBuf, &'static str)>,
    provider: &str,
    path: &str,
) {
    if let Some(home) = dirs::home_dir() {
        candidates.push((provider.to_string(), home.join(path), "global"));
    }
}

pub(super) fn add_skill_root(
    roots: &mut Vec<(String, PathBuf, &'static str)>,
    provider: &str,
    path: PathBuf,
    scope: &'static str,
) {
    roots.push((provider.to_string(), path, scope));
}

pub(super) fn add_home_skill_root(
    roots: &mut Vec<(String, PathBuf, &'static str)>,
    provider: &str,
    path: &str,
) {
    if let Some(home) = dirs::home_dir() {
        roots.push((provider.to_string(), home.join(path), "global"));
    }
}

pub(super) fn discover_global_instruction_paths(home: &Path, provider: &str) -> Vec<String> {
    let mut candidates = Vec::<PathBuf>::new();
    match provider {
        "codex" => candidates.push(home.join(".codex/AGENTS.md")),
        "claude" => candidates.push(home.join(".claude/CLAUDE.md")),
        "cursor" => {
            let rules = home.join(".cursor/rules");
            if rules.is_dir() {
                for entry in WalkDir::new(rules)
                    .max_depth(6)
                    .into_iter()
                    .filter_map(Result::ok)
                {
                    let path = entry.path();
                    if path.is_file()
                        && matches!(
                            path.extension().and_then(|value| value.to_str()),
                            Some("mdc") | Some("md")
                        )
                        && cursor_rule_is_always_apply(path)
                    {
                        candidates.push(path.to_path_buf());
                    }
                }
            }
        }
        "opencode" => candidates.push(home.join(".config/opencode/AGENTS.md")),
        "zcode" => candidates.push(home.join(".zcode/AGENTS.md")),
        "reasonix" => {
            for name in ["REASONIX.md", "AGENTS.md", "CLAUDE.md"] {
                candidates.push(home.join(".reasonix").join(name));
                candidates.push(
                    home.join(".reasonix")
                        .join(name.trim_end_matches(".md").to_string() + ".local.md"),
                );
            }
        }
        _ => {}
    }

    candidates
        .into_iter()
        .filter(|path| path.is_file())
        .map(|path| display_home_path(home, &path))
        .collect()
}

pub(super) fn discover_global_skill_paths(home: &Path, provider: &str) -> Vec<String> {
    let roots: &[&str] = match provider {
        "codex" => &[".agents/skills", ".codex/skills"],
        "claude" => &[".claude/skills"],
        "cursor" => &[".cursor/skills"],
        "copilot" => &[".github/skills"],
        "opencode" => &[".opencode/skills", ".config/opencode/skills"],
        "zcode" => &[".zcode/skills"],
        "reasonix" => &[".reasonix/skills", ".agent/skills"],
        _ => &[],
    };

    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        let root = home.join(root);
        if !root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&root)
            .min_depth(1)
            .max_depth(4)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if !path.is_file()
                || path.file_name().and_then(|value| value.to_str()) != Some("SKILL.md")
            {
                continue;
            }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let key = canonical.to_string_lossy().to_string();
            if seen.insert(key) {
                result.push(display_home_path(home, &canonical));
            }
        }
    }
    result
}

pub(super) fn display_home_path(home: &Path, path: &Path) -> String {
    let canonical_home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path
        .strip_prefix(&canonical_home)
        .map(|relative| format!("~/{}", relative.to_string_lossy().replace('\\', "/")))
        .unwrap_or_else(|_| canonical_path.to_string_lossy().into_owned())
}

pub(super) fn split_config_paths(value: &str) -> Vec<String> {
    value
        .split(['\n', ','])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) fn resolve_config_path(workspace_root: &Path, value: &str) -> PathBuf {
    if value == "~" {
        return dirs::home_dir().unwrap_or_else(|| workspace_root.to_path_buf());
    }
    if let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        workspace_root.join(path)
    }
}

pub(super) fn display_path(workspace_root: &Path, path: &Path) -> String {
    path.strip_prefix(workspace_root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}
