use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::util::hash::sha256_hex;
use crate::util::text::truncate_utf8;

mod discovery;
mod presentation;

use discovery::{
    add_auto_instruction_candidates, add_instruction_candidates_for_provider, add_skill_root,
    add_skill_roots_for_provider, collect_auto_workspace_skills, collect_skill_candidates,
    discover_global_instruction_files, discover_global_skill_files, display_home_path,
    display_path, effective_sources, normalize_provider, normalized_source_list,
    resolve_config_path, resolve_home_config_path, split_config_paths, InstructionCandidate,
};
use presentation::parse_skill;
pub use presentation::render_instruction_documents;
#[cfg(test)]
pub use presentation::render_skill_catalog;

pub const AUTO_SOURCE: &str = "auto";
const DISABLED_SOURCE: &str = "disabled";
const MAX_SKILL_BYTES: usize = 100 * 1024;
const COMPACT_WORKSPACE_INSTRUCTION_FILES: &[&str] = &["AGENTS.override.md", "AGENTS.md"];
const DISCOVERABLE_SOURCES: &[&str] = &[
    "agents", "codex", "claude", "cursor", "copilot", "opencode", "zcode", "reasonix", "gemini",
];

#[derive(Debug, Clone, Default)]
pub struct AgentContextRuntimeConfig {
    pub instruction_sources: Vec<String>,
    pub skill_sources: Vec<String>,
    pub custom_instruction_paths: String,
    pub custom_skill_paths: String,
    pub disabled_instruction_ids: Vec<String>,
    pub disabled_skill_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionDocument {
    pub id: String,
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
    pub globally_disabled_instruction_ids: Vec<String>,
    pub globally_disabled_skill_ids: Vec<String>,
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
    pub instructions: Vec<InstructionDocument>,
    pub skills: Vec<SkillDescriptor>,
}

fn stable_item_id(canonical: &Path) -> String {
    let key = canonical.to_string_lossy();
    sha256_hex(key.as_bytes())[..16].to_string()
}

pub fn merge_id_lists(global: &[String], workspace: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for raw in global.iter().chain(workspace.iter()) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() && !result.iter().any(|existing| existing == trimmed) {
            result.push(trimmed.to_string());
        }
    }
    result
}

pub fn filter_instructions(
    instructions: Vec<InstructionDocument>,
    disabled_ids: &[String],
) -> Vec<InstructionDocument> {
    if disabled_ids.is_empty() {
        return instructions;
    }
    let disabled: HashSet<&str> = disabled_ids.iter().map(String::as_str).collect();
    instructions
        .into_iter()
        .filter(|doc| !disabled.contains(doc.id.as_str()))
        .collect()
}

