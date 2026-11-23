use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use super::InstructionDocument;
#[cfg(test)]
use super::SkillEntry;

const MAX_SKILL_DESCRIPTION_CHARS: usize = 1024;
const FALLBACK_DESCRIPTION_CHARS: usize = 240;

pub fn render_instruction_documents(documents: &[InstructionDocument]) -> String {
    if documents.is_empty() {
        return String::new();
    }
    let mut out = String::from("Repository / IDE instructions:\n");
    for document in documents {
        let _ = writeln!(
            out,
            "\n## [{}] {}\n{}",
            document.provider, document.path, document.content
        );
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
        let _ = writeln!(
            out,
            "- {}: {} (provider: {}, id: {})",
            skill.descriptor.name, description, skill.descriptor.provider, skill.descriptor.id
        );
    }
    out.trim().to_string()
}

pub(super) fn parse_skill(raw: &str, path: &Path) -> Option<(String, String, String)> {
    let (frontmatter, body) = split_frontmatter(raw);
    let name = frontmatter_value(frontmatter, "name")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| path.parent()?.file_name()?.to_str().map(str::to_string))?;
    let trimmed_body = body.trim();
    let raw_description = frontmatter_value(frontmatter, "description")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| fallback_description_from_body(trimmed_body))?;
    let description = truncate_chars(raw_description.trim(), MAX_SKILL_DESCRIPTION_CHARS);
    if name.trim().is_empty() || description.is_empty() {
        return None;
    }
    Some((
        name.trim().to_string(),
        description,
        trimmed_body.to_string(),
    ))
}

fn fallback_description_from_body(body: &str) -> Option<String> {
    let mut in_code_fence = false;
    for raw_line in body.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence
            || trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("---")
            || trimmed.starts_with('>')
        {
            continue;
        }
        return Some(truncate_chars(trimmed, FALLBACK_DESCRIPTION_CHARS));
    }
    None
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let collected: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        let clipped: String = collected
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect();
        format!("{}…", clipped.trim_end())
    } else {
        collected
    }
}

pub(super) fn split_frontmatter(raw: &str) -> (&str, &str) {
    let trimmed = raw.trim_start_matches('\u{feff}');
    let after_open = if let Some(rest) = trimmed.strip_prefix("---\r\n") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("---\n") {
        rest
    } else {
        return ("", trimmed);
    };

    let mut offset = 0usize;
    for line in after_open.split_inclusive('\n') {
        let clean = line.trim_end_matches(['\r', '\n']);
        if clean == "---" || clean == "..." {
            return (&after_open[..offset], &after_open[offset + line.len()..]);
        }
        offset += line.len();
    }
    ("", trimmed)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockStyle {
    Folded,
    Literal,
}

fn parse_block_scalar_header(rest: &str) -> Option<BlockStyle> {
    let token = rest.split('#').next().unwrap_or("").trim();
    let mut chars = token.chars();
    let style = match chars.next()? {
        '>' => BlockStyle::Folded,
        '|' => BlockStyle::Literal,
        _ => return None,
    };
    for ch in chars {
        if !matches!(ch, '-' | '+' | '1'..='9') {
            return None;
        }
    }
    Some(style)
}

fn collect_indented_block(lines: &[&str], start_index: usize) -> Vec<String> {
    let mut collected = Vec::new();
    let mut min_indent: Option<usize> = None;

    for raw_line in &lines[start_index..] {
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            collected.push(String::new());
            continue;
        }
        let indent = line
            .bytes()
            .take_while(|byte| *byte == b' ' || *byte == b'\t')
            .count();
        if indent == 0 {
            break;
        }
        min_indent = Some(min_indent.map_or(indent, |current| current.min(indent)));
        collected.push(line.to_string());
    }

    while collected.last().is_some_and(|line| line.is_empty()) {
        collected.pop();
    }

    let strip_indent = min_indent.unwrap_or(0);
    collected
        .into_iter()
        .map(|line| {
            if line.is_empty() {
                line
            } else {
                line.get(strip_indent..)
                    .unwrap_or(line.trim_start())
                    .to_string()
            }
        })
        .collect()
}

