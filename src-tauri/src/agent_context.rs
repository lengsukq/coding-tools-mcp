use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

pub const AUTO_SOURCE: &str = "auto";
const DISABLED_SOURCE: &str = "disabled";
const DISCOVERABLE_SOURCES: &[&str] = &[
    "codex",
    "claude",
    "cursor",
    "copilot",
    "opencode",
    "zcode",
    "reasonix",
];

#[derive(Debug, Clone, Default)]
pub struct AgentContextRuntimeConfig {
    pub instruction_sources: Vec<String>,
    pub skill_sources: Vec<String>,
    pub custom_instruction_paths: String,
    pub custom_skill_paths: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionDocument {
    pub provider: String,
    pub path: String,
    pub scope: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider: String,
    pub path: String,
    pub scope: String,
}

#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub descriptor: SkillDescriptor,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentContextSnapshot {
    pub instructions: Vec<InstructionDocument>,
    pub skills: Vec<SkillDescriptor>,
    pub rendered_instructions: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSourceDetection {
    pub provider: String,
    pub instruction_paths: Vec<String>,
    pub skill_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalAgentContextScan {
    pub sources: Vec<AgentSourceDetection>,
    pub detected_instruction_sources: Vec<String>,
    pub detected_skill_sources: Vec<String>,
}

pub fn merge_source_lists(global: &[String], workspace: &[String]) -> Vec<String> {
    let global = normalized_source_list(global);
    let workspace = normalized_source_list(workspace);

    if !workspace.is_empty() {
        if workspace.iter().any(|source| source == AUTO_SOURCE) {
            return vec![AUTO_SOURCE.to_string()];
        }
        if workspace.iter().any(|source| source == DISABLED_SOURCE) {
            return vec![DISABLED_SOURCE.to_string()];
        }
    }

    let mut result = Vec::new();
    if workspace.is_empty() {
        result.extend(global);
        return result;
    }

    for source in global.iter().filter(|source| *source != AUTO_SOURCE && *source != DISABLED_SOURCE).chain(workspace.iter()) {
        if !result.iter().any(|item| item == source) {
            result.push(source.clone());
        }
    }
    result
}

pub fn discover(
    workspace_root: &Path,
    instruction_sources: &[String],
    skill_sources: &[String],
    custom_instruction_paths: &str,
    custom_skill_paths: &str,
) -> AgentContextSnapshot {
    let instructions = discover_instructions(workspace_root, instruction_sources, custom_instruction_paths);
    let skill_entries = discover_skills(workspace_root, skill_sources, custom_skill_paths);
    let skills = skill_entries.iter().map(|entry| entry.descriptor.clone()).collect::<Vec<_>>();
    let rendered_instructions = render_instruction_documents(&instructions);
    AgentContextSnapshot { instructions, skills, rendered_instructions }
}

pub fn scan_global_agent_context() -> GlobalAgentContextScan {
    let Some(home) = dirs::home_dir() else {
        return GlobalAgentContextScan {
            sources: Vec::new(),
            detected_instruction_sources: Vec::new(),
            detected_skill_sources: Vec::new(),
        };
    };

    scan_global_agent_context_at(&home)
}

fn scan_global_agent_context_at(home: &Path) -> GlobalAgentContextScan {

    let mut sources = Vec::new();
    let mut detected_instruction_sources = Vec::new();
    let mut detected_skill_sources = Vec::new();

    for provider in DISCOVERABLE_SOURCES {
        let instruction_paths = discover_global_instruction_paths(&home, provider);
        let skill_paths = discover_global_skill_paths(&home, provider);

        if !instruction_paths.is_empty() {
            detected_instruction_sources.push((*provider).to_string());
        }
        if !skill_paths.is_empty() {
            detected_skill_sources.push((*provider).to_string());
        }
        if !instruction_paths.is_empty() || !skill_paths.is_empty() {
            sources.push(AgentSourceDetection {
                provider: (*provider).to_string(),
                instruction_paths,
                skill_paths,
            });
        }
    }

    GlobalAgentContextScan {
        sources,
        detected_instruction_sources,
        detected_skill_sources,
    }
}

pub fn discover_instructions(
    workspace_root: &Path,
    sources: &[String],
    custom_paths: &str,
) -> Vec<InstructionDocument> {
    let (sources, auto_enabled) = effective_sources(sources);
    let mut candidates = Vec::<(String, PathBuf, &'static str)>::new();
    for raw_source in &sources {
        let provider = normalize_provider(raw_source);
        match provider.as_str() {
            "codex" => {
                add_home_candidate(&mut candidates, &provider, ".codex/AGENTS.md");
                let override_path = workspace_root.join("AGENTS.override.md");
                if override_path.is_file() {
                    candidates.push((provider.clone(), override_path, "workspace"));
                } else {
                    candidates.push((provider.clone(), workspace_root.join("AGENTS.md"), "workspace"));
                }
            }
            "claude" => {
                add_home_candidate(&mut candidates, &provider, ".claude/CLAUDE.md");
                candidates.push((provider.clone(), workspace_root.join("CLAUDE.md"), "workspace"));
            }
            "cursor" => {
                let global_rules = dirs::home_dir().map(|home| home.join(".cursor/rules"));
                if let Some(rules) = global_rules.filter(|rules| rules.is_dir()) {
                    for entry in WalkDir::new(rules).max_depth(6).into_iter().filter_map(Result::ok) {
                        let path = entry.path();
                        if path.is_file()
                            && matches!(path.extension().and_then(|value| value.to_str()), Some("mdc") | Some("md"))
                            && cursor_rule_is_always_apply(path)
                        {
                            candidates.push((provider.clone(), path.to_path_buf(), "global"));
                        }
                    }
                }
                candidates.push((provider.clone(), workspace_root.join("AGENTS.md"), "workspace"));
                candidates.push((provider.clone(), workspace_root.join(".cursorrules"), "workspace"));
                let rules = workspace_root.join(".cursor/rules");
                if rules.is_dir() {
                    for entry in WalkDir::new(rules).max_depth(6).into_iter().filter_map(Result::ok) {
                        let path = entry.path();
                        if path.is_file()
                            && matches!(path.extension().and_then(|value| value.to_str()), Some("mdc") | Some("md"))
                            && cursor_rule_is_always_apply(path)
                        {
                            candidates.push((provider.clone(), path.to_path_buf(), "workspace"));
                        }
                    }
                }
            }
            "copilot" => candidates.push((
                provider.clone(),
                workspace_root.join(".github/copilot-instructions.md"),
                "workspace",
            )),
            "opencode" => {
                add_home_candidate(&mut candidates, &provider, ".config/opencode/AGENTS.md");
                candidates.push((provider.clone(), workspace_root.join("AGENTS.md"), "workspace"));
                if !workspace_root.join("AGENTS.md").is_file() {
                    candidates.push((provider.clone(), workspace_root.join("CLAUDE.md"), "workspace"));
                }
            }
            "zcode" => {
                add_home_candidate(&mut candidates, &provider, ".zcode/AGENTS.md");
                candidates.push((provider.clone(), workspace_root.join("AGENTS.md"), "workspace"));
            }
            "reasonix" => {
                for name in ["REASONIX.md", "AGENTS.md", "CLAUDE.md"] {
                    add_home_candidate(&mut candidates, &provider, &format!(".reasonix/{name}"));
                    let global_local = name.trim_end_matches(".md").to_string() + ".local.md";
                    add_home_candidate(&mut candidates, &provider, &format!(".reasonix/{global_local}"));
                    candidates.push((provider.clone(), workspace_root.join(name), "workspace"));
                    let local = name.trim_end_matches(".md").to_string() + ".local.md";
                    candidates.push((provider.clone(), workspace_root.join(local), "workspace"));
                }
            }
            "custom" => {}
            _ => {}
        }
    }

    if auto_enabled {
        add_auto_instruction_candidates(&mut candidates, workspace_root);
    }

    if sources.iter().any(|source| normalize_provider(source) == "custom")
        || (auto_enabled && !custom_paths.trim().is_empty())
    {
        for path in split_config_paths(custom_paths) {
            candidates.push(("custom".into(), resolve_config_path(workspace_root, &path), "custom"));
        }
    }

    let mut seen_paths = HashSet::new();
    let mut seen_content = HashSet::new();
    let mut result = Vec::new();
    for (provider, path, scope) in candidates {
        if !path.is_file() { continue; }
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let path_key = canonical.to_string_lossy().to_string();
        if !seen_paths.insert(path_key) { continue; }
        let Ok(content) = fs::read_to_string(&canonical) else { continue; };
        let content = content.trim().to_string();
        if content.is_empty() || !seen_content.insert(hex_hash(content.as_bytes())) { continue; }
        result.push(InstructionDocument {
            provider,
            path: display_path(workspace_root, &canonical),
            scope: scope.into(),
            content,
        });
    }
    result
}

pub fn discover_skills(workspace_root: &Path, sources: &[String], custom_paths: &str) -> Vec<SkillEntry> {
    let (sources, auto_enabled) = effective_sources(sources);
    let mut roots = Vec::<(String, PathBuf, &'static str)>::new();
    for raw_source in &sources {
        let provider = normalize_provider(raw_source);
        match provider.as_str() {
            "codex" => {
                for root in [".agents/skills", ".codex/skills"] {
                    add_home_skill_root(&mut roots, &provider, root);
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "claude" => {
                for root in [".claude/skills", ".agents/skills"] {
                    add_home_skill_root(&mut roots, &provider, root);
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "cursor" => {
                for root in [".cursor/skills", ".agents/skills"] {
                    add_home_skill_root(&mut roots, &provider, root);
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "copilot" => {
                for root in [".github/skills", ".claude/skills", ".agents/skills"] {
                    add_home_skill_root(&mut roots, &provider, root);
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "opencode" => {
                for root in [".opencode/skills", ".claude/skills", ".agents/skills"] {
                    add_home_skill_root(&mut roots, &provider, root);
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
                add_home_skill_root(&mut roots, &provider, ".config/opencode/skills");
            }
            "zcode" => {
                add_home_skill_root(&mut roots, &provider, ".zcode/skills");
                add_skill_root(&mut roots, &provider, workspace_root.join(".zcode/skills"), "workspace");
            }
            "reasonix" => {
                for root in [".reasonix/skills", ".agents/skills", ".agent/skills", ".claude/skills"] {
                    add_home_skill_root(&mut roots, &provider, root);
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "custom" => {}
            _ => {}
        }
    }

    if sources.iter().any(|source| normalize_provider(source) == "custom")
        || (auto_enabled && !custom_paths.trim().is_empty())
    {
        for path in split_config_paths(custom_paths) {
            add_skill_root(&mut roots, "custom", resolve_config_path(workspace_root, &path), "custom");
        }
    }

    let mut candidates = Vec::<(String, PathBuf, &'static str)>::new();
    for (provider, root, scope) in roots {
        collect_skill_candidates(&mut candidates, &provider, &root, scope, 4);
    }
    if auto_enabled {
        collect_auto_workspace_skills(&mut candidates, workspace_root);
    }

    let mut seen_paths = HashSet::new();
    let mut seen_content = HashSet::new();
    let mut result = Vec::new();
    for (provider, path, scope) in candidates {
        let canonical = path.canonicalize().unwrap_or(path);
        let path_key = canonical.to_string_lossy().to_string();
        if !seen_paths.insert(path_key.clone()) { continue; }
        let Ok(mut raw) = fs::read_to_string(&canonical) else { continue; };
        if raw.len() > 100 * 1024 { raw = raw.chars().take(100 * 1024).collect(); }
        let Some((name, description, body)) = parse_skill(&raw, &canonical) else { continue; };
        if !seen_content.insert(hex_hash(raw.as_bytes())) { continue; }
        let id = hex_hash(path_key.as_bytes())[..16].to_string();
        result.push(SkillEntry {
            descriptor: SkillDescriptor {
                id,
                name,
                description,
                provider,
                path: display_path(workspace_root, &canonical),
                scope: scope.into(),
            },
            body,
        });
    }
    result
}

pub fn render_instruction_documents(documents: &[InstructionDocument]) -> String {
    if documents.is_empty() { return String::new(); }
    let mut out = String::from("Repository / IDE instructions:\n");
    for document in documents {
        out.push_str(&format!("\n## [{}] {}\n{}\n", document.provider, document.path, document.content));
    }
    out.trim().to_string()
}

pub fn render_skill_catalog(skills: &[SkillEntry]) -> String {
    if skills.is_empty() { return String::new(); }
    let mut out = String::from("Available skills are loaded on demand. Use list_skills to inspect them and get_skill with a skill id to load the full SKILL.md.\n");
    for skill in skills.iter().take(50) {
        let description = skill.descriptor.description.chars().take(250).collect::<String>();
        out.push_str(&format!("- {}: {} (provider: {}, id: {})\n", skill.descriptor.name, description, skill.descriptor.provider, skill.descriptor.id));
    }
    out.trim().to_string()
}

fn parse_skill(raw: &str, path: &Path) -> Option<(String, String, String)> {
    let (frontmatter, body) = split_frontmatter(raw);
    let name = frontmatter_value(frontmatter, "name").or_else(|| path.parent()?.file_name()?.to_str().map(str::to_string))?;
    let description = frontmatter_value(frontmatter, "description")?;
    if name.trim().is_empty() || description.trim().is_empty() || description.chars().count() > 1024 { return None; }
    Some((name.trim().to_string(), description.trim().to_string(), body.trim().to_string()))
}

fn split_frontmatter(raw: &str) -> (&str, &str) {
    let trimmed = raw.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") { return ("", trimmed); }
    let rest = &trimmed[3..];
    if let Some(end) = rest.find("\n---") { (&rest[..end], &rest[end + 4..]) } else { ("", trimmed) }
}

fn frontmatter_value(frontmatter: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    frontmatter.lines().find_map(|line| line.trim().strip_prefix(&prefix).map(|value| value.trim().trim_matches(['\'', '"']).to_string()))
}

fn cursor_rule_is_always_apply(path: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(path) else { return false; };
    let (frontmatter, _) = split_frontmatter(&raw);
    frontmatter.lines().any(|line| line.trim().replace(' ', "").to_ascii_lowercase() == "alwaysapply:true")
}

fn effective_sources(sources: &[String]) -> (Vec<String>, bool) {
    let normalized = normalized_source_list(sources);

    let auto_enabled = normalized.is_empty() || normalized.iter().any(|source| source == AUTO_SOURCE);
    if auto_enabled {
        return (
            DISCOVERABLE_SOURCES.iter().map(|source| (*source).to_string()).collect(),
            true,
        );
    }
    (normalized, false)
}

fn normalized_source_list(sources: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for source in sources {
        let source = normalize_provider(source);
        if !source.is_empty() && !normalized.iter().any(|item| item == &source) {
            normalized.push(source);
        }
    }
    normalized
}

fn add_auto_instruction_candidates(
    candidates: &mut Vec<(String, PathBuf, &'static str)>,
    workspace_root: &Path,
) {
    for (provider, relative) in [
        ("auto", "GEMINI.md"),
        ("auto", ".windsurfrules"),
    ] {
        candidates.push((provider.into(), workspace_root.join(relative), "workspace"));
    }
}

fn collect_skill_candidates(
    candidates: &mut Vec<(String, PathBuf, &'static str)>,
    provider: &str,
    root: &Path,
    scope: &'static str,
    max_depth: usize,
) {
    if !root.is_dir() { return; }
    for entry in WalkDir::new(root).min_depth(1).max_depth(max_depth).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(|value| value.to_str()) == Some("SKILL.md") {
            candidates.push((provider.to_string(), path.to_path_buf(), scope));
        }
    }
}

fn collect_auto_workspace_skills(
    candidates: &mut Vec<(String, PathBuf, &'static str)>,
    workspace_root: &Path,
) {
    let walker = WalkDir::new(workspace_root)
        .min_depth(1)
        .max_depth(8)
        .into_iter()
        .filter_entry(|entry| auto_scan_entry_allowed(entry));
    for entry in walker.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(|value| value.to_str()) == Some("SKILL.md") {
            candidates.push((infer_skill_provider(workspace_root, path), path.to_path_buf(), "workspace"));
        }
    }
}

fn auto_scan_entry_allowed(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() { return true; }
    let Some(name) = entry.file_name().to_str() else { return true; };
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

fn infer_skill_provider(workspace_root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(workspace_root).unwrap_or(path).to_string_lossy().replace('\\', "/");
    if relative.starts_with(".codex/") { return "codex".into(); }
    if relative.starts_with(".claude/") { return "claude".into(); }
    if relative.starts_with(".cursor/") { return "cursor".into(); }
    if relative.starts_with(".github/") { return "copilot".into(); }
    if relative.starts_with(".opencode/") { return "opencode".into(); }
    if relative.starts_with(".zcode/") { return "zcode".into(); }
    if relative.starts_with(".reasonix/") { return "reasonix".into(); }
    if relative.starts_with(".agents/") || relative.starts_with(".agent/") { return "shared".into(); }
    "auto".into()
}

fn normalize_provider(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "github" | "github-copilot" | "github_copilot" => "copilot".into(),
        "open-code" | "open_code" => "opencode".into(),
        "z-code" | "z_code" => "zcode".into(),
        value => value.to_string(),
    }
}

fn add_home_candidate(candidates: &mut Vec<(String, PathBuf, &'static str)>, provider: &str, path: &str) {
    if let Some(home) = dirs::home_dir() { candidates.push((provider.to_string(), home.join(path), "global")); }
}

fn add_skill_root(roots: &mut Vec<(String, PathBuf, &'static str)>, provider: &str, path: PathBuf, scope: &'static str) {
    roots.push((provider.to_string(), path, scope));
}

fn add_home_skill_root(roots: &mut Vec<(String, PathBuf, &'static str)>, provider: &str, path: &str) {
    if let Some(home) = dirs::home_dir() {
        roots.push((provider.to_string(), home.join(path), "global"));
    }
}

fn discover_global_instruction_paths(home: &Path, provider: &str) -> Vec<String> {
    let mut candidates = Vec::<PathBuf>::new();
    match provider {
        "codex" => candidates.push(home.join(".codex/AGENTS.md")),
        "claude" => candidates.push(home.join(".claude/CLAUDE.md")),
        "cursor" => {
            let rules = home.join(".cursor/rules");
            if rules.is_dir() {
                for entry in WalkDir::new(rules).max_depth(6).into_iter().filter_map(Result::ok) {
                    let path = entry.path();
                    if path.is_file()
                        && matches!(path.extension().and_then(|value| value.to_str()), Some("mdc") | Some("md"))
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
                candidates.push(home.join(".reasonix").join(name.trim_end_matches(".md").to_string() + ".local.md"));
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

fn discover_global_skill_paths(home: &Path, provider: &str) -> Vec<String> {
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
        if !root.is_dir() { continue; }
        for entry in WalkDir::new(&root).min_depth(1).max_depth(4).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() || path.file_name().and_then(|value| value.to_str()) != Some("SKILL.md") {
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

fn display_home_path(home: &Path, path: &Path) -> String {
    let canonical_home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path.strip_prefix(&canonical_home)
        .map(|relative| format!("~/{}", relative.to_string_lossy().replace('\\', "/")))
        .unwrap_or_else(|_| canonical_path.to_string_lossy().into_owned())
}

fn split_config_paths(value: &str) -> Vec<String> {
    value.split(|ch| ch == '\n' || ch == ',').map(str::trim).filter(|item| !item.is_empty()).map(str::to_string).collect()
}

fn resolve_config_path(workspace_root: &Path, value: &str) -> PathBuf {
    if value == "~" { return dirs::home_dir().unwrap_or_else(|| workspace_root.to_path_buf()); }
    if let Some(rest) = value.strip_prefix("~/").or_else(|| value.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() { return home.join(rest); }
    }
    let path = PathBuf::from(value);
    if path.is_absolute() { path } else { workspace_root.join(path) }
}

fn display_path(workspace_root: &Path, path: &Path) -> String {
    path.strip_prefix(workspace_root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

fn hex_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_lists_are_normalized_and_deduplicated() {
        assert_eq!(merge_source_lists(&["codex".into(), "github-copilot".into()], &["copilot".into(), "Open-Code".into()]), vec!["codex", "copilot", "opencode"]);
    }

    #[test]
    fn workspace_auto_mode_overrides_global_provider_limits() {
        assert_eq!(
            merge_source_lists(&["codex".into()], &[AUTO_SOURCE.into()]),
            vec![AUTO_SOURCE]
        );
    }

    #[test]
    fn workspace_manual_sources_override_global_auto_mode() {
        assert_eq!(
            merge_source_lists(&[AUTO_SOURCE.into()], &["claude".into()]),
            vec!["claude"]
        );
    }

    #[test]
    fn workspace_disabled_mode_turns_off_inherited_scanning() {
        assert_eq!(
            merge_source_lists(&["codex".into()], &[DISABLED_SOURCE.into()]),
            vec![DISABLED_SOURCE]
        );
    }

    #[test]
    fn duplicate_instruction_files_are_injected_once() {
        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("AGENTS.md"), "shared instructions").expect("agents");
        let docs = discover_instructions(root.path(), &["codex".into(), "opencode".into(), "zcode".into()], "");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].content, "shared instructions");
    }

    #[test]
    fn skills_are_progressively_disclosed_from_skill_md() {
        let root = tempfile::tempdir().expect("root");
        let skill_dir = root.path().join(".agents/skills/release");
        fs::create_dir_all(&skill_dir).expect("skill dir");
        fs::write(skill_dir.join("SKILL.md"), "---\nname: release\ndescription: Prepare a release safely\n---\n\n# Steps\nRun tests.").expect("skill");
        let skills = discover_skills(root.path(), &["custom".into()], ".agents/skills");
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].descriptor.name, "release");
        assert_eq!(skills[0].body, "# Steps\nRun tests.");
    }

    #[test]
    fn global_scan_detects_user_level_instruction_and_skill_sources() {
        let home = tempfile::tempdir().expect("home");
        let codex = home.path().join(".codex");
        fs::create_dir_all(&codex).expect("codex dir");
        fs::write(codex.join("AGENTS.md"), "Global Codex instructions").expect("codex instructions");

        let claude_skill = home.path().join(".claude/skills/review");
        fs::create_dir_all(&claude_skill).expect("claude skill dir");
        fs::write(
            claude_skill.join("SKILL.md"),
            "---\nname: review\ndescription: Review code\n---\nReview carefully.",
        )
        .expect("claude skill");

        let scan = scan_global_agent_context_at(home.path());

        assert!(scan.detected_instruction_sources.iter().any(|source| source == "codex"));
        assert!(scan.detected_skill_sources.iter().any(|source| source == "claude"));
        let codex = scan.sources.iter().find(|source| source.provider == "codex").expect("codex detection");
        assert_eq!(codex.instruction_paths, vec!["~/.codex/AGENTS.md"]);
        let claude = scan.sources.iter().find(|source| source.provider == "claude").expect("claude detection");
        assert_eq!(claude.skill_paths, vec!["~/.claude/skills/review/SKILL.md"]);
    }

    #[test]
    fn cursor_only_loads_always_apply_rules_in_static_phase() {
        let root = tempfile::tempdir().expect("root");
        let rules = root.path().join(".cursor/rules");
        fs::create_dir_all(&rules).expect("rules");
        fs::write(rules.join("always.mdc"), "---\nalwaysApply: true\n---\nAlways use tests.").expect("always");
        fs::write(rules.join("scoped.mdc"), "---\nalwaysApply: false\nglobs: src/**\n---\nScoped rule.").expect("scoped");
        let docs = discover_instructions(root.path(), &["cursor".into()], "");
        assert_eq!(docs.len(), 1);
        assert!(docs[0].content.contains("Always use tests"));
    }

    #[test]
    fn empty_sources_enable_auto_instruction_discovery() {
        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("AGENTS.md"), "Use repository rules.").expect("agents");
        fs::write(root.path().join("GEMINI.md"), "Use Gemini repository rules.").expect("gemini");

        let docs = discover_instructions(root.path(), &[], "");

        assert!(docs.iter().any(|doc| doc.path.ends_with("AGENTS.md")));
        assert!(docs.iter().any(|doc| doc.path.ends_with("GEMINI.md")));
    }

    #[test]
    fn auto_skill_discovery_finds_unknown_skill_roots_and_skips_dependencies() {
        let root = tempfile::tempdir().expect("root");
        let custom_skill = root.path().join(".custom-agent/workflows/release");
        fs::create_dir_all(&custom_skill).expect("custom skill dir");
        fs::write(
            custom_skill.join("SKILL.md"),
            "---\nname: auto-release\ndescription: Automatically discovered release workflow\n---\nRun release checks.",
        )
        .expect("custom skill");

        let dependency_skill = root.path().join("node_modules/dependency/skills/ignored");
        fs::create_dir_all(&dependency_skill).expect("dependency skill dir");
        fs::write(
            dependency_skill.join("SKILL.md"),
            "---\nname: ignored-dependency\ndescription: Must not be scanned\n---\nIgnore me.",
        )
        .expect("dependency skill");

        let skills = discover_skills(root.path(), &[AUTO_SOURCE.into()], "");

        assert!(skills.iter().any(|skill| skill.descriptor.name == "auto-release"));
        assert!(!skills.iter().any(|skill| skill.descriptor.name == "ignored-dependency"));
    }
}