pub fn filter_skills(skills: Vec<SkillEntry>, disabled_ids: &[String]) -> Vec<SkillEntry> {
    if disabled_ids.is_empty() {
        return skills;
    }
    let disabled: HashSet<&str> = disabled_ids.iter().map(String::as_str).collect();
    skills
        .into_iter()
        .filter(|skill| !disabled.contains(skill.descriptor.id.as_str()))
        .collect()
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

    for source in global
        .iter()
        .filter(|source| *source != AUTO_SOURCE && *source != DISABLED_SOURCE)
        .chain(workspace.iter())
    {
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
    let instructions = discover_instructions(
        workspace_root,
        instruction_sources,
        custom_instruction_paths,
    );
    let skill_entries = discover_skills(workspace_root, skill_sources, custom_skill_paths);
    let skills = skill_entries
        .iter()
        .map(|entry| entry.descriptor.clone())
        .collect::<Vec<_>>();
    let rendered_instructions = render_instruction_documents(&instructions);
    AgentContextSnapshot {
        instructions,
        skills,
        rendered_instructions,
        globally_disabled_instruction_ids: Vec::new(),
        globally_disabled_skill_ids: Vec::new(),
    }
}

pub fn scan_global_agent_context_with_custom(
    custom_instruction_paths: &str,
    custom_skill_paths: &str,
) -> GlobalAgentContextScan {
    let Some(home) = dirs::home_dir() else {
        return GlobalAgentContextScan {
            sources: Vec::new(),
            detected_instruction_sources: Vec::new(),
            detected_skill_sources: Vec::new(),
            instructions: Vec::new(),
            skills: Vec::new(),
        };
    };

    scan_global_agent_context_at(&home, custom_instruction_paths, custom_skill_paths)
}

fn scan_global_agent_context_at(
    home: &Path,
    custom_instruction_paths: &str,
    custom_skill_paths: &str,
) -> GlobalAgentContextScan {
    let mut sources = Vec::new();
    let mut detected_instruction_sources = Vec::new();
    let mut detected_skill_sources = Vec::new();

    let mut instruction_candidates: Vec<(String, PathBuf, &'static str)> = Vec::new();
    let mut skill_candidates: Vec<(String, PathBuf, &'static str)> = Vec::new();

    for provider in DISCOVERABLE_SOURCES {
        let instruction_files = discover_global_instruction_files(home, provider);
        let skill_files = discover_global_skill_files(home, provider);
        let instruction_paths: Vec<String> = instruction_files
            .iter()
            .map(|path| display_home_path(home, path))
            .collect();
        let skill_paths: Vec<String> = skill_files
            .iter()
            .map(|path| display_home_path(home, path))
            .collect();

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

        for path in instruction_files {
            instruction_candidates.push(((*provider).to_string(), path, "global"));
        }
        for path in skill_files {
            skill_candidates.push(((*provider).to_string(), path, "global"));
        }
    }

    for custom in split_config_paths(custom_instruction_paths) {
        let resolved = resolve_home_config_path(home, &custom);
        if resolved.is_file() {
            instruction_candidates.push(("custom".to_string(), resolved, "custom"));
        }
    }
    for custom in split_config_paths(custom_skill_paths) {
        let resolved = resolve_home_config_path(home, &custom);
        if resolved.is_dir() {
            let canonical_root = resolved.canonicalize().unwrap_or(resolved);
            collect_skill_candidates(
                &mut skill_candidates,
                "custom",
                &canonical_root,
                "custom",
                4,
            );
        }
    }

    let mut seen_instruction_paths = HashSet::new();
    let mut seen_instruction_content = HashSet::new();
    let mut instructions = Vec::new();
    for (provider, path, scope) in instruction_candidates {
        let canonical = path.canonicalize().unwrap_or(path);
        let path_key = canonical.to_string_lossy().to_string();
        if !seen_instruction_paths.insert(path_key) {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&canonical) else {
            continue;
        };
        let content = raw.trim().to_string();
        if content.is_empty() || !seen_instruction_content.insert(sha256_hex(content.as_bytes())) {
            continue;
        }
        instructions.push(InstructionDocument {
            id: stable_item_id(&canonical),
            provider,
            path: display_home_path(home, &canonical),
            scope: scope.into(),
            content,
        });
    }

    let mut seen_skill_paths = HashSet::new();
    let mut seen_skill_content = HashSet::new();
    let mut skills = Vec::new();
    for (provider, path, scope) in skill_candidates {
        let canonical = path.canonicalize().unwrap_or(path);
        let path_key = canonical.to_string_lossy().to_string();
        if !seen_skill_paths.insert(path_key) {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&canonical) else {
            continue;
        };
        let raw = truncate_utf8(&raw, MAX_SKILL_BYTES);
        let Some((name, description, _body)) = parse_skill(raw, &canonical) else {
            continue;
        };
        if !seen_skill_content.insert(sha256_hex(raw.as_bytes())) {
            continue;
        }
        skills.push(SkillDescriptor {
            id: stable_item_id(&canonical),
            name,
            description,
            provider,
            path: display_home_path(home, &canonical),
            scope: scope.into(),
        });
    }

    skills.sort_by_key(|a| a.name.to_lowercase());

    GlobalAgentContextScan {
        sources,
        detected_instruction_sources,
        detected_skill_sources,
        instructions,
        skills,
    }
}

pub fn discover_compact_workspace_instructions(
    workspace_root: &Path,
    sources: &[String],
) -> Vec<InstructionDocument> {
    let (effective, auto_enabled) = effective_sources(sources);
    if !auto_enabled && effective.iter().any(|source| source == DISABLED_SOURCE) {
        return Vec::new();
    }

    let provider = effective
        .iter()
        .find(|source| {
            matches!(
                source.as_str(),
                "agents" | "codex" | "opencode" | "zcode" | "cursor" | "reasonix"
            )
        })
        .cloned()
        .unwrap_or_else(|| "workspace".to_string());

    for filename in COMPACT_WORKSPACE_INSTRUCTION_FILES {
        let candidate = workspace_root.join(filename);
        if !candidate.is_file() {
            continue;
        }
        let canonical = candidate.canonicalize().unwrap_or(candidate);
        let Ok(raw) = fs::read_to_string(&canonical) else {
            continue;
        };
        let content = raw.trim().to_string();
        if content.is_empty() {
            continue;
        }
        return vec![InstructionDocument {
            id: stable_item_id(&canonical),
            provider,
            path: display_path(workspace_root, &canonical),
            scope: "workspace".into(),
            content,
        }];
    }

    Vec::new()
}

pub fn discover_instructions(
    workspace_root: &Path,
    sources: &[String],
    custom_paths: &str,
) -> Vec<InstructionDocument> {
    let (sources, auto_enabled) = effective_sources(sources);
    let mut candidates = Vec::<InstructionCandidate>::new();
    for raw_source in &sources {
        let provider = normalize_provider(raw_source);
        add_instruction_candidates_for_provider(&mut candidates, &provider, workspace_root);
    }

    if auto_enabled {
        add_auto_instruction_candidates(&mut candidates, workspace_root);
    }

    if sources
        .iter()
        .any(|source| normalize_provider(source) == "custom")
        || (auto_enabled && !custom_paths.trim().is_empty())
    {
        for path in split_config_paths(custom_paths) {
            candidates.push(InstructionCandidate::deferred(
                "custom",
                resolve_config_path(workspace_root, &path),
                "custom",
            ));
        }
    }

    let mut seen_paths = HashSet::new();
    let mut seen_content = HashSet::new();
    let mut result = Vec::new();
    for candidate in candidates {
        if !candidate.path.is_file() {
            continue;
        }
        let canonical = candidate
            .path
            .canonicalize()
            .unwrap_or_else(|_| candidate.path.clone());
        let path_key = canonical.to_string_lossy().to_string();
        if !seen_paths.insert(path_key) {
            continue;
        }
        let content = match candidate.preloaded_content {
            Some(content) => content,
            None => {
                let Ok(content) = fs::read_to_string(&canonical) else {
                    continue;
                };
                content
            }
        };
        let content = content.trim().to_string();
        if content.is_empty() || !seen_content.insert(sha256_hex(content.as_bytes())) {
            continue;
        }
        result.push(InstructionDocument {
            id: stable_item_id(&canonical),
            provider: candidate.provider,
            path: display_path(workspace_root, &canonical),
            scope: candidate.scope.into(),
            content,
        });
    }
    result
}

pub fn discover_skills(
    workspace_root: &Path,
    sources: &[String],
    custom_paths: &str,
) -> Vec<SkillEntry> {
    let (sources, auto_enabled) = effective_sources(sources);
    let mut roots = Vec::<(String, PathBuf, &'static str)>::new();
    for raw_source in &sources {
        let provider = normalize_provider(raw_source);
        add_skill_roots_for_provider(&mut roots, &provider, workspace_root);
    }

    if sources
        .iter()
        .any(|source| normalize_provider(source) == "custom")
        || (auto_enabled && !custom_paths.trim().is_empty())
    {
        for path in split_config_paths(custom_paths) {
            add_skill_root(
                &mut roots,
                "custom",
                resolve_config_path(workspace_root, &path),
                "custom",
            );
        }
    }

    // Deduplicate shared provider skill roots (e.g. .agents/skills, .claude/skills)
    // before walking so each directory tree is traversed at most once.
    let mut seen_roots = HashSet::new();
    let mut candidates = Vec::<(String, PathBuf, &'static str)>::new();
    for (provider, root, scope) in roots {
        if !root.is_dir() {
            continue;
        }
        let canonical_root = root.canonicalize().unwrap_or(root);
        if !seen_roots.insert(canonical_root.clone()) {
            continue;
        }
        collect_skill_candidates(&mut candidates, &provider, &canonical_root, scope, 4);
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
        if !seen_paths.insert(path_key) {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&canonical) else {
            continue;
        };
        let raw = truncate_utf8(&raw, MAX_SKILL_BYTES);
        let Some((name, description, body)) = parse_skill(raw, &canonical) else {
            continue;
        };
        if !seen_content.insert(sha256_hex(raw.as_bytes())) {
            continue;
        }
        let id = stable_item_id(&canonical);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_lists_are_normalized_and_deduplicated() {
        assert_eq!(
            merge_source_lists(
                &["codex".into(), "github-copilot".into()],
                &["copilot".into(), "Open-Code".into()]
            ),
            vec!["codex", "copilot", "opencode"]
        );
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
        let docs = discover_instructions(
            root.path(),
            &["codex".into(), "opencode".into(), "zcode".into()],
            "",
        );
        let shared_docs = docs
            .iter()
            .filter(|doc| doc.content == "shared instructions")
            .collect::<Vec<_>>();
        assert_eq!(shared_docs.len(), 1);
    }

    #[test]
    fn skills_are_progressively_disclosed_from_skill_md() {
        let root = tempfile::tempdir().expect("root");
        let skill_dir = root.path().join(".agents/skills/release");
        fs::create_dir_all(&skill_dir).expect("skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: release\ndescription: Prepare a release safely\n---\n\n# Steps\nRun tests.",
        )
        .expect("skill");
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
        fs::write(codex.join("AGENTS.md"), "Global Codex instructions")
            .expect("codex instructions");

        let claude_skill = home.path().join(".claude/skills/review");
        fs::create_dir_all(&claude_skill).expect("claude skill dir");
        fs::write(
            claude_skill.join("SKILL.md"),
            "---\nname: review\ndescription: Review code\n---\nReview carefully.",
        )
        .expect("claude skill");

        let scan = scan_global_agent_context_at(home.path(), "", "");

        assert!(scan
            .detected_instruction_sources
            .iter()
            .any(|source| source == "codex"));
        assert!(scan
            .detected_skill_sources
            .iter()
            .any(|source| source == "claude"));
        let codex = scan
            .sources
            .iter()
            .find(|source| source.provider == "codex")
            .expect("codex detection");
        assert_eq!(codex.instruction_paths, vec!["~/.codex/AGENTS.md"]);
        let claude = scan
            .sources
            .iter()
            .find(|source| source.provider == "claude")
            .expect("claude detection");
        assert_eq!(claude.skill_paths, vec!["~/.claude/skills/review/SKILL.md"]);
        assert_eq!(scan.instructions.len(), 1);
        assert_eq!(scan.instructions[0].provider, "codex");
        assert!(!scan.instructions[0].id.is_empty());
        assert_eq!(scan.skills.len(), 1);
        assert_eq!(scan.skills[0].name, "review");
        assert_eq!(scan.skills[0].description, "Review code");
    }

    #[cfg(unix)]
    #[test]
    fn global_and_workspace_scans_follow_symlinked_skill_directories_and_deduplicate() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().expect("home");
        let hub_skill = home.path().join(".agents/skills/apple-design");
        fs::create_dir_all(&hub_skill).expect("hub skill");
        fs::write(
            hub_skill.join("SKILL.md"),
            "---\nname: apple-design\ndescription: >-\n  Apple HIG\n  reviewer skill.\n---\n# Guide\nBody",
        )
        .expect("hub skill md");

        // Whole-directory symlink (~/.codex/skills -> ~/.agents/skills)
        let codex_dir = home.path().join(".codex");
        fs::create_dir_all(&codex_dir).expect("codex dir");
        symlink(home.path().join(".agents/skills"), codex_dir.join("skills")).expect("codex link");

        // Per-skill directory symlink (~/.cursor/skills/apple-design -> ~/.agents/skills/apple-design)
        let cursor_skills = home.path().join(".cursor/skills");
        fs::create_dir_all(&cursor_skills).expect("cursor skills");
        symlink(&hub_skill, cursor_skills.join("apple-design")).expect("cursor link");

        let scan = scan_global_agent_context_at(home.path(), "", "");
        assert!(scan.detected_skill_sources.contains(&"agents".to_string()));
        assert!(scan.detected_skill_sources.contains(&"codex".to_string()));
        assert!(scan.detected_skill_sources.contains(&"cursor".to_string()));
        assert_eq!(
            scan.skills.len(),
            1,
            "symlinked duplicates must be deduplicated"
        );
        assert_eq!(scan.skills[0].name, "apple-design");
        assert_eq!(scan.skills[0].description, "Apple HIG reviewer skill.");
        assert_eq!(scan.skills[0].provider, "agents");
    }

    #[test]
    fn filter_skills_and_instructions_exclude_disabled_ids() {
        let root = tempfile::tempdir().expect("root");
        let first = root.path().join(".agents/skills/alpha");
        let second = root.path().join(".agents/skills/beta");
        fs::create_dir_all(&first).expect("alpha");
        fs::create_dir_all(&second).expect("beta");
        fs::write(
            first.join("SKILL.md"),
            "---\nname: alpha\ndescription: First skill\n---\nBody A",
        )
        .expect("alpha md");
        fs::write(
            second.join("SKILL.md"),
            "---\nname: beta\ndescription: Second skill\n---\nBody B",
        )
        .expect("beta md");

        let skills = discover_skills(root.path(), &["custom".into()], ".agents/skills");
        assert_eq!(skills.len(), 2);
        let alpha_id = skills
            .iter()
            .find(|s| s.descriptor.name == "alpha")
            .expect("alpha")
            .descriptor
            .id
            .clone();
        let filtered = filter_skills(skills, &[alpha_id]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].descriptor.name, "beta");

        fs::write(root.path().join("AGENTS.md"), "Workspace rules").expect("agents");
        let docs = discover_compact_workspace_instructions(root.path(), &[AUTO_SOURCE.into()]);
        assert_eq!(docs.len(), 1);
        let doc_id = docs[0].id.clone();
        assert!(filter_instructions(docs, &[doc_id]).is_empty());
    }

    #[test]
    fn cursor_only_loads_always_apply_rules_in_static_phase() {
        let root = tempfile::tempdir().expect("root");
        let rules = root.path().join(".cursor/rules");
        fs::create_dir_all(&rules).expect("rules");
        fs::write(
            rules.join("always.mdc"),
            "---\nalwaysApply: true\n---\nAlways use tests.",
        )
        .expect("always");
        fs::write(
            rules.join("scoped.mdc"),
            "---\nalwaysApply: false\nglobs: src/**\n---\nScoped rule.",
        )
        .expect("scoped");
        let docs = discover_instructions(root.path(), &["cursor".into()], "");
        assert_eq!(docs.len(), 1);
        assert!(docs[0].content.contains("Always use tests"));
    }

    #[test]
    fn empty_sources_enable_auto_instruction_discovery() {
        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("AGENTS.md"), "Use repository rules.").expect("agents");
        fs::write(
            root.path().join("GEMINI.md"),
            "Use Gemini repository rules.",
        )
        .expect("gemini");

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

        assert!(skills
            .iter()
            .any(|skill| skill.descriptor.name == "auto-release"));
        assert!(!skills
            .iter()
            .any(|skill| skill.descriptor.name == "ignored-dependency"));
    }

    #[test]
    fn compact_workspace_instructions_respect_disabled_priority_and_scope() {
        let root = tempfile::tempdir().expect("root");
        let root_path = root.path().canonicalize().expect("canonical root");
        fs::write(root_path.join("AGENTS.md"), "base agents rules").expect("agents");
        fs::write(root_path.join("CLAUDE.md"), "claude rules").expect("claude");
        fs::write(root_path.join("GEMINI.md"), "gemini rules").expect("gemini");

        assert!(
            discover_compact_workspace_instructions(&root_path, &[DISABLED_SOURCE.into()])
                .is_empty()
        );

        let base_docs = discover_compact_workspace_instructions(&root_path, &[AUTO_SOURCE.into()]);
        assert_eq!(base_docs.len(), 1);
        assert_eq!(base_docs[0].scope, "workspace");
        assert_eq!(base_docs[0].path, "AGENTS.md");
        assert_eq!(base_docs[0].content, "base agents rules");

        fs::write(
            root_path.join("AGENTS.override.md"),
            "override agents rules",
        )
        .expect("agents override");
        let override_docs = discover_compact_workspace_instructions(&root_path, &[]);
        assert_eq!(override_docs.len(), 1);
        assert_eq!(override_docs[0].scope, "workspace");
        assert_eq!(override_docs[0].path, "AGENTS.override.md");
        assert_eq!(override_docs[0].content, "override agents rules");

        // When only non-AGENTS provider files exist, compact discovery returns empty
        // and never pulls home-directory or other provider instructions.
        fs::remove_file(root_path.join("AGENTS.override.md")).expect("remove override");
        fs::remove_file(root_path.join("AGENTS.md")).expect("remove agents");
        assert!(discover_compact_workspace_instructions(&root_path, &[]).is_empty());
    }

    #[test]
    fn discover_skills_truncates_multibyte_utf8_at_byte_boundary() {
        let root = tempfile::tempdir().expect("root");
        let skill_dir = root.path().join(".agents/skills/utf8-large");
        fs::create_dir_all(&skill_dir).expect("skill dir");

        let header =
            "---\nname: utf8-large\ndescription: Multibyte truncation boundary test\n---\n\n";
        // Pad with 1 byte so 3-byte Chinese chars straddle the 100 KiB byte boundary.
        let body = format!("a{}", "中".repeat(45_000));
        let full_content = format!("{header}{body}");
        assert!(full_content.len() > MAX_SKILL_BYTES);
        fs::write(skill_dir.join("SKILL.md"), &full_content).expect("skill");

        let skills = discover_skills(root.path(), &["custom".into()], ".agents/skills");
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].descriptor.name, "utf8-large");
        assert!(skills[0].body.len() <= MAX_SKILL_BYTES - header.len());
        assert!(skills[0].body.ends_with('中'));
    }

    #[test]
    fn discover_skills_deduplicates_shared_provider_roots() {
        let root = tempfile::tempdir().expect("root");
        let skill_dir = root.path().join(".agents/skills/shared-check");
        fs::create_dir_all(&skill_dir).expect("skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: shared-check\ndescription: Shared across providers\n---\nCheck once.",
        )
        .expect("skill");

        let skills = discover_skills(
            root.path(),
            &[
                "codex".into(),
                "claude".into(),
                "cursor".into(),
                "copilot".into(),
                "opencode".into(),
            ],
            "",
        );
        let matching = skills
            .iter()
            .filter(|skill| skill.descriptor.name == "shared-check")
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].descriptor.provider, "codex");
    }
}