fn join_folded_block(block_lines: &[String]) -> String {
    let mut result = String::new();
    let mut pending_blank = false;

    for line in block_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !result.is_empty() {
                pending_blank = true;
            }
            continue;
        }
        if !result.is_empty() {
            if pending_blank {
                result.push('\n');
                pending_blank = false;
            } else {
                result.push(' ');
            }
        }
        result.push_str(trimmed);
    }
    result
}

fn unquote_scalar(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        return inner
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n")
            .replace("\\t", "\t");
    }
    if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        let inner = &trimmed[1..trimmed.len() - 1];
        return inner.replace("''", "'");
    }
    trimmed.to_string()
}

pub(super) fn frontmatter_value(frontmatter: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    let lines: Vec<&str> = frontmatter.lines().collect();

    for (index, raw_line) in lines.iter().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        // Match only top-level keys so nested fields (e.g. metadata.name) never shadow them.
        let Some(after_key) = line.strip_prefix(&prefix) else {
            continue;
        };
        let rest = after_key.trim();
        if let Some(style) = parse_block_scalar_header(rest) {
            let block = collect_indented_block(&lines, index + 1);
            let parsed = match style {
                BlockStyle::Folded => join_folded_block(&block),
                BlockStyle::Literal => block.join("\n").trim().to_string(),
            };
            return Some(parsed);
        }
        if rest.is_empty() {
            let block = collect_indented_block(&lines, index + 1);
            if !block.is_empty() {
                return Some(join_folded_block(&block));
            }
            return Some(String::new());
        }
        let mut combined = unquote_scalar(rest);
        if !rest.starts_with('"') && !rest.starts_with('\'') {
            let continuation = collect_indented_block(&lines, index + 1);
            if !continuation.is_empty() {
                let extra = join_folded_block(&continuation);
                if !extra.is_empty() {
                    combined.push(' ');
                    combined.push_str(&extra);
                }
            }
        }
        return Some(combined);
    }
    None
}

pub(super) fn cursor_rule_content_is_always_apply(raw: &str) -> bool {
    let (frontmatter, _) = split_frontmatter(raw);
    frontmatter.lines().any(|line| {
        line.trim()
            .replace(' ', "")
            .eq_ignore_ascii_case("alwaysapply:true")
    })
}

pub(super) fn cursor_rule_is_always_apply(path: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    cursor_rule_content_is_always_apply(&raw)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn parse_skill_supports_folded_and_literal_yaml_block_scalars() {
        let folded = "---\nname: swarm\ndescription: >-\n  第一行说明。\n  第二行补充。\nmetadata:\n  name: nested-ignored\n---\n\n# Body\nContent";
        let (name, description, body) =
            parse_skill(folded, Path::new("/skills/swarm/SKILL.md")).expect("folded skill");
        assert_eq!(name, "swarm");
        assert_eq!(description, "第一行说明。 第二行补充。");
        assert_eq!(body, "# Body\nContent");

        let literal = "---\r\nname: \"cloudflare\"\r\ndescription: |\r\n  Line A\r\n  Line B\r\n---\r\n\r\nBody text";
        let (name, description, body) =
            parse_skill(literal, Path::new("/skills/cloudflare/SKILL.md")).expect("literal skill");
        assert_eq!(name, "cloudflare");
        assert_eq!(description, "Line A\nLine B");
        assert_eq!(body, "Body text");
    }

    #[test]
    fn parse_skill_ignores_nested_keys_and_truncates_overlong_description() {
        let long_desc = "a".repeat(1200);
        let raw = format!(
            "---\nmetadata:\n  name: wrong-nested\nname: real-skill\ndescription: {long_desc}\n---\n\nBody"
        );
        let (name, description, _) =
            parse_skill(&raw, Path::new("/skills/real-skill/SKILL.md")).expect("long skill");
        assert_eq!(name, "real-skill");
        assert_eq!(description.chars().count(), MAX_SKILL_DESCRIPTION_CHARS);
        assert!(description.ends_with('…'));
    }

    #[test]
    fn parse_skill_falls_back_to_body_paragraph_when_frontmatter_lacks_description() {
        let raw = "# Local Guide\n\nHelpful instructions for local deployments.\n";
        let (name, description, body) =
            parse_skill(raw, Path::new("/skills/local-guide/SKILL.md")).expect("fallback skill");
        assert_eq!(name, "local-guide");
        assert_eq!(description, "Helpful instructions for local deployments.");
        assert!(body.contains("Helpful instructions"));
    }
}
