use serde_json::{json, Value};

use crate::tools::context::ToolContext;
use crate::tools::workspace::{tool_ok, WorkspaceError};

pub fn list_skills(ctx: &ToolContext, _args: &Value) -> Result<Value, WorkspaceError> {
    let skills = ctx.current_skills();
    Ok(tool_ok(json!({
        "skills": skills.iter().map(|skill| &skill.descriptor).collect::<Vec<_>>(),
        "count": skills.len()
    })))
}

pub fn get_skill(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let id = args.get("id").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty());
    let name = args.get("name").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty());
    if id.is_none() && name.is_none() {
        return Err(WorkspaceError::invalid_argument("id or name is required"));
    }
    let skills = ctx.current_skills();
    let matches = skills.iter().filter(|skill| {
        id.is_some_and(|value| skill.descriptor.id == value)
            || name.is_some_and(|value| skill.descriptor.name.eq_ignore_ascii_case(value))
    }).collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Err(WorkspaceError::Tool {
            code: "SKILL_NOT_FOUND",
            message: "Skill not found in discovered agent sources".into(),
            category: "runtime",
            retryable: false,
        }),
        [skill] => Ok(tool_ok(json!({
            "skill": skill.descriptor,
            "content": skill.body
        }))),
        _ => Err(WorkspaceError::ToolDetails {
            code: "AMBIGUOUS_SKILL",
            message: "Multiple skills use this name; call get_skill with id instead".into(),
            category: "runtime",
            retryable: false,
            details: json!({
                "matches": matches.iter().map(|skill| &skill.descriptor).collect::<Vec<_>>()
            }),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_context::{AgentContextRuntimeConfig, SkillDescriptor, SkillEntry};
    use crate::tools::ToolContext;

    #[test]
    fn get_skill_loads_body_on_demand() {
        let workspace = tempfile::tempdir().expect("workspace");
        let harness = tempfile::tempdir().expect("harness");
        let ctx = ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
            .expect("context")
            .with_skills(vec![SkillEntry {
                descriptor: SkillDescriptor {
                    id: "abc".into(),
                    name: "release".into(),
                    description: "Release safely".into(),
                    provider: "codex".into(),
                    path: ".agents/skills/release/SKILL.md".into(),
                    scope: "workspace".into(),
                },
                body: "Run tests first.".into(),
            }]);
        let result = get_skill(&ctx, &json!({"name":"release"})).expect("skill");
        assert_eq!(result["content"], "Run tests first.");
    }

    #[test]
    fn list_skills_refreshes_files_created_after_context_initialization() {
        let workspace = tempfile::tempdir().expect("workspace");
        let harness = tempfile::tempdir().expect("harness");
        let ctx = ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
            .expect("context")
            .with_agent_context(AgentContextRuntimeConfig {
                instruction_sources: vec!["codex".into()],
                skill_sources: vec!["custom".into()],
                custom_instruction_paths: String::new(),
                custom_skill_paths: ".agents/skills".into(),
            });

        let empty = list_skills(&ctx, &json!({})).expect("empty skills");
        assert_eq!(empty["count"], 0);

        let skill_dir = workspace.path().join(".agents/skills/live-refresh");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: live-refresh\ndescription: Refresh after startup\n---\nFresh body.",
        )
        .expect("skill");

        let refreshed = list_skills(&ctx, &json!({})).expect("refreshed skills");
        assert_eq!(refreshed["count"], 1);
        assert_eq!(refreshed["skills"][0]["name"], "live-refresh");
    }
}
