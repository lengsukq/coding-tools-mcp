use super::*;

pub fn render_instruction_documents(documents: &[InstructionDocument]) -> String {
    if documents.is_empty() {
        return String::new();
    }
    let mut out = String::from("Repository / IDE instructions:\n");
    for document in documents {
        out.push_str(&format!(
            "\n## [{}] {}\n{}\n",
            document.provider, document.path, document.content
        ));
    }
    out.trim().to_string()
}

#[cfg(test)]
pub fn render_skill_catalog(skills: &[SkillEntry]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut out = String::from("Available skills are loaded on demand. Use list_skills to inspect them and get_skill with a skill id to load the full SKILL.md.\n");
    for skill in skills.iter().take(50) {
        let description = skill
            .descriptor
            .description
            .chars()
            .take(250)
            .collect::<String>();
        out.push_str(&format!(
            "- {}: {} (provider: {}, id: {})\n",
            skill.descriptor.name, description, skill.descriptor.provider, skill.descriptor.id
        ));
    }
    out.trim().to_string()
}

pub(super) fn parse_skill(raw: &str, path: &Path) -> Option<(String, String, String)> {
    let (frontmatter, body) = split_frontmatter(raw);
    let name = frontmatter_value(frontmatter, "name")
        .or_else(|| path.parent()?.file_name()?.to_str().map(str::to_string))?;
    let description = frontmatter_value(frontmatter, "description")?;
    if name.trim().is_empty() || description.trim().is_empty() || description.chars().count() > 1024
    {
        return None;
    }
    Some((
        name.trim().to_string(),
        description.trim().to_string(),
        body.trim().to_string(),
    ))
}

pub(super) fn split_frontmatter(raw: &str) -> (&str, &str) {
    let trimmed = raw.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") {
        return ("", trimmed);
    }
    let rest = &trimmed[3..];
    if let Some(end) = rest.find("\n---") {
        (&rest[..end], &rest[end + 4..])
    } else {
        ("", trimmed)
    }
}

pub(super) fn frontmatter_value(frontmatter: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    frontmatter.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .map(|value| value.trim().trim_matches(['\'', '"']).to_string())
    })
}

pub(super) fn cursor_rule_is_always_apply(path: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let (frontmatter, _) = split_frontmatter(&raw);
    frontmatter.lines().any(|line| {
        line.trim()
            .replace(' ', "")
            .eq_ignore_ascii_case("alwaysapply:true")
    })
}
