use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

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

pub fn merge_source_lists(global: &[String], workspace: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for source in global.iter().chain(workspace.iter()) {
        let source = normalize_provider(source);
        if !source.is_empty() && !result.iter().any(|item| item == &source) {
            result.push(source);
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

pub fn discover_instructions(
    workspace_root: &Path,
    sources: &[String],
    custom_paths: &str,
) -> Vec<InstructionDocument> {
    let mut candidates = Vec::<(String, PathBuf, &'static str)>::new();
    for raw_source in sources {
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
                    candidates.push((provider.clone(), workspace_root.join(name), "workspace"));
                    let local = name.trim_end_matches(".md").to_string() + ".local.md";
                    candidates.push((provider.clone(), workspace_root.join(local), "workspace"));
                }
            }
            "custom" => {}
            _ => {}
        }
    }

    if sources.iter().any(|source| normalize_provider(source) == "custom") {
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
    let mut roots = Vec::<(String, PathBuf, &'static str)>::new();
    for raw_source in sources {
        let provider = normalize_provider(raw_source);
        match provider.as_str() {
            "codex" => {
                for root in [".agents/skills", ".codex/skills"] {
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "claude" => {
                for root in [".claude/skills", ".agents/skills"] {
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "cursor" => {
                for root in [".cursor/skills", ".agents/skills"] {
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "copilot" => {
                for root in [".github/skills", ".claude/skills", ".agents/skills"] {
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "opencode" => {
                for root in [".opencode/skills", ".claude/skills", ".agents/skills"] {
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "zcode" => {
                add_skill_root(&mut roots, &provider, workspace_root.join(".zcode/skills"), "workspace");
            }
            "reasonix" => {
                for root in [".reasonix/skills", ".agents/skills", ".agent/skills", ".claude/skills"] {
                    add_skill_root(&mut roots, &provider, workspace_root.join(root), "workspace");
                }
            }
            "custom" => {}
            _ => {}
        }
    }

    if sources.iter().any(|source| normalize_provider(source) == "custom") {
        for path in split_config_paths(custom_paths) {
            add_skill_root(&mut roots, "custom", resolve_config_path(workspace_root, &path), "custom");
        }
    }

    let mut seen_paths = HashSet::new();
    let mut seen_content = HashSet::new();
    let mut result = Vec::new();
    for (provider, root, scope) in roots {
        if !root.is_dir() { continue; }
        for entry in WalkDir::new(&root).min_depth(1).max_depth(4).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() || path.file_name().and_then(|value| value.to_str()) != Some("SKILL.md") { continue; }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
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
                    provider: provider.clone(),
                    path: display_path(workspace_root, &canonical),
                    scope: scope.into(),
                },
                body,
            });
        }
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
        let skills = discover_skills(root.path(), &["codex".into()], "");
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].descriptor.name, "release");
        assert_eq!(skills[0].body, "# Steps\nRun tests.");
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
}
