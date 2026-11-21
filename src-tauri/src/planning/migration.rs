use serde_json::{Map, Value};

use super::PLANNING_SCHEMA_VERSION;

pub(super) fn normalize_legacy_state(value: &mut Value) {
    let fallback_updated_at = value
        .get("execution")
        .and_then(|execution| execution.get("updated_at"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let Some(root) = value.as_object_mut() else {
        return;
    };
    root.insert(
        "schema_version".into(),
        Value::from(PLANNING_SCHEMA_VERSION),
    );

    normalize_legacy_goals(root, &fallback_updated_at);
    let goal_plan_links = normalize_legacy_plans(root, &fallback_updated_at);
    link_legacy_goal_plans(root, &goal_plan_links);
}

fn normalize_legacy_goals(root: &mut Map<String, Value>, fallback_updated_at: &str) {
    let Some(goals) = root.get_mut("goals").and_then(Value::as_array_mut) else {
        return;
    };
    for goal in goals {
        let Some(goal) = goal.as_object_mut() else {
            continue;
        };
        if !goal.contains_key("objective") {
            let objective = goal
                .get("description")
                .and_then(Value::as_str)
                .or_else(|| goal.get("title").and_then(Value::as_str))
                .unwrap_or("")
                .to_string();
            goal.insert("objective".into(), Value::String(objective));
        }
        goal.entry("created_at")
            .or_insert_with(|| Value::String(fallback_updated_at.to_string()));
        goal.entry("updated_at")
            .or_insert_with(|| Value::String(fallback_updated_at.to_string()));
        goal.entry("plan_ids")
            .or_insert_with(|| Value::Array(Vec::new()));
    }
}

fn normalize_legacy_plans(
    root: &mut Map<String, Value>,
    fallback_updated_at: &str,
) -> Vec<(String, String)> {
    let mut goal_plan_links = Vec::new();
    let Some(plans) = root.get_mut("plans").and_then(Value::as_array_mut) else {
        return goal_plan_links;
    };

    for (plan_index, plan) in plans.iter_mut().enumerate() {
        let Some(plan) = plan.as_object_mut() else {
            continue;
        };
        if !plan.contains_key("objective") {
            let objective = plan
                .get("strategy")
                .and_then(Value::as_str)
                .or_else(|| plan.get("title").and_then(Value::as_str))
                .unwrap_or("")
                .to_string();
            plan.insert("objective".into(), Value::String(objective));
        }
        plan.entry("revision").or_insert_with(|| Value::from(1_u64));
        plan.entry("task_ids")
            .or_insert_with(|| Value::Array(Vec::new()));
        plan.entry("created_at")
            .or_insert_with(|| Value::String(fallback_updated_at.to_string()));
        plan.entry("updated_at")
            .or_insert_with(|| Value::String(fallback_updated_at.to_string()));

        let needs_steps = plan
            .get("steps")
            .and_then(Value::as_array)
            .map(|steps| steps.is_empty())
            .unwrap_or(true);
        if needs_steps {
            let steps = extract_phase_steps(plan, plan_index);
            plan.insert("steps".into(), Value::Array(steps));
        }

        if let (Some(goal_id), Some(plan_id)) = (
            plan.get("goal_id").and_then(Value::as_str),
            plan.get("id").and_then(Value::as_str),
        ) {
            goal_plan_links.push((goal_id.to_string(), plan_id.to_string()));
        }
    }
    goal_plan_links
}

fn extract_phase_steps(plan: &Map<String, Value>, plan_index: usize) -> Vec<Value> {
    let mut steps = Vec::new();
    let Some(phases) = plan.get("phases").and_then(Value::as_array) else {
        return steps;
    };
    for (phase_index, phase) in phases.iter().enumerate() {
        let Some(phase) = phase.as_object() else {
            continue;
        };
        let phase_title = phase.get("title").and_then(Value::as_str).unwrap_or("");
        let Some(tasks) = phase.get("tasks").and_then(Value::as_array) else {
            continue;
        };
        for (task_index, task) in tasks.iter().enumerate() {
            let Some(task) = task.as_object() else {
                continue;
            };
            let id = task
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "legacy-step-{}-{}-{}",
                        plan_index + 1,
                        phase_index + 1,
                        task_index + 1
                    )
                });
            let title = task
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(&id)
                .to_string();
            let status = normalize_legacy_step_status(
                task.get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("pending"),
            );
            steps.push(serde_json::json!({
                "id": id,
                "title": title,
                "status": status,
                "notes": if phase_title.is_empty() {
                    Value::Null
                } else {
                    Value::String(format!("Phase: {phase_title}"))
                }
            }));
        }
    }
    steps
}

fn normalize_legacy_step_status(raw: &str) -> &'static str {
    match raw {
        "completed" | "done" => "completed",
        "in_progress" | "in-progress" | "inprogress" => "in_progress",
        "blocked" => "blocked",
        "skipped" => "skipped",
        _ => "pending",
    }
}

fn link_legacy_goal_plans(root: &mut Map<String, Value>, goal_plan_links: &[(String, String)]) {
    let Some(goals) = root.get_mut("goals").and_then(Value::as_array_mut) else {
        return;
    };
    for goal in goals {
        let Some(goal) = goal.as_object_mut() else {
            continue;
        };
        let Some(goal_id) = goal.get("id").and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        let Some(plan_ids) = goal.get_mut("plan_ids").and_then(Value::as_array_mut) else {
            continue;
        };
        for (_, plan_id) in goal_plan_links.iter().filter(|(id, _)| id == &goal_id) {
            if !plan_ids.iter().any(|value| value.as_str() == Some(plan_id)) {
                plan_ids.push(Value::String(plan_id.clone()));
            }
        }
    }
}
